//! pinkbin-cli — TRAE Skill 用的磁盘清理 CLI。
//!
//! agent 就是 LLM,所以本 CLI 不调任何 AI API,只负责:
//!   1. 扫盘(scanner crate)产出 JSON 树
//!   2. 提取目录元数据摘要(喂给 agent 推理)
//!   3. 列出 / 预览 / 执行 scaffold 清理计划(executor + scaffold crate)
//!
//! 所有输出都是 stdout JSON,错误走 stderr。agent 用 shell 调用并解析 JSON。

mod elevate;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use clap::{Parser, Subcommand};
use include_dir::include_dir;
use pinkbin_executor::{execute, Action, Plan};
use pinkbin_guard::{audit_plan, check_uninstall_string, AuditFlags, AuditResult, Verdict};
use pinkbin_scaffold::{
    compile_all, detect_compiled, expand_env, load_dir, CompiledScaffold, Mode,
    RecycleGranularity, Scaffold,
};
use pinkbin_scanner::{sample_paths, scan_with_stats, Node, ScanOptions, ScanStats};
use serde::Serialize;
use tracing_subscriber::EnvFilter;

/// Compile-time embed of repo-root scaffolds/. Portable binary (no resource_dir,
/// arbitrary cwd) still ships with all packaged scaffolds.
static EMBEDDED_SCAFFOLDS: include_dir::Dir<'static> =
    include_dir!("$CARGO_MANIFEST_DIR/../../scaffolds");

#[derive(Parser)]
#[command(
    name = "pinkbin-cli",
    version,
    about = "Disk scan + scaffold cleanup CLI for TRAE Skill"
)]
struct Cli {
    /// Scaffold search directory. Defaults to <exe_dir>/scaffolds, then ./scaffolds,
    /// then embedded. Can be set to load user-contributed scaffolds.
    #[arg(long, global = true, env = "PINKBIN_SCAFFOLDS_DIR")]
    scaffolds_dir: Option<PathBuf>,

    /// Undo log path. Defaults to <exe_dir>/undo.jsonl. Appended on every execute.
    #[arg(long, global = true, env = "PINKBIN_UNDO_LOG")]
    undo_log: Option<PathBuf>,

    /// Quarantine root. Defaults to <exe_dir>/quarantine.
    #[arg(long, global = true, env = "PINKBIN_QUARANTINE_ROOT")]
    quarantine_root: Option<PathBuf>,

    /// 触发 UAC 提权重启(仅 Windows)。检测到非管理员时,会弹 UAC 提示,
    /// 用户同意后以管理员身份重启本 CLI,执行相同子命令。
    /// 用于 hibernate off / pagefile migrate / restore delete-all
    /// 等需要管理员的操作。agent 用法:`pinkbin-cli --elevate scan C:\`。
    #[arg(long, global = true, default_value_t = false)]
    elevate: bool,

    /// 内部 flag:由 `--elevate` 父进程注入,指定子进程把 stdout 重定向到
    /// 该文件,父进程等待结束后读取并透传。用户/agent 不应直接使用。
    #[arg(long, global = true, hide = true)]
    elevated_output: Option<PathBuf>,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scan a disk/directory and output a JSON tree of sizes.
    Scan {
        /// Path to scan (e.g. "C:\\" or "/home/user").
        path: PathBuf,
        /// Max depth. None = unlimited.
        #[arg(long)]
        max_depth: Option<usize>,
        /// Output format: json (default) | summary (text only).
        #[arg(long, default_value = "json")]
        format: String,
    },

    /// Extract a directory's metadata summary — the JSON to feed an agent
    /// for "what is this folder / can I delete it" reasoning. Never reads
    /// file contents.
    Inspect {
        /// Directory to inspect.
        path: PathBuf,
        /// How many sample file paths to collect (shallowest-first).
        #[arg(long, default_value_t = 20)]
        samples: usize,
    },

    /// List all loaded scaffolds.
    Scaffolds,

    /// Preview which files/dirs a scaffold scope would match (dry-run).
    Preview {
        scaffold_id: String,
        root_path: PathBuf,
        /// Scope id. If omitted, preview all scopes of the scaffold.
        #[arg(long)]
        scope: Option<String>,
        /// Retention filter: only show files older than N days.
        #[arg(long)]
        older_than_days: Option<u32>,
    },

    /// Execute a scaffold cleanup. Default is dry-run; pass --dry-run false to
    /// actually delete. Deleted items go to Recycle Bin by default.
    Execute {
        scaffold_id: String,
        root_path: PathBuf,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        older_than_days: Option<u32>,
        /// true = preview only (default); false = actually delete.
        /// Pass `--dry-run false` to execute for real.
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
        dry_run: bool,
        /// Force hard delete (bypass Recycle Bin). Dangerous — almost never used.
        #[arg(long)]
        hard_delete: bool,
        /// Allow cleanup of system-protected paths (C:\Windows etc).
        /// Almost never used — guard blocks these by default.
        #[arg(long, default_value_t = false)]
        allow_system: bool,
    },

    /// Scan + match scaffolds in one shot. Returns the tree plus a summary
    /// of scaffold matches and top-N largest entries — the agent's main entry.
    Analyze {
        path: PathBuf,
        /// How many top entries to include in the summary.
        #[arg(long, default_value_t = 50)]
        top: usize,
    },

    /// Find duplicate files (three-phase: size → head-hash → full-hash).
    Dedup {
        /// Root directory to scan.
        path: PathBuf,
        /// Minimum file size in bytes (default 1024).
        #[arg(long, default_value_t = 1024)]
        min_size: u64,
        /// Max number of groups to return (0 = all).
        #[arg(long, default_value_t = 50)]
        top: usize,
    },

    /// List all installed applications (reads Windows registry).
    Inventory,

    /// Scan for leftover files after an app uninstall.
    Leftovers {
        /// App name to search for (case-insensitive substring match).
        app_name: String,
    },

    /// Hibernate (休眠文件) 管控:status / on / off / size。
    Hibernate {
        #[command(subcommand)]
        action: HibernateAction,
    },

    /// Pagefile (虚拟内存) 管理:status / migrate。
    Pagefile {
        #[command(subcommand)]
        action: PagefileAction,
    },

    /// System restore points (系统还原点) 管理:status / delete-all。
    Restore {
        #[command(subcommand)]
        action: RestoreAction,
    },

    /// 分析 C 盘上可迁移到其他盘的大软件(只读)。
    Migrate,

    /// 卸载软件(封装 uninstall_string,默认 dry-run,guard 校验)。
    Uninstall {
        /// 软件名称(精确匹配,大小写不敏感)。
        app_name: String,
        /// 尝试静默卸载(对 MSI/InstallShield 追加 /quiet 或 /S)。
        #[arg(long, default_value_t = false)]
        silent: bool,
        /// true = 仅预览(默认);false = 真正执行卸载。
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
        dry_run: bool,
    },

    /// 实际迁移软件到其他盘(robocopy + 注册表 + 快捷方式,不删原目录)。
    MigrateApp {
        /// 源安装路径(如 C:\Program Files\App)。
        source_path: PathBuf,
        /// 目标盘符(单个字母,如 D)。
        target_drive: String,
        /// true = 仅预览(默认);false = 真正执行迁移。
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum HibernateAction {
    /// 查询当前休眠状态(只读)。
    Status,
    /// 关闭休眠(释放 hiberfil.sys,需管理员)。
    Off,
    /// 开启休眠(需管理员)。
    On,
    /// 设置 hiberfil.sys 大小占物理内存的百分比(50-100,需管理员)。
    Size {
        /// 百分比,50-100。
        percent: u32,
    },
}

#[derive(Subcommand)]
enum PagefileAction {
    /// 查询当前 pagefile 配置(只读)。
    Status,
    /// 迁移 pagefile 到目标盘(需管理员,需重启生效)。
    Migrate {
        /// 目标盘符(单个字母,如 D)。
        drive: String,
    },
}

#[derive(Subcommand)]
enum RestoreAction {
    /// 查询系统还原点 / 卷影副本状态(需管理员完整查看)。
    Status,
    /// 删除所有还原点(需管理员,不可逆)。
    DeleteAll,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .with_target(false)
        .init();

    let cli = Cli::parse();

    // ── UAC 提权分流(必须早于任何 println!,否则 Rust stdout 缓存旧 handle) ──
    // 子进程模式:--elevated-output 由父进程注入,把 stdout 重定向到文件。
    if let Some(out_path) = &cli.elevated_output {
        if let Err(e) = elevate::redirect_stdout_to_file(out_path) {
            eprintln!(
                "[elevate] FATAL: 重定向 stdout 到 {:?} 失败: {}",
                out_path, e
            );
            std::process::exit(1);
        }
        // 后续 println! / serde_json::to_writer(stdout) 都会写到 out_path
    }

    // 父进程模式:--elevate 且当前非管理员 → 弹 UAC 重启自己。
    if cli.elevate && !elevate::is_elevated() {
        let tmp = std::env::temp_dir().join(format!(
            "pinkbin-elev-{}-{}.out",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));

        // 子进程参数:跳过 exe 名,移除 --elevate,追加 --elevated-output <tmp>
        let mut child_args: Vec<String> = std::env::args()
            .skip(1)
            .filter(|a| a != "--elevate")
            .collect();
        child_args.push("--elevated-output".into());
        child_args.push(tmp.to_string_lossy().into_owned());

        eprintln!("[elevate] 当前非管理员,正在弹 UAC 提权重启...");

        let exit_code = match elevate::relaunch_elevated(&child_args) {
            Ok(c) => c,
            Err(e) => {
                // UAC 拒绝 / 系统不允许提权 → 给 agent 一个明确 JSON 错误
                let err_json = serde_json::json!({
                    "error": "elevation_failed",
                    "message": format!("提权重启失败: {}", e),
                    "hint": "用户可能拒绝了 UAC 提示,或系统策略禁止提权。可让用户手动以管理员身份运行。"
                });
                println!("{}", err_json);
                let _ = std::fs::remove_file(&tmp);
                return Ok(());
            }
        };

        // 透传子进程 stdout 到本进程 stdout
        if let Ok(content) = std::fs::read_to_string(&tmp) {
            print!("{}", content);
            let _ = std::io::stdout().flush();
        }
        let _ = std::fs::remove_file(&tmp);
        std::process::exit(exit_code);
    }

    let scaffolds_dir = cli.scaffolds_dir.clone().unwrap_or_else(default_scaffolds_dir);
    let undo_log = cli.undo_log.clone().unwrap_or_else(default_undo_log);
    let quarantine_root = cli.quarantine_root.clone().unwrap_or_else(default_quarantine);

    let scaffolds = load_all_scaffolds(&scaffolds_dir);
    tracing::info!("loaded {} scaffolds from {:?}", scaffolds.len(), scaffolds_dir);

    match cli.command {
        Cmd::Scan { path, max_depth, format } => {
            cmd_scan(&path, max_depth, &format, &scaffolds)
        }
        Cmd::Inspect { path, samples } => {
            cmd_inspect(&path, samples)
        }
        Cmd::Scaffolds => {
            cmd_scaffolds(&scaffolds)
        }
        Cmd::Preview { scaffold_id, root_path, scope, older_than_days } => {
            cmd_preview(&scaffolds, &scaffold_id, &root_path, scope, older_than_days)
        }
        Cmd::Execute { scaffold_id, root_path, scope, older_than_days, dry_run, hard_delete, allow_system } => {
            cmd_execute(
                &scaffolds,
                &scaffold_id,
                &root_path,
                scope,
                older_than_days,
                dry_run,
                hard_delete,
                allow_system,
                &undo_log,
                &quarantine_root,
            )
        }
        Cmd::Analyze { path, top } => {
            cmd_analyze(&path, top, &scaffolds)
        }
        Cmd::Dedup { path, min_size, top } => {
            cmd_dedup(&path, min_size, top)
        }
        Cmd::Inventory => {
            cmd_inventory()
        }
        Cmd::Leftovers { app_name } => {
            cmd_leftovers(&app_name)
        }
        Cmd::Hibernate { action } => {
            cmd_hibernate(action)
        }
        Cmd::Pagefile { action } => {
            cmd_pagefile(action)
        }
        Cmd::Restore { action } => {
            cmd_restore(action)
        }
        Cmd::Migrate => {
            cmd_migrate()
        }
        Cmd::Uninstall { app_name, silent, dry_run } => {
            cmd_uninstall(&app_name, silent, dry_run, &undo_log)
        }
        Cmd::MigrateApp { source_path, target_drive, dry_run } => {
            cmd_migrate_app(&source_path, &target_drive, dry_run, &undo_log)
        }
    }
}

// ─────────────────────────── command impls ───────────────────────────

#[derive(Serialize)]
struct ScanOutput {
    root: Node,
    stats: ScanStats,
}

fn cmd_scan(
    path: &Path,
    max_depth: Option<usize>,
    format: &str,
    scaffolds: &[Scaffold],
) -> anyhow::Result<()> {
    let opts = ScanOptions {
        max_depth,
        ..Default::default()
    };
    let compiled = compile_all(scaffolds);
    let (mut node, stats) = scan_with_stats(path.to_path_buf(), opts, |_| {})?;
    tag_and_truncate(&mut node, &compiled, 0);

    if format == "summary" {
        print_tree_summary(&node, 0, 2);
    } else {
        let out = ScanOutput { root: node, stats };
        println!("{}", serde_json::to_string_pretty(&out)?);
    }
    Ok(())
}

#[derive(Serialize)]
struct InspectOutput {
    path: String,
    size_bytes: u64,
    file_count: u64,
    top_extensions: Vec<ExtShareOut>,
    sample_paths: Vec<String>,
    top_children: Vec<ChildOut>,
    scaffold_hint: Option<String>,
}

#[derive(Serialize)]
struct ExtShareOut {
    ext: String,
    bytes: u64,
    count: u64,
}

#[derive(Serialize)]
struct ChildOut {
    name: String,
    size: u64,
    is_dir: bool,
}

fn cmd_inspect(path: &Path, samples: usize) -> anyhow::Result<()> {
    // Full-depth scan so size/file_count/top_extensions are accurate.
    // top_children still comes from node.children (immediate children only).
    let opts = ScanOptions {
        keep_files_per_dir: Some(50),
        ..Default::default()
    };
    let (node, _stats) = scan_with_stats(path.to_path_buf(), opts, |_| {})?;

    let sample_paths = sample_paths(path, samples);

    let top_extensions = node
        .top_extensions
        .iter()
        .map(|e| ExtShareOut {
            ext: e.ext.clone(),
            bytes: e.bytes,
            count: e.count,
        })
        .collect();

    let top_children = node
        .children
        .iter()
        .take(20)
        .map(|c| ChildOut {
            name: c.name.clone(),
            size: c.size,
            is_dir: c.is_dir,
        })
        .collect();

    let out = InspectOutput {
        path: path.to_string_lossy().to_string(),
        size_bytes: node.size,
        file_count: node.file_count,
        top_extensions,
        sample_paths,
        top_children,
        // scaffold_hint left to caller (agent can run `scaffolds` + path matching
        // separately; keeping inspect dependency-free on scaffold state)
        scaffold_hint: None,
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn cmd_scaffolds(scaffolds: &[Scaffold]) -> anyhow::Result<()> {
    let out: Vec<_> = scaffolds
        .iter()
        .map(|s| serde_json::json!({
            "id": s.id,
            "name": s.name,
            "risk": s.risk,
            "disclaimer": s.disclaimer,
            "detect": s.detect,
            "scopes": s.scopes.iter().map(|sc| {
                serde_json::json!({
                    "id": sc.id,
                    "label": sc.label,
                    "glob": sc.glob,
                    "mode": sc.mode,
                    "recycle_granularity": sc.recycle_granularity,
                })
            }).collect::<Vec<_>>(),
        }))
        .collect();
    println!("{}", serde_json::to_string_pretty(&serde_json::Value::Array(out))?);
    Ok(())
}

#[derive(Serialize)]
struct PreviewOutput {
    scaffold_id: String,
    root_path: String,
    matched: Vec<MatchedItem>,
    total_bytes: u64,
    total_count: u64,
    /// guard 拦截的路径(执行时会拒绝)
    guard_blocked: Vec<serde_json::Value>,
    /// guard 警告的路径(用户数据等,可执行但需确认)
    guard_warnings: Vec<serde_json::Value>,
}

#[derive(Serialize)]
struct MatchedItem {
    path: String,
    size: u64,
    is_dir: bool,
    scope_id: String,
    action: String,
}

fn cmd_preview(
    scaffolds: &[Scaffold],
    scaffold_id: &str,
    root_path: &Path,
    scope_filter: Option<String>,
    older_than_days: Option<u32>,
) -> anyhow::Result<()> {
    let scaffold = scaffolds
        .iter()
        .find(|s| s.id == scaffold_id)
        .ok_or_else(|| anyhow::anyhow!("scaffold not found: {scaffold_id}"))?;

    let scopes: Vec<_> = scaffold
        .scopes
        .iter()
        .filter(|s| scope_filter.as_ref().map_or(true, |f| &s.id == f))
        .collect();

    if scopes.is_empty() {
        eprintln!("no scopes matched the filter");
        return Ok(());
    }

    // ── system-cmd 分流:不走文件 glob,不走 guard 文件审计 ──
    if scopes.iter().all(|s| matches!(s.mode, Mode::SystemCmd)) {
        return cmd_preview_system_cmd(scaffold_id, &scopes, root_path);
    }
    if scopes.iter().any(|s| matches!(s.mode, Mode::SystemCmd)) {
        anyhow::bail!("system_cmd scope 不能与文件 scope 混在同一个 preview 调用中");
    }

    let mut matched: Vec<MatchedItem> = Vec::new();
    let mut total_bytes: u64 = 0;

    for scope in &scopes {
        let action = action_for(scope.mode, scope.recycle_granularity, false);
        let action_str = match action {
            Action::Recycle => "recycle",
            Action::Quarantine => "quarantine",
            Action::Delete => "delete",
        };
        let items = match scope.recycle_granularity {
            RecycleGranularity::Directory => {
                find_matching_dirs(root_path, scope, older_than_days)
            }
            RecycleGranularity::File => {
                find_matching_files(root_path, scope, older_than_days)
            }
        };
        for (p, size, is_dir) in items {
            total_bytes = total_bytes.saturating_add(size);
            matched.push(MatchedItem {
                path: p.to_string_lossy().replace('\\', "/"),
                size,
                is_dir,
                scope_id: scope.id.clone(),
                action: action_str.to_string(),
            });
        }
    }

    // guard 审计:预览阶段就标出哪些会被拦,让用户提前知情
    let paths: Vec<PathBuf> = matched
        .iter()
        .map(|m| PathBuf::from(m.path.replace('/', std::path::MAIN_SEPARATOR_STR)))
        .collect();
    let (blocked, warnings) = match audit_plan(&paths, &AuditFlags::default()) {
        AuditResult::Approved { warnings } => (Vec::new(), warnings),
        AuditResult::Rejected { blocked, warnings } => (blocked, warnings),
    };

    let total_count = matched.len() as u64;
    let out = PreviewOutput {
        scaffold_id: scaffold_id.to_string(),
        root_path: root_path.to_string_lossy().to_string(),
        matched,
        total_bytes,
        total_count,
        guard_blocked: blocked.iter().map(|n| serde_json::json!({
            "path": n.path,
            "reason": n.reason,
        })).collect(),
        guard_warnings: warnings.iter().map(|n| serde_json::json!({
            "path": n.path,
            "reason": n.reason,
        })).collect(),
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn cmd_execute(
    scaffolds: &[Scaffold],
    scaffold_id: &str,
    root_path: &Path,
    scope_filter: Option<String>,
    older_than_days: Option<u32>,
    dry_run: bool,
    hard_delete: bool,
    allow_system: bool,
    undo_log: &Path,
    quarantine_root: &Path,
) -> anyhow::Result<()> {
    let scaffold = scaffolds
        .iter()
        .find(|s| s.id == scaffold_id)
        .ok_or_else(|| anyhow::anyhow!("scaffold not found: {scaffold_id}"))?;

    let scopes: Vec<_> = scaffold
        .scopes
        .iter()
        .filter(|s| scope_filter.as_ref().map_or(true, |f| &s.id == f))
        .collect();

    if scopes.is_empty() {
        eprintln!("no scopes matched the filter");
        return Ok(());
    }

    // ── system-cmd 分流:不走文件 glob,不走 guard 文件审计 ──
    if scopes.iter().all(|s| matches!(s.mode, Mode::SystemCmd)) {
        return cmd_execute_system_cmd(scaffold_id, &scopes, root_path, dry_run, undo_log);
    }
    if scopes.iter().any(|s| matches!(s.mode, Mode::SystemCmd)) {
        anyhow::bail!("system_cmd scope 不能与文件 scope 混在同一个 execute 调用中");
    }

    let mut all_paths: Vec<PathBuf> = Vec::new();
    for scope in &scopes {
        let items = match scope.recycle_granularity {
            RecycleGranularity::Directory => {
                find_matching_dirs(root_path, scope, older_than_days)
            }
            RecycleGranularity::File => {
                find_matching_files(root_path, scope, older_than_days)
            }
        };
        for (p, _, _) in items {
            all_paths.push(p);
        }
    }

    if all_paths.is_empty() {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "scaffold_id": scaffold_id,
            "executed": false,
            "reason": "no matching paths",
            "matched_count": 0,
        }))?);
        return Ok(());
    }

    // ── guard 审计门:执行前必审,被拦就整体拒绝 ──
    let flags = AuditFlags {
        allow_system,
        ..Default::default()
    };
    let (blocked, warnings) = match audit_plan(&all_paths, &flags) {
        AuditResult::Approved { warnings } => (Vec::new(), warnings),
        AuditResult::Rejected { blocked, warnings } => (blocked, warnings),
    };

    if !blocked.is_empty() {
        // 拒绝执行,报告所有被拦路径
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "scaffold_id": scaffold_id,
                "executed": false,
                "reason": "guard blocked protected paths",
                "blocked_count": blocked.len(),
                "blocked": blocked.iter().map(|n| serde_json::json!({
                    "path": n.path,
                    "reason": n.reason,
                })).collect::<Vec<_>>(),
                "warning_count": warnings.len(),
                "hint": "如需清理系统路径,请加 --allow-system flag(危险)",
            }))?
        );
        return Ok(());
    }

    // Directory granularity is locked to Recycle — recovering a removed
    // directory (conda env, node_modules) is too costly to risk hard delete.
    let any_dir_scope = scopes
        .iter()
        .any(|s| s.recycle_granularity == RecycleGranularity::Directory);
    let action = if hard_delete && !any_dir_scope {
        Action::Delete
    } else if hard_delete && any_dir_scope {
        return Err(anyhow::anyhow!(
            "hard_delete not allowed on directory-granularity scopes (would permanently remove entire dirs)"
        ));
    } else {
        let modes: Vec<Mode> = scopes.iter().map(|s| s.mode).collect();
        if modes.iter().all(|&m| matches!(m, Mode::Delete)) {
            Action::Delete
        } else if modes.iter().all(|&m| matches!(m, Mode::Quarantine)) {
            Action::Quarantine
        } else {
            Action::Recycle
        }
    };

    // Stat total bytes BEFORE execute — files are gone after a real delete.
    let total_bytes: u64 = all_paths
        .iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();

    let plan = Plan {
        action,
        paths: all_paths.clone(),
        reason: format!("pinkbin-cli scaffold {scaffold_id}"),
    };
    let undo_entries = execute(&plan, dry_run, undo_log, quarantine_root)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "scaffold_id": scaffold_id,
            "executed": !dry_run,
            "dry_run": dry_run,
            "action": match action {
                Action::Recycle => "recycle",
                Action::Quarantine => "quarantine",
                Action::Delete => "delete",
            },
            "matched_count": all_paths.len(),
            "total_bytes": total_bytes,
            "undo_entries": undo_entries.len(),
            "undo_log": undo_log.to_string_lossy(),
            "guard_warnings": warnings.iter().map(|n| serde_json::json!({
                "path": n.path,
                "reason": n.reason,
            })).collect::<Vec<_>>(),
        }))?
    );
    Ok(())
}

#[derive(Serialize)]
struct AnalyzeOutput {
    root: Node,
    stats: ScanStats,
    summary: AnalyzeSummary,
}

#[derive(Serialize)]
struct AnalyzeSummary {
    top_dirs: Vec<TopEntry>,
    scaffold_matches: Vec<ScaffoldMatch>,
}

#[derive(Serialize)]
struct TopEntry {
    path: String,
    name: String,
    size: u64,
    is_dir: bool,
    scaffold_id: Option<String>,
}

#[derive(Serialize)]
struct ScaffoldMatch {
    scaffold_id: String,
    path: String,
    size: u64,
    file_count: u64,
}

fn cmd_analyze(path: &Path, top: usize, scaffolds: &[Scaffold]) -> anyhow::Result<()> {
    let compiled = compile_all(scaffolds);
    let opts = ScanOptions::default();
    let (mut node, stats) = scan_with_stats(path.to_path_buf(), opts, |_| {})?;
    tag_and_truncate(&mut node, &compiled, 0);

    // Flatten top-N by size (depth >= 1 so we skip the root itself)
    let mut all: Vec<(Node, usize)> = Vec::new();
    flatten(&node, 1, &mut all);
    all.sort_by(|a, b| b.0.size.cmp(&a.0.size));
    let top_dirs: Vec<TopEntry> = all
        .iter()
        .take(top)
        .map(|(n, _)| TopEntry {
            path: n.path.clone(),
            name: n.name.clone(),
            size: n.size,
            is_dir: n.is_dir,
            scaffold_id: n.scaffold_id.clone(),
        })
        .collect();

    // Collect scaffold matches
    let mut matches: Vec<ScaffoldMatch> = Vec::new();
    collect_scaffold_matches(&node, &mut matches);
    matches.sort_by(|a, b| b.size.cmp(&a.size));

    let out = AnalyzeOutput {
        root: node,
        stats,
        summary: AnalyzeSummary {
            top_dirs,
            scaffold_matches: matches,
        },
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

// ─────────────────────────── dedup / inventory ───────────────────────────

fn cmd_dedup(path: &Path, min_size: u64, top: usize) -> anyhow::Result<()> {
    let opts = pinkbin_dedup::DedupOptions {
        min_size,
        ..Default::default()
    };
    let groups = pinkbin_dedup::find_duplicates(path, &opts);

    let total_waste: u64 = groups.iter().map(|g| g.waste_bytes).sum();
    let total_dup_files: usize = groups.iter().map(|g| g.files.len()).sum();

    let groups = if top > 0 {
        groups.into_iter().take(top).collect::<Vec<_>>()
    } else {
        groups
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "root": path.to_string_lossy(),
            "total_groups": groups.len(),
            "total_dup_files": total_dup_files,
            "total_waste_bytes": total_waste,
            "groups": groups,
        }))?
    );
    Ok(())
}

fn cmd_inventory() -> anyhow::Result<()> {
    let apps = pinkbin_inventory::list_installed_apps();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "total": apps.len(),
            "apps": apps,
        }))?
    );
    Ok(())
}

fn cmd_leftovers(app_name: &str) -> anyhow::Result<()> {
    let result = pinkbin_inventory::find_leftovers(app_name);
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

// ─────────────────────────── system-ops (hibernate/pagefile/restore/migrate) ───────────────────────────

fn cmd_hibernate(action: HibernateAction) -> anyhow::Result<()> {
    let result = match action {
        HibernateAction::Status => serde_json::to_value(pinkbin_system_ops::hibernate_status())?,
        HibernateAction::Off => serde_json::to_value(pinkbin_system_ops::hibernate_off())?,
        HibernateAction::On => serde_json::to_value(pinkbin_system_ops::hibernate_on())?,
        HibernateAction::Size { percent } => {
            serde_json::to_value(pinkbin_system_ops::hibernate_set_size(percent))?
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn cmd_pagefile(action: PagefileAction) -> anyhow::Result<()> {
    let result = match action {
        PagefileAction::Status => serde_json::to_value(pinkbin_system_ops::pagefile_status())?,
        PagefileAction::Migrate { drive } => {
            serde_json::to_value(pinkbin_system_ops::pagefile_migrate(&drive))?
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn cmd_restore(action: RestoreAction) -> anyhow::Result<()> {
    let result = match action {
        RestoreAction::Status => serde_json::to_value(pinkbin_system_ops::restore_status())?,
        RestoreAction::DeleteAll => serde_json::to_value(pinkbin_system_ops::restore_delete_all())?,
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn cmd_migrate() -> anyhow::Result<()> {
    let result = pinkbin_system_ops::analyze_migratable_apps();
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

// ─────────────────────────── uninstall / migrate-app ───────────────────────────

#[derive(Serialize)]
struct UninstallOutput {
    app_name_query: String,
    matched: Vec<UninstallCandidate>,
    /// 当且仅当 matched.len()==1 且非 dry_run 时才有此字段
    #[serde(skip_serializing_if = "Option::is_none")]
    execution: Option<UninstallExecution>,
    guard_verdict: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    guard_reason: Option<String>,
    dry_run: bool,
    undo_log: String,
}

#[derive(Serialize)]
struct UninstallCandidate {
    name: String,
    version: Option<String>,
    publisher: Option<String>,
    install_path: Option<String>,
    uninstall_string: String,
    is_per_user: bool,
}

#[derive(Serialize)]
struct UninstallExecution {
    command_run: String,
    executed: bool,
    requires_admin: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn cmd_uninstall(
    app_name: &str,
    silent: bool,
    dry_run: bool,
    undo_log: &Path,
) -> anyhow::Result<()> {
    let apps = pinkbin_inventory::list_installed_apps();
    // 精确匹配优先(大小写不敏感),无精确匹配则取 substring 候选
    let lower = app_name.to_lowercase();
    let mut exact: Vec<&pinkbin_inventory::InstalledApp> = apps
        .iter()
        .filter(|a| a.name.to_lowercase() == lower)
        .collect();
    let candidates: Vec<&pinkbin_inventory::InstalledApp> = if !exact.is_empty() {
        exact.drain(..).collect()
    } else {
        apps.iter().filter(|a| a.name.to_lowercase().contains(&lower)).collect()
    };

    let matched: Vec<UninstallCandidate> = candidates
        .iter()
        .map(|a| UninstallCandidate {
            name: a.name.clone(),
            version: a.version.clone(),
            publisher: a.publisher.clone(),
            install_path: a.install_path.clone(),
            uninstall_string: a.uninstall_string.clone().unwrap_or_default(),
            is_per_user: a.is_per_user,
        })
        .collect();

    // guard 校验卸载串
    let (verdict_str, guard_reason) = if matched.len() == 1 {
        match check_uninstall_string(&matched[0].uninstall_string) {
            Verdict::Block(r) => ("block".to_string(), Some(r)),
            Verdict::Warn(r) => ("warn".to_string(), Some(r)),
            Verdict::Pass => ("pass".to_string(), None),
            Verdict::NeedsReview => ("needs_review".to_string(), None),
        }
    } else {
        ("skip".to_string(), None)
    };

    // 执行条件:唯一匹配 + guard 非 Block + 非 dry_run
    let mut execution: Option<UninstallExecution> = None;
    if matched.len() == 1 && verdict_str != "block" && !dry_run {
        let raw = &matched[0].uninstall_string;
        // 构造实际命令:silent 时尝试追加 /quiet(MSI)或 /S(NSIS)
        let command_run = if silent {
            if raw.to_lowercase().contains("msiexec") {
                format!("{} /quiet /noreboot", raw)
            } else if !raw.to_lowercase().contains("/s") {
                format!("{} /S", raw)
            } else {
                raw.clone()
            }
        } else {
            raw.clone()
        };

        let requires_admin = !matched[0].is_per_user;
        let result = run_uninstall_command(&command_run, requires_admin);
        execution = Some(UninstallExecution {
            command_run,
            executed: result.0,
            requires_admin,
            exit_code: result.1,
            stdout: result.2,
            stderr: result.3,
            error: result.4,
        });

        // 写 undo log
        write_uninstall_undo_log(undo_log, candidates[0], &execution.as_ref().unwrap())?;
    } else if matched.len() == 1 && verdict_str != "block" && dry_run {
        let raw = &matched[0].uninstall_string;
        let command_run = if silent {
            if raw.to_lowercase().contains("msiexec") {
                format!("{} /quiet /noreboot", raw)
            } else if !raw.to_lowercase().contains("/s") {
                format!("{} /S", raw)
            } else {
                raw.clone()
            }
        } else {
            raw.clone()
        };
        execution = Some(UninstallExecution {
            command_run,
            executed: false,
            requires_admin: !matched[0].is_per_user,
            exit_code: None,
            stdout: "[dry-run] 将执行上述卸载命令".to_string(),
            stderr: String::new(),
            error: None,
        });
    }

    let out = UninstallOutput {
        app_name_query: app_name.to_string(),
        matched,
        execution,
        guard_verdict: verdict_str,
        guard_reason,
        dry_run,
        undo_log: undo_log.to_string_lossy().to_string(),
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

/// 运行卸载命令。返回 (executed, exit_code, stdout, stderr, error)。
fn run_uninstall_command(command: &str, _requires_admin: bool) -> (bool, Option<i32>, String, String, Option<String>) {
    if !cfg!(windows) {
        return (false, None, String::new(), String::new(),
                Some("卸载命令只能在 Windows 上执行(当前非 Windows 环境)".to_string()));
    }
    match std::process::Command::new("cmd").args(["/C", command]).output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let exit_code = out.status.code();
            let executed = out.status.success();
            let error = if executed { None } else { Some(format!("命令退出码: {:?}", exit_code)) };
            (executed, exit_code, stdout, stderr, error)
        }
        Err(e) => (false, None, String::new(), String::new(), Some(format!("命令启动失败: {}", e))),
    }
}

fn write_uninstall_undo_log(
    undo_log: &Path,
    app: &pinkbin_inventory::InstalledApp,
    exec: &UninstallExecution,
) -> anyhow::Result<()> {
    if let Some(parent) = undo_log.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(undo_log)?;
    let entry = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "type": "uninstall",
        "app_name": app.name,
        "version": app.version,
        "install_path": app.install_path,
        "uninstall_string": app.uninstall_string,
        "command_run": exec.command_run,
        "executed": exec.executed,
        "exit_code": exec.exit_code,
    });
    writeln!(f, "{}", entry)?;
    Ok(())
}

// ── migrate-app ──

#[derive(Serialize)]
struct MigrateAppOutput {
    source_path: String,
    target_drive: String,
    target_path: String,
    source_size_bytes: u64,
    dry_run: bool,
    guard_verdict: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    guard_reason: Option<String>,
    /// 迁移步骤(每步含 description + status)
    steps: Vec<MigrateStep>,
    undo_log: String,
}

#[derive(Serialize)]
struct MigrateStep {
    step: String,
    description: String,
    executed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn cmd_migrate_app(
    source_path: &Path,
    target_drive: &str,
    dry_run: bool,
    undo_log: &Path,
) -> anyhow::Result<()> {
    let drive = target_drive.trim().trim_end_matches(':').trim_end_matches('\\');
    if drive.len() != 1 || !drive.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
        anyhow::bail!("target_drive 必须是单个盘符(如 D),收到: {}", target_drive);
    }
    let upper = drive.to_uppercase();
    let target_path_str = format!("{}:\\{}", upper, source_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "MigratedApp".to_string()));
    let _target_path = PathBuf::from(&target_path_str);

    // 源目录大小
    let source_size = dir_size_local(source_path);

    // guard 校验:源路径不能在保护列表(System32/Windows 等)
    let verdict = pinkbin_guard::check(source_path);
    let (guard_verdict, guard_reason) = match verdict {
        Verdict::Block(r) => ("block".to_string(), Some(r)),
        Verdict::Warn(r) => ("warn".to_string(), Some(r)),
        Verdict::Pass => ("pass".to_string(), None),
        Verdict::NeedsReview => ("needs_review".to_string(), None),
    };

    let mut steps: Vec<MigrateStep> = Vec::new();

    if guard_verdict == "block" {
        // guard 拦截,直接返回,不执行任何步骤
        let out = MigrateAppOutput {
            source_path: source_path.to_string_lossy().to_string(),
            target_drive: upper.clone(),
            target_path: target_path_str,
            source_size_bytes: source_size,
            dry_run,
            guard_verdict,
            guard_reason,
            steps: Vec::new(),
            undo_log: undo_log.to_string_lossy().to_string(),
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    // 步骤 1:robocopy 复制目录(/E 复制子目录含空目录,/COPYALL 复制权限)
    let robocopy_cmd = format!(
        "robocopy \"{}\" \"{}\" /E /COPYALL /R:1 /W:1 /NFL /NDL /NP",
        source_path.to_string_lossy(),
        target_path_str
    );
    steps.push(execute_migrate_step("robocopy", &format!("复制目录到 {}", target_path_str), &robocopy_cmd, dry_run));

    // 步骤 2:修改注册表 InstallLocation(查找含源路径的 Uninstall 项)
    // 用 PowerShell 查找并修改
    let ps_cmd = format!(
        "powershell -Command \"Get-ChildItem 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall','HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall','HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall' | ForEach-Object {{ $p = Get-ItemProperty $_.PSPath; if ($p.InstallLocation -like '{}*') {{ Set-ItemProperty -Path $_.PSPath -Name InstallLocation -Value '{}' }} }}\"",
        source_path.to_string_lossy().replace('\\', "\\\\"),
        target_path_str.replace('\\', "\\\\")
    );
    steps.push(execute_migrate_step("update_registry", "修改注册表 InstallLocation 指向新路径", &ps_cmd, dry_run));

    // 步骤 3:更新开始菜单快捷方式(扫描 .lnk 并修改 Target)
    let lnk_cmd = format!(
        "powershell -Command \"$shell = New-Object -ComObject WScript.Shell; Get-ChildItem -Path 'C:\\ProgramData\\Microsoft\\Windows\\Start Menu','{}\\Microsoft\\Windows\\Start Menu' -Recurse -Filter *.lnk | ForEach-Object {{ $s = $shell.CreateShortcut($_.FullName); if ($s.TargetPath -like '{}*') {{ $s.TargetPath = $s.TargetPath.Replace('{}', '{}'); $s.Save() }} }}\"",
        std::env::var("APPDATA").unwrap_or_default(),
        source_path.to_string_lossy(),
        source_path.to_string_lossy(),
        target_path_str
    );
    steps.push(execute_migrate_step("update_shortcuts", "更新开始菜单快捷方式指向新路径", &lnk_cmd, dry_run));

    // 不删除原目录(让用户验证新路径可用后再手动删)
    steps.push(MigrateStep {
        step: "keep_source".to_string(),
        description: "保留原目录(验证新路径可用后手动删除,避免迁移失败导致软件损坏)".to_string(),
        executed: true,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        error: None,
    });

    // 写 undo log
    if !dry_run {
        write_migrate_undo_log(undo_log, source_path, &target_path_str, &steps)?;
    }

    let out = MigrateAppOutput {
        source_path: source_path.to_string_lossy().to_string(),
        target_drive: upper,
        target_path: target_path_str,
        source_size_bytes: source_size,
        dry_run,
        guard_verdict,
        guard_reason,
        steps,
        undo_log: undo_log.to_string_lossy().to_string(),
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn execute_migrate_step(step: &str, description: &str, command: &str, dry_run: bool) -> MigrateStep {
    if dry_run {
        return MigrateStep {
            step: step.to_string(),
            description: description.to_string(),
            executed: false,
            exit_code: None,
            stdout: format!("[dry-run] 将运行: {}", command),
            stderr: String::new(),
            error: None,
        };
    }
    if !cfg!(windows) {
        return MigrateStep {
            step: step.to_string(),
            description: description.to_string(),
            executed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some("此命令只能在 Windows 上执行(当前非 Windows 环境)".to_string()),
        };
    }
    match std::process::Command::new("cmd").args(["/C", command]).output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let exit_code = out.status.code();
            // robocopy 退出码 0-7 为成功,8+ 为失败
            let success = if step == "robocopy" {
                exit_code.map(|c| c < 8).unwrap_or(false)
            } else {
                out.status.success()
            };
            MigrateStep {
                step: step.to_string(),
                description: description.to_string(),
                executed: success,
                exit_code,
                stdout,
                stderr,
                error: if success { None } else { Some(format!("退出码: {:?}", exit_code)) },
            }
        }
        Err(e) => MigrateStep {
            step: step.to_string(),
            description: description.to_string(),
            executed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(format!("命令启动失败: {}", e)),
        },
    }
}

fn write_migrate_undo_log(
    undo_log: &Path,
    source_path: &Path,
    target_path: &str,
    steps: &[MigrateStep],
) -> anyhow::Result<()> {
    if let Some(parent) = undo_log.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(undo_log)?;
    let entry = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "type": "migrate_app",
        "source_path": source_path.to_string_lossy(),
        "target_path": target_path,
        "steps": steps.iter().map(|s| serde_json::json!({
            "step": s.step, "executed": s.executed, "exit_code": s.exit_code,
        })).collect::<Vec<_>>(),
        "note": "原目录未删除,验证新路径可用后可手动删除;如需回滚,删除目标目录并恢复注册表",
    });
    writeln!(f, "{}", entry)?;
    Ok(())
}

fn dir_size_local(dir: &Path) -> u64 {
    if !dir.exists() {
        return 0;
    }
    let mut total: u64 = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&p) {
            for entry in entries.flatten() {
                if let Ok(md) = entry.metadata() {
                    if md.is_file() {
                        total = total.saturating_add(md.len());
                    } else if md.is_dir() {
                        stack.push(entry.path());
                    }
                }
            }
        }
    }
    total
}

// ─────────────────────────── system-cmd scope ───────────────────────────

#[derive(Serialize)]
struct SystemCmdPreviewOutput {
    scaffold_id: String,
    mode: String,
    scopes: Vec<SystemCmdScopePreview>,
}

#[derive(Serialize)]
struct SystemCmdScopePreview {
    scope_id: String,
    label: String,
    command: String,
    requires_admin: bool,
    /// glob 字段在 system-cmd 模式下被复用为"目标目录"——preview 时扫描其当前大小(只读)。
    target_path: String,
    target_exists: bool,
    target_size_bytes: u64,
    target_file_count: u64,
}

fn cmd_preview_system_cmd(
    scaffold_id: &str,
    scopes: &[&pinkbin_scaffold::Scope],
    root_path: &Path,
) -> anyhow::Result<()> {
    let mut out_scopes: Vec<SystemCmdScopePreview> = Vec::new();

    for scope in scopes {
        let command = scope.command.clone().unwrap_or_default();
        let requires_admin = scope.requires_admin.unwrap_or(false);

        // glob 在 system-cmd 模式下是目标目录路径(如 %WINDIR%/WinSxS)
        // 优先用 scope.glob(展开环境变量),回退到 root_path
        let target_str = if scope.glob.is_empty() {
            root_path.to_string_lossy().to_string()
        } else {
            expand_env(&scope.glob)
        };
        let target_path = PathBuf::from(&target_str);
        let (target_exists, target_size_bytes, target_file_count) = if target_path.exists() {
            let (node, _stats) = scan_with_stats(
                target_path.clone(),
                ScanOptions::default(),
                |_| {},
            ).unwrap_or_else(|_| (Node {
                path: target_str.clone(),
                name: String::new(),
                size: 0,
                file_count: 0,
                is_dir: true,
                children: Vec::new(),
                top_extensions: Vec::new(),
                scaffold_id: None,
            }, ScanStats::default()));
            (true, node.size, node.file_count)
        } else {
            (false, 0, 0)
        };

        out_scopes.push(SystemCmdScopePreview {
            scope_id: scope.id.clone(),
            label: scope.label.clone(),
            command,
            requires_admin,
            target_path: target_str.replace('\\', "/"),
            target_exists,
            target_size_bytes,
            target_file_count,
        });
    }

    let out = SystemCmdPreviewOutput {
        scaffold_id: scaffold_id.to_string(),
        mode: "system_cmd".to_string(),
        scopes: out_scopes,
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

#[derive(Serialize)]
struct SystemCmdExecuteOutput {
    scaffold_id: String,
    mode: String,
    dry_run: bool,
    results: Vec<SystemCmdResult>,
    undo_log: String,
}

#[derive(Serialize)]
struct SystemCmdResult {
    scope_id: String,
    command: String,
    executed: bool,
    requires_admin: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    error: Option<String>,
}

fn cmd_execute_system_cmd(
    scaffold_id: &str,
    scopes: &[&pinkbin_scaffold::Scope],
    _root_path: &Path,
    dry_run: bool,
    undo_log: &Path,
) -> anyhow::Result<()> {
    let mut results: Vec<SystemCmdResult> = Vec::new();

    for scope in scopes {
        let command = scope.command.clone().unwrap_or_default();
        let requires_admin = scope.requires_admin.unwrap_or(false);

        if command.is_empty() {
            results.push(SystemCmdResult {
                scope_id: scope.id.clone(),
                command: String::new(),
                executed: false,
                requires_admin,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                error: Some("scope.command 为空".to_string()),
            });
            continue;
        }

        if dry_run {
            results.push(SystemCmdResult {
                scope_id: scope.id.clone(),
                command: command.clone(),
                executed: false,
                requires_admin,
                exit_code: None,
                stdout: format!("[dry-run] 将运行: {}", command),
                stderr: if requires_admin {
                    "[dry-run] 注意: 此命令需要管理员权限".to_string()
                } else {
                    String::new()
                },
                error: None,
            });
            continue;
        }

        // 真正执行:Windows 用 cmd /C,其他平台无法运行 Windows 命令
        let output = if cfg!(windows) {
            std::process::Command::new("cmd")
                .args(["/C", &command])
                .output()
        } else {
            // 非 Windows:无法运行 DISM/takeown 等 Windows 命令,直接报错
            results.push(SystemCmdResult {
                scope_id: scope.id.clone(),
                command: command.clone(),
                executed: false,
                requires_admin,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                error: Some("system_cmd 只能在 Windows 上执行(当前非 Windows 环境)".to_string()),
            });
            continue;
        };

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let exit_code = out.status.code();
                let executed = out.status.success();
                results.push(SystemCmdResult {
                    scope_id: scope.id.clone(),
                    command: command.clone(),
                    executed,
                    requires_admin,
                    exit_code,
                    stdout,
                    stderr,
                    error: if executed { None } else { Some(format!("命令退出码: {:?}", exit_code)) },
                });
            }
            Err(e) => {
                results.push(SystemCmdResult {
                    scope_id: scope.id.clone(),
                    command: command.clone(),
                    executed: false,
                    requires_admin,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: Some(format!("命令启动失败: {}", e)),
                });
            }
        }
    }

    // 写 undo log(记录执行了哪些系统命令)
    write_system_cmd_undo_log(undo_log, scaffold_id, &results)?;

    let out = SystemCmdExecuteOutput {
        scaffold_id: scaffold_id.to_string(),
        mode: "system_cmd".to_string(),
        dry_run,
        results,
        undo_log: undo_log.to_string_lossy().to_string(),
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn write_system_cmd_undo_log(
    undo_log: &Path,
    scaffold_id: &str,
    results: &[SystemCmdResult],
) -> anyhow::Result<()> {
    if let Some(parent) = undo_log.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(undo_log)?;
    for r in results {
        if r.executed || r.dry_run_is_actionable() {
            let entry = serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "type": "system_cmd",
                "scaffold_id": scaffold_id,
                "scope_id": r.scope_id,
                "command": r.command,
                "executed": r.executed,
                "requires_admin": r.requires_admin,
                "exit_code": r.exit_code,
            });
            writeln!(f, "{}", entry)?;
        }
    }
    Ok(())
}

// dry_run 结果也算可记录的 undo 条目(用于审计追踪)
impl SystemCmdResult {
    fn dry_run_is_actionable(&self) -> bool {
        !self.command.is_empty() && self.error.is_none()
    }
}

// ─────────────────────────── helpers ───────────────────────────

fn default_scaffolds_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(Path::new(".")).join("scaffolds");
        if dir.exists() {
            return dir;
        }
    }
    PathBuf::from("scaffolds")
}

fn default_undo_log() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(Path::new("."));
        return dir.join("undo.jsonl");
    }
    PathBuf::from("undo.jsonl")
}

fn default_quarantine() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(Path::new("."));
        return dir.join("quarantine");
    }
    PathBuf::from("quarantine")
}

fn load_all_scaffolds(dir: &Path) -> Vec<Scaffold> {
    let mut by_id: HashMap<String, Scaffold> = HashMap::new();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if dir.exists() {
        candidates.push(dir.to_path_buf());
    }
    candidates.push(PathBuf::from("scaffolds"));
    candidates.push(PathBuf::from("../../scaffolds"));
    candidates.push(PathBuf::from("../../../scaffolds"));

    for p in &candidates {
        if !p.exists() {
            continue;
        }
        if let Ok(v) = load_dir(p) {
            for s in v {
                by_id.insert(s.id.clone(), s);
            }
        }
    }
    // Lowest-priority fallback: embedded scaffolds
    for f in EMBEDDED_SCAFFOLDS.files() {
        if f.path().extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Some(text) = f.contents_utf8() else { continue };
        match toml::from_str::<Scaffold>(text) {
            Ok(s) => {
                by_id.entry(s.id.clone()).or_insert(s);
            }
            Err(e) => tracing::warn!("embedded scaffold parse error in {:?}: {}", f.path(), e),
        }
    }
    let mut out: Vec<Scaffold> = by_id.into_values().collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Walks `node` in place, filling `scaffold_id` for directories and truncating
/// children by depth-based caps. Same logic as Pinkbin's tag_and_truncate.
fn tag_and_truncate(node: &mut Node, compiled: &[CompiledScaffold], depth: usize) {
    if node.is_dir {
        node.scaffold_id = detect_compiled(compiled, Path::new(&node.path));
    }
    for c in &mut node.children {
        tag_and_truncate(c, compiled, depth + 1);
    }
    let cap = if depth < 2 { 100 } else if depth < 4 { 50 } else { 20 };
    if node.children.len() > cap {
        let (tagged, rest): (Vec<Node>, Vec<Node>) =
            node.children.drain(..).partition(|n| n.scaffold_id.is_some());
        let mut survivors: Vec<Node> = tagged.into_iter().take(cap).collect();
        let need = cap.saturating_sub(survivors.len());
        survivors.extend(rest.into_iter().take(need));
        survivors.sort_by_key(|c| std::cmp::Reverse(c.size));
        node.children = survivors;
    }
}

fn flatten(node: &Node, depth: usize, out: &mut Vec<(Node, usize)>) {
    if depth > 0 {
        out.push((node.clone(), depth));
    }
    for c in &node.children {
        flatten(c, depth + 1, out);
    }
}

fn collect_scaffold_matches(node: &Node, out: &mut Vec<ScaffoldMatch>) {
    if let Some(sid) = &node.scaffold_id {
        out.push(ScaffoldMatch {
            scaffold_id: sid.clone(),
            path: node.path.clone(),
            size: node.size,
            file_count: node.file_count,
        });
        // Don't recurse into matched subtrees — the match is the topmost root
        return;
    }
    for c in &node.children {
        collect_scaffold_matches(c, out);
    }
}

fn print_tree_summary(node: &Node, depth: usize, max_depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{}{} {} ({} bytes, {} files)", indent, if node.is_dir { "D" } else { "F" }, node.name, node.size, node.file_count);
    if depth < max_depth {
        for c in &node.children {
            print_tree_summary(c, depth + 1, max_depth);
        }
    }
}

fn action_for(mode: Mode, granularity: RecycleGranularity, hard_delete: bool) -> Action {
    if hard_delete && granularity != RecycleGranularity::Directory {
        return Action::Delete;
    }
    // Directory granularity always recycles — too costly to undo otherwise
    if granularity == RecycleGranularity::Directory {
        return Action::Recycle;
    }
    match mode {
        Mode::Recycle => Action::Recycle,
        Mode::Quarantine => Action::Quarantine,
        Mode::Delete => Action::Delete,
        // SystemCmd 走独立分流(cmd_preview_system_cmd / cmd_execute_system_cmd),
        // 不会到达 action_for。到达这里说明调用路径有 bug,直接 panic 暴露问题。
        Mode::SystemCmd => unreachable!("SystemCmd scope should be intercepted before action_for"),
    }
}

// ───── scaffold scope matching (lifted from Pinkbin's lib.rs) ─────

const PRUNED_SYSTEM_DIRS: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    ".trash",
    ".trashes",
];

fn is_pruned_system_dir(name: &std::ffi::OsStr) -> bool {
    let Some(s) = name.to_str() else { return false };
    let lower = s.to_ascii_lowercase();
    PRUNED_SYSTEM_DIRS.iter().any(|p| *p == lower)
}

fn pinkbin_walker(root: &Path) -> jwalk::WalkDir {
    jwalk::WalkDir::new(root)
        .skip_hidden(false)
        .follow_links(false)
        .process_read_dir(|_, _, _, children| {
            children.retain(|res| {
                let Ok(entry) = res else { return true };
                if !entry.file_type.is_dir() {
                    return true;
                }
                !is_pruned_system_dir(&entry.file_name)
            });
        })
}

fn mtime_older_than(metadata: &std::fs::Metadata, days: Option<u32>) -> bool {
    let Some(d) = days else { return true };
    let Ok(modified) = metadata.modified() else { return true };
    let threshold = SystemTime::now()
        .checked_sub(Duration::from_secs(d as u64 * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    modified <= threshold
}

fn make_globset(glob: &str) -> anyhow::Result<globset::GlobSet> {
    let pattern = expand_env(glob);
    let g = globset::GlobBuilder::new(&pattern)
        .literal_separator(false)
        .case_insensitive(true)
        .build()?;
    let mut b = globset::GlobSetBuilder::new();
    b.add(g);
    Ok(b.build()?)
}

/// Find directories matching the scope's glob, after pruning ancestors.
fn find_matching_dirs(
    root: &Path,
    scope: &pinkbin_scaffold::Scope,
    older_than_days: Option<u32>,
) -> Vec<(PathBuf, u64, bool)> {
    let set = match make_globset(&scope.glob) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("scope `{}` invalid glob: {}", scope.id, e);
            return Vec::new();
        }
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in pinkbin_walker(root).into_iter().flatten() {
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        if path == root {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !mtime_older_than(&metadata, older_than_days) {
            continue;
        }
        let path_str = path.to_string_lossy().replace('\\', "/");
        if set.is_match(&path_str) {
            candidates.push(path);
        }
    }
    candidates.sort_by_key(|p| p.as_os_str().len());
    let mut keep: Vec<PathBuf> = Vec::with_capacity(candidates.len());
    for c in candidates {
        if !keep.iter().any(|k| c.starts_with(k)) {
            keep.push(c);
        }
    }
    keep.into_iter()
        .map(|p| {
            let size = dir_size(&p);
            (p, size, true)
        })
        .collect()
}

fn find_matching_files(
    root: &Path,
    scope: &pinkbin_scaffold::Scope,
    older_than_days: Option<u32>,
) -> Vec<(PathBuf, u64, bool)> {
    let set = match make_globset(&scope.glob) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("scope `{}` invalid glob: {}", scope.id, e);
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for entry in pinkbin_walker(root).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !mtime_older_than(&metadata, older_than_days) {
            continue;
        }
        let s = path.to_string_lossy().replace('\\', "/");
        if set.is_match(&s) {
            out.push((path, metadata.len(), false));
        }
    }
    out
}

fn dir_size(dir: &Path) -> u64 {
    let mut total: u64 = 0;
    for entry in pinkbin_walker(dir).into_iter().flatten() {
        if entry.file_type().is_file() {
            if let Ok(md) = entry.metadata() {
                total = total.saturating_add(md.len());
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pruned_system_dir_is_case_insensitive() {
        assert!(is_pruned_system_dir(std::ffi::OsStr::new("$RECYCLE.BIN")));
        assert!(is_pruned_system_dir(std::ffi::OsStr::new("$recycle.bin")));
        assert!(is_pruned_system_dir(std::ffi::OsStr::new("System Volume Information")));
        assert!(!is_pruned_system_dir(std::ffi::OsStr::new("normal_dir")));
    }
}
