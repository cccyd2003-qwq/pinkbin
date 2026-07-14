//! 系统级优化操作(系统优化类能力)。
//!
//! 封装 4 类 Windows 系统级空间优化操作:
//!   - `hibernate`:休眠文件管控(powercfg /h on/off/size)
//!   - `pagefile`:虚拟内存迁移(禁用 C 盘 pagefile + 在目标盘创建)
//!   - `restore`:系统还原点清理(vssadmin delete shadows)
//!   - `migrate`:可迁移软件分析(扫描 C 盘 Program Files 中占用大的软件)
//!
//! 所有操作返回结构化 JSON。查询类操作只读,修改类操作需管理员权限。
//! 非 Windows 环境下,执行类操作返回错误,查询类返回空/默认值。

use serde::Serialize;

// ─────────────────────────── hibernate ───────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct HibernateStatus {
    /// 休眠是否开启
    pub enabled: bool,
    /// hiberfil.sys 路径(通常 C:\hiberfil.sys)
    pub hiberfil_path: String,
    /// hiberfil.sys 当前大小(字节),None = 文件不存在或无法读取
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hiberfil_size_bytes: Option<u64>,
    /// hiberfil.sys 大小占物理内存的百分比(None = 未知)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_percent: Option<u32>,
    /// powercfg /a 的原始输出(供 agent 解读)
    pub powercfg_a_output: String,
    pub requires_admin: bool,
}

/// 查询休眠状态(只读,不需管理员)。
pub fn hibernate_status() -> HibernateStatus {
    let hiberfil_path = "C:\\hiberfil.sys".to_string();
    let hiberfil_size_bytes = std::fs::metadata(&hiberfil_path)
        .ok()
        .map(|m| m.len());

    let powercfg_a_output = run_cmd_capture("powercfg /a");
    let enabled = hiberfil_size_bytes.is_some()
        && !powercfg_a_output.to_lowercase().contains("hibernation has been removed");

    HibernateStatus {
        enabled,
        hiberfil_path,
        hiberfil_size_bytes,
        size_percent: None,
        powercfg_a_output,
        requires_admin: false,
    }
}

/// 关闭休眠(释放 hiberfil.sys,需管理员)。
/// 实际执行 `powercfg /h off`。
pub fn hibernate_off() -> CmdResult {
    run_cmd("powercfg /h off", true)
}

/// 开启休眠(需管理员)。
/// 实际执行 `powercfg /h on`。
pub fn hibernate_on() -> CmdResult {
    run_cmd("powercfg /h on", true)
}

/// 设置 hiberfil.sys 大小占物理内存的百分比(需管理员)。
/// `powercfg /h /size <percent>`,percent 范围 50-100。
pub fn hibernate_set_size(percent: u32) -> CmdResult {
    if !(50..=100).contains(&percent) {
        return CmdResult {
            command: String::new(),
            executed: false,
            requires_admin: true,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(format!("percent 必须在 50-100 之间,收到 {}", percent)),
        };
    }
    run_cmd(&format!("powercfg /h /size {}", percent), true)
}

// ─────────────────────────── pagefile ───────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PagefileStatus {
    /// 当前 pagefile 配置列表
    pub pagefiles: Vec<PagefileEntry>,
    /// 是否为自动管理(AutomaticManagedPagefile)
    pub auto_managed: bool,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PagefileEntry {
    /// 如 "C:\\pagefile.sys"
    pub name: String,
    /// 所在盘符
    pub drive: String,
    /// 当前大小(字节)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_size_bytes: Option<u64>,
    /// 峰值使用(字节)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_usage_bytes: Option<u64>,
    /// 临时文件状态(AllocatedBaseSize / CurrentUsage)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocated_base_size_mb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_usage_mb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_usage_mb: Option<u64>,
}

/// 查询 pagefile 配置(只读,不需管理员)。
/// 用 PowerShell `Get-CimInstance` 读取(比 wmic 更现代)。
pub fn pagefile_status() -> PagefileStatus {
    // 用 PowerShell 查询 CIM
    let ps_cmd = "powershell -Command \"Get-CimInstance Win32_PageFileUsage | Select-Object Name,AllocatedBaseSize,CurrentUsage,PeakUsage | ConvertTo-Json\"";
    let output = run_cmd_capture(ps_cmd);

    let mut pagefiles: Vec<PagefileEntry> = Vec::new();
    if !output.trim().is_empty() && output.trim() != "null" {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&output) {
            let arr = match &v {
                serde_json::Value::Array(a) => a.clone(),
                serde_json::Value::Object(_) => vec![v],
                _ => Vec::new(),
            };
            for item in arr {
                let name = item
                    .get("Name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                let drive = name.chars().next().map(|c| c.to_string()).unwrap_or_default();
                pagefiles.push(PagefileEntry {
                    name,
                    drive,
                    current_size_bytes: None,
                    peak_usage_bytes: None,
                    allocated_base_size_mb: item
                        .get("AllocatedBaseSize")
                        .and_then(|v| v.as_u64()),
                    current_usage_mb: item.get("CurrentUsage").and_then(|v| v.as_u64()),
                    peak_usage_mb: item.get("PeakUsage").and_then(|v| v.as_u64()),
                });
            }
        }
    }

    // 查询 auto_managed
    let auto_cmd = "powershell -Command \"(Get-CimInstance Win32_ComputerSystem).AutomaticManagedPagefile\"";
    let auto_output = run_cmd_capture(auto_cmd);
    let auto_managed = auto_output.trim().to_lowercase() == "true";

    PagefileStatus {
        pagefiles,
        auto_managed,
        requires_admin: false,
    }
}

/// 迁移 pagefile 到目标盘(需管理员,需重启生效)。
///
/// 步骤:
///   1. 关闭自动管理:`wmic computersystem set AutomaticManagedPagefile=False`
///   2. 删除 C 盘 pagefile 配置:`wmic pagefilesetting where name=\"C:\\\\pagefile.sys\" delete`
///   3. 在目标盘创建新 pagefile:`wmic pagefilesetting create name=\"<DRIVE>:\\\\pagefile.sys\"`
///
/// 注意:不指定大小则由系统管理。迁移后需重启生效。
pub fn pagefile_migrate(target_drive: &str) -> CmdResult {
    let drive = target_drive.trim().trim_end_matches(':').trim_end_matches('\\');
    if drive.len() != 1 || !drive.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
        return CmdResult {
            command: String::new(),
            executed: false,
            requires_admin: true,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(format!("target_drive 必须是单个盘符(如 D),收到: {}", target_drive)),
        };
    }
    let upper = drive.to_uppercase();
    let cmd = format!(
        "wmic computersystem set AutomaticManagedPagefile=False && wmic pagefilesetting where name=\"C:\\\\pagefile.sys\" delete && wmic pagefilesetting create name=\"{}:\\\\pagefile.sys\"",
        upper
    );
    run_cmd(&cmd, true)
}

// ─────────────────────────── restore points ───────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct RestorePointStatus {
    /// 卷影副本数量
    pub shadow_count: usize,
    /// vssadmin list shadows 的原始输出
    pub vssadmin_output: String,
    /// 已用空间(从 vssadmin list shadowstorage 解析)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_space_bytes: Option<u64>,
    /// 已分配空间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocated_space_bytes: Option<u64>,
    pub requires_admin: bool,
}

/// 查询系统还原点 / 卷影副本状态(需管理员才能完整查看)。
pub fn restore_status() -> RestorePointStatus {
    let shadows_output = run_cmd_capture("vssadmin list shadows");
    let shadow_count = shadows_output.matches("Shadow Copy Volume").count();

    // 查询 shadowstorage 获取空间占用
    let storage_output = run_cmd_capture("vssadmin list shadowstorage");
    let used_space_bytes = parse_vss_storage(&storage_output, "Used");
    let allocated_space_bytes = parse_vss_storage(&storage_output, "Allocated");

    RestorePointStatus {
        shadow_count,
        vssadmin_output: format!("=== shadows ===\n{}\n=== shadowstorage ===\n{}", shadows_output, storage_output),
        used_space_bytes,
        allocated_space_bytes,
        requires_admin: true,
    }
}

/// 解析 vssadmin list shadowstorage 输出中的空间数值。
/// 输出格式示例:
///   Used Shadow Copy Storage space: 5.2 GB (5,589,xxxx bytes)
///   Allocated Shadow Copy Storage space: 6 GB (6,4xxx bytes)
fn parse_vss_storage(output: &str, keyword: &str) -> Option<u64> {
    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.contains(&keyword.to_lowercase()) && lower.contains("bytes") {
            // 提取括号内的 "X,XXX,XXX bytes"
            if let Some(start) = line.rfind('(') {
                if let Some(end) = line[start..].find(" bytes") {
                    let num_str = &line[start + 1..start + end];
                    let cleaned: String = num_str.chars().filter(|c| c.is_ascii_digit()).collect();
                    if let Ok(n) = cleaned.parse::<u64>() {
                        return Some(n);
                    }
                }
            }
        }
    }
    None
}

/// 清理所有系统还原点 / 卷影副本(需管理员)。
/// `vssadmin delete shadows /all /quiet`
/// 注意:删除后无法通过系统还原回滚到之前的状态。
pub fn restore_delete_all() -> CmdResult {
    run_cmd("vssadmin delete shadows /all /quiet", true)
}

// ─────────────────────────── app migration analysis ───────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct MigrationCandidate {
    pub name: String,
    pub install_path: String,
    pub size_bytes: u64,
    pub drive: String,
    /// 是否有 uninstall_string(影响迁移难度)
    pub has_uninstall_string: bool,
}

#[derive(Debug, Serialize)]
pub struct MigrationAnalysis {
    /// C 盘上可迁移的大软件列表(按大小降序)
    pub candidates: Vec<MigrationCandidate>,
    /// C 盘 Program Files / Program Files (x86) 总占用
    pub c_program_files_total_bytes: u64,
    pub requires_admin: bool,
}

/// 分析 C 盘上可迁移的大软件(只读,不需管理员)。
///
/// 扫描:
///   - C:\Program Files\*
///   - C:\Program Files (x86)\*
///   - C:\Users\<user>\AppData\Local\Programs\*
///
/// 列出每个软件目录的大小,按降序排列。agent 可据此建议用户迁移哪些。
pub fn analyze_migratable_apps() -> MigrationAnalysis {
    let mut candidates: Vec<MigrationCandidate> = Vec::new();
    let mut c_program_files_total: u64 = 0;

    let scan_roots = [
        "C:\\Program Files",
        "C:\\Program Files (x86)",
    ];

    for root in &scan_roots {
        let root_path = std::path::Path::new(root);
        if !root_path.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(root_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let size = dir_size_recursive(&path);
                c_program_files_total = c_program_files_total.saturating_add(size);
                let name = entry
                    .file_name()
                    .to_string_lossy()
                    .to_string();
                let drive = "C".to_string();
                candidates.push(MigrationCandidate {
                    name,
                    install_path: path.to_string_lossy().to_string(),
                    size_bytes: size,
                    drive,
                    has_uninstall_string: false,
                });
            }
        }
    }

    // 扫描用户级安装
    let local_programs = format!(
        "{}\\Programs",
        std::env::var("LOCALAPPDATA").unwrap_or_default()
    );
    let lp_path = std::path::Path::new(&local_programs);
    if lp_path.exists() {
        if let Ok(entries) = std::fs::read_dir(lp_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let size = dir_size_recursive(&path);
                let name = entry.file_name().to_string_lossy().to_string();
                candidates.push(MigrationCandidate {
                    name,
                    install_path: path.to_string_lossy().to_string(),
                    size_bytes: size,
                    drive: "C".to_string(),
                    has_uninstall_string: false,
                });
            }
        }
    }

    // 按大小降序
    candidates.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

    MigrationAnalysis {
        candidates,
        c_program_files_total_bytes: c_program_files_total,
        requires_admin: false,
    }
}

fn dir_size_recursive(dir: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    let stack = vec![dir.to_path_buf()];
    let mut stack = stack;
    while let Some(p) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&p) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if let Ok(md) = entry.metadata() {
                    if md.is_file() {
                        total = total.saturating_add(md.len());
                    } else if md.is_dir() {
                        stack.push(entry_path);
                    }
                }
            }
        }
    }
    total
}

// ─────────────────────────── command runner ───────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CmdResult {
    pub command: String,
    pub executed: bool,
    pub requires_admin: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 运行命令(Windows 用 cmd /C,非 Windows 返回错误)。
fn run_cmd(command: &str, requires_admin: bool) -> CmdResult {
    if command.is_empty() {
        return CmdResult {
            command: String::new(),
            executed: false,
            requires_admin,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some("command 为空".to_string()),
        };
    }

    if !cfg!(windows) {
        return CmdResult {
            command: command.to_string(),
            executed: false,
            requires_admin,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some("此命令只能在 Windows 上执行(当前非 Windows 环境)".to_string()),
        };
    }

    match std::process::Command::new("cmd")
        .args(["/C", command])
        .output()
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let exit_code = out.status.code();
            let executed = out.status.success();
            CmdResult {
                command: command.to_string(),
                executed,
                requires_admin,
                exit_code,
                stdout,
                stderr,
                error: if executed {
                    None
                } else {
                    Some(format!("命令退出码: {:?}", exit_code))
                },
            }
        }
        Err(e) => CmdResult {
            command: command.to_string(),
            executed: false,
            requires_admin,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(format!("命令启动失败: {}", e)),
        },
    }
}

/// 运行命令并捕获 stdout(用于查询类操作)。
/// 非 Windows 返回空字符串。
fn run_cmd_capture(command: &str) -> String {
    if !cfg!(windows) {
        return String::new();
    }
    std::process::Command::new("cmd")
        .args(["/C", command])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hibernate_status_does_not_crash() {
        // 非 Windows 上返回默认值,不应崩溃
        let s = hibernate_status();
        assert!(!s.enabled || cfg!(windows));
        assert_eq!(s.hiberfil_path, "C:\\hiberfil.sys");
    }

    #[test]
    fn pagefile_status_does_not_crash() {
        let _s = pagefile_status();
    }

    #[test]
    fn restore_status_does_not_crash() {
        let _s = restore_status();
    }

    #[test]
    fn parse_vss_storage_extracts_bytes() {
        let output = "Shadow Copy Storage association\n\
            For volume: (C:)\\\\\\\\?\\\\Volume{guid}\n\
            Shadow Copy Storage volume: (C:)\\\\\\\\?\\\\Volume{guid}\n\
            Used Shadow Copy Storage space: 5.2 GB (5,589,345,234 bytes)\n\
            Allocated Shadow Copy Storage space: 6 GB (6,442,450,944 bytes)\n\
            Maximum Shadow Copy Storage space: UNBOUNDED";
        let used = parse_vss_storage(output, "Used");
        let allocated = parse_vss_storage(output, "Allocated");
        assert_eq!(used, Some(5589345234));
        assert_eq!(allocated, Some(6442450944));
    }

    #[test]
    fn parse_vss_storage_returns_none_on_no_match() {
        assert_eq!(parse_vss_storage("no relevant info", "Used"), None);
    }

    #[test]
    fn hibernate_set_size_rejects_invalid_percent() {
        let r = hibernate_set_size(49);
        assert!(!r.executed);
        assert!(r.error.unwrap().contains("50-100"));
    }

    #[test]
    fn pagefile_migrate_rejects_invalid_drive() {
        let r = pagefile_migrate("DE");
        assert!(!r.executed);
        assert!(r.error.unwrap().contains("单个盘符"));
    }

    #[test]
    fn analyze_migratable_apps_does_not_crash() {
        let a = analyze_migratable_apps();
        // 非 Windows 上 C:\Program Files 不存在,candidates 为空
        if !cfg!(windows) {
            assert!(a.candidates.is_empty() || a.c_program_files_total_bytes == 0);
        }
    }

    #[test]
    fn run_cmd_non_windows_returns_error() {
        if !cfg!(windows) {
            let r = run_cmd("echo test", false);
            assert!(!r.executed);
            assert!(r.error.unwrap().contains("非 Windows"));
        }
    }
}
