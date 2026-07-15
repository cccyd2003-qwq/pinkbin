//! Risk-validation gate for all cleanup actions.
//!
//! 在 executor::execute 调用前,必须先过 `audit_plan`。这是整个 skill
//! 的安全基座 —— 即使 agent 被诱导、scaffold 写错、用户手滑,guard
//! 也会拦住指向系统关键路径的删除。
//!
//! 白名单是**编译期硬编码**的,不读配置文件。唯一逃生口是
//! `--allow-system` flag(在 AuditFlags 中),且会在审计结果里留痕。
//!
//! 设计原则:
//!   - 误拦优先于漏放(Block 严格一点,大不了用户加 flag)
//!   - 路径比较大小写不敏感、分隔符归一化(\\ → /)
//!   - 用户数据(Documents/Pictures/...)默认 Warn,需用户显式确认
//!   - System Volume Information / $Recycle.Bin / Windows 永远 Block

use serde::Serialize;
use std::path::{Path, PathBuf};

/// 审计结论。`audit_plan` 的返回。
pub enum AuditResult {
    /// 通过。`warnings` 是 Warn 级别的路径(用户数据等),执行方应提示但不阻塞。
    Approved { warnings: Vec<AuditNote> },
    /// 拒绝。`blocked` 是被拦的路径。执行方必须中止并报告给用户。
    Rejected { blocked: Vec<AuditNote>, warnings: Vec<AuditNote> },
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditNote {
    pub path: String,
    pub reason: String,
    pub class: Class,
}

/// 单个路径的分类。决定了 Verdict。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// 系统关键路径,永远 Block
    Protected,
    /// 用户数据目录,默认 Warn(需用户确认)
    UserData,
    /// 系统临时文件,Pass
    SystemTemp,
    /// 缓存类,Pass
    Cache,
    /// 未识别,交给 agent 判断(NeedsReview)
    Unknown,
}

/// 对单个路径的判定。
#[derive(Debug)]
pub enum Verdict {
    Pass,
    Warn(String),
    Block(String),
    NeedsReview,
}

/// 审计选项。`allow_system` 是唯一的逃生口,对应 CLI 的 `--allow-system`。
/// 开启后,Protected 路径降级为 Warn(仍留痕),不再 Block。
#[derive(Debug, Clone, Default)]
pub struct AuditFlags {
    pub allow_system: bool,
    /// 特殊 scaffold 专属放行(如 windows-old 需要 --confirm-windows-old)
    pub confirmed_scaffolds: Vec<String>,
}

/// 系统关键路径白名单(小写,已归一化分隔符)。
/// 这些路径下的任何文件/目录都 Block,除非 allow_system。
///
/// 注意:WinSxS 在这里被保护 —— 只能通过 DISM(system-cmd mode)清,
/// 绝不能直接删文件。guard 拦的就是"直接删 WinSxS"这种误操作。
const PROTECTED_PATHS: &[&str] = &[
    // ── 操作系统核心
    "c:/windows/system32",
    "c:/windows/syswow64",
    "c:/windows/winsxs",
    "c:/windows/boot",
    "c:/windows/efi",
    "c:/windows/fonts",
    "c:/windows/system",
    "c:/windows/system32/config",
    // ── 启动 / 引导
    "c:/boot",
    "c:/efi",
    // ── 卷元数据 / 回收站本身
    "c:/$recycle.bin",
    "c:/system volume information",
    // ── 注册表 hive(防误删 config 目录)
    "c:/windows/system32/config",
    // ── Program Files(软件卸载走专用流程,不在这里直接删)
    "c:/program files",
    "c:/program files (x86)",
    "c:/programdata",
];

/// 用户数据目录名(小写)。这些目录名出现在路径任意层级时触发 Warn。
/// 与 PROTECTED_PATHS 不同,这是按目录名匹配(因为用户目录可能在
/// D:\Users\... 等非 C 盘位置)。
const USER_DATA_DIR_NAMES: &[&str] = &[
    "documents",
    "pictures",
    "music",
    "videos",
    "desktop",
];

/// 检查单个路径的安全性。
pub fn check(path: &Path) -> Verdict {
    let norm = normalize(path);

    // 1. Protected:精确前缀匹配
    for prot in PROTECTED_PATHS {
        if norm == *prot || norm.starts_with(&format!("{}/", prot)) {
            return Verdict::Block(format!("系统关键路径:{}", prot));
        }
    }

    // 2. UserData:路径任意层级的目录名匹配
    for segment in norm.split('/') {
        if USER_DATA_DIR_NAMES.iter().any(|&u| u == segment) {
            return Verdict::Warn("用户数据目录,需用户显式确认".into());
        }
    }

    // 3. SystemTemp / Cache:按路径特征识别
    if is_system_temp(&norm) {
        return Verdict::Pass;
    }
    if is_cache_like(&norm) {
        return Verdict::Pass;
    }

    // 4. 未识别 → NeedsReview(交给 agent)
    Verdict::NeedsReview
}

/// 审计整个清理计划。返回 Approved 或 Rejected。
pub fn audit_plan(paths: &[PathBuf], flags: &AuditFlags) -> AuditResult {
    let mut blocked: Vec<AuditNote> = Vec::new();
    let mut warnings: Vec<AuditNote> = Vec::new();

    for p in paths {
        let verdict = check(p);
        let note = AuditNote {
            path: p.to_string_lossy().replace('\\', "/"),
            reason: match &verdict {
                Verdict::Pass => continue,
                Verdict::Warn(r) | Verdict::Block(r) => r.clone(),
                Verdict::NeedsReview => "未识别路径,需 agent 判断".into(),
            },
            class: match &verdict {
                Verdict::Pass => Class::Cache,
                Verdict::Warn(_) => Class::UserData,
                Verdict::Block(_) => Class::Protected,
                Verdict::NeedsReview => Class::Unknown,
            },
        };
        match verdict {
            Verdict::Pass => {}
            Verdict::Warn(_) => warnings.push(note),
            Verdict::NeedsReview => warnings.push(note),
            Verdict::Block(reason) => {
                if flags.allow_system {
                    // 逃生口:降级为 Warn,但留痕
                    warnings.push(AuditNote {
                        reason: format!("allow_system 放行:{}", reason),
                        ..note
                    });
                } else {
                    blocked.push(note);
                }
            }
        }
    }

    if blocked.is_empty() {
        AuditResult::Approved { warnings }
    } else {
        AuditResult::Rejected { blocked, warnings }
    }
}

/// 重复文件组删除前校验:每组至少保留 1 个副本。
/// (为后续 dedup crate 准备,当前 CLI 还没用到)
pub fn check_dedup_group(group_len: usize, delete_len: usize) -> Verdict {
    if delete_len >= group_len {
        return Verdict::Block("重复文件组全部被标记删除,至少保留 1 个".into());
    }
    Verdict::Pass
}

/// 校验 uninstall_string 是否含危险命令模式。
///
/// 拒绝(Block):format / diskpart / reg delete HKLM\SYSTEM / rd /s /q C:\Windows 等
///   会破坏系统的命令。
/// 警告(Warn):cmd /c、powershell、msiexec —— 正常但需 agent 注意。
/// 通过(Pass):普通卸载串(如 "C:\Program Files\App\uninstall.exe")。
pub fn check_uninstall_string(s: &str) -> Verdict {
    let lower = s.to_lowercase();

    // 绝对禁止的危险命令模式
    let block_patterns = [
        ("format ", "format 命令会格式化磁盘"),
        ("diskpart", "diskpart 可破坏分区表"),
        ("rd /s /q c:\\windows", "递归删除 Windows 目录"),
        ("del /f /s /q c:\\", "递归强删 C 盘根"),
        ("rmdir /s /q c:\\windows", "递归删除 Windows 目录"),
        ("reg delete hklm\\system", "删除系统注册表项"),
        ("reg delete hklm\\software\\microsoft\\windows", "删除 Windows 注册表项"),
        ("shutdown", "shutdown 会导致关机/重启"),
        ("-noexit -command", "PowerShell -noexit 可能执行任意脚本"),
    ];
    for (pat, reason) in &block_patterns {
        if lower.contains(pat) {
            return Verdict::Block(format!("uninstall_string 含危险命令模式 ({}): {}", pat, reason));
        }
    }

    // 警告级:合法但需注意
    let warn_patterns = ["cmd /c", "powershell", "msiexec", "wscript", "cscript"];
    for pat in &warn_patterns {
        if lower.contains(pat) {
            return Verdict::Warn(format!("uninstall_string 含 {} —— 正常但需 agent 确认来源可信", pat));
        }
    }

    Verdict::Pass
}

// ─────────────────────────── helpers ───────────────────────────

/// 路径归一化:小写 + 反斜杠转正斜杠 + 去掉末尾斜杠。
/// 用于和 PROTECTED_PATHS 比较(Windows 路径大小写不敏感)。
fn normalize(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    s.trim_end_matches('/').to_string()
}

fn is_system_temp(norm: &str) -> bool {
    // %TEMP% / %TMP% / C:\Windows\Temp
    // 环境变量已在 scaffold 层展开,这里只看特征
    norm.contains("/temp/") || norm.ends_with("/temp")
        || norm.contains("/windows/temp/")
        || norm.contains("/tmp/")
}

fn is_cache_like(norm: &str) -> bool {
    // 常见缓存目录名特征
    let lower = norm.to_lowercase();
    lower.contains("/cache/")
        || lower.ends_with("/cache")
        || lower.contains("/caches/")
        || lower.contains("/.cache/")
        || lower.contains("/logs/")
        || lower.contains("/log/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn blocks_windows_system32() {
        match check(Path::new("C:\\Windows\\System32\\drivers")) {
            Verdict::Block(_) => {}
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn blocks_winsxs_directly() {
        // WinSxS 必须走 DISM,直接删文件被拦
        match check(Path::new("C:\\Windows\\WinSxS\\some_manifest")) {
            Verdict::Block(_) => {}
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn blocks_recycle_bin_itself() {
        match check(Path::new("C:\\$Recycle.Bin\\S-1-5-21")) {
            Verdict::Block(_) => {}
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn warns_user_documents() {
        match check(Path::new("C:\\Users\\alice\\Documents\\report.docx")) {
            Verdict::Warn(_) => {}
            other => panic!("expected Warn, got {:?}", other),
        }
    }

    #[test]
    fn warns_user_pictures_on_d_drive() {
        // 用户数据可能在非 C 盘
        match check(Path::new("D:\\Users\\bob\\Pictures\\vacation.jpg")) {
            Verdict::Warn(_) => {}
            other => panic!("expected Warn, got {:?}", other),
        }
    }

    #[test]
    fn passes_temp_dir() {
        match check(Path::new("C:\\Users\\alice\\AppData\\Local\\Temp\\junk.tmp")) {
            Verdict::Pass => {}
            other => panic!("expected Pass, got {:?}", other),
        }
    }

    #[test]
    fn passes_cache_dir() {
        match check(Path::new("C:\\Users\\alice\\AppData\\Local\\Chrome\\Cache\\f_00001")) {
            Verdict::Pass => {}
            other => panic!("expected Pass, got {:?}", other),
        }
    }

    #[test]
    fn needs_review_for_unknown() {
        match check(Path::new("D:\\Games\\Steam\\steamapps\\common\\Cyberpunk")) {
            Verdict::NeedsReview => {}
            other => panic!("expected NeedsReview, got {:?}", other),
        }
    }

    #[test]
    fn audit_rejects_plan_with_protected_path() {
        let paths = vec![
            PathBuf::from("C:\\Users\\alice\\AppData\\Local\\Temp\\ok.tmp"),
            PathBuf::from("C:\\Windows\\System32\\kernel32.dll"),
        ];
        match audit_plan(&paths, &AuditFlags::default()) {
            AuditResult::Rejected { blocked, .. } => {
                assert_eq!(blocked.len(), 1);
                // path is normalized to lowercase + forward slashes
                assert!(
                    blocked[0].path.to_lowercase().contains("system32"),
                    "blocked path was: {}",
                    blocked[0].path
                );
            }
            AuditResult::Approved { .. } => panic!("should reject"),
        }
    }

    #[test]
    fn audit_approves_with_allow_system() {
        let paths = vec![PathBuf::from("C:\\Windows\\System32\\kernel32.dll")];
        let flags = AuditFlags { allow_system: true, ..Default::default() };
        match audit_plan(&paths, &flags) {
            AuditResult::Approved { warnings } => {
                assert_eq!(warnings.len(), 1);
                assert!(warnings[0].reason.contains("allow_system"));
            }
            AuditResult::Rejected { .. } => panic!("should approve with allow_system"),
        }
    }

    #[test]
    fn audit_approves_all_pass() {
        let paths = vec![
            PathBuf::from("/tmp/junk1.tmp"),
            PathBuf::from("/tmp/junk2.tmp"),
        ];
        match audit_plan(&paths, &AuditFlags::default()) {
            AuditResult::Approved { warnings } => assert!(warnings.is_empty()),
            AuditResult::Rejected { .. } => panic!("should approve"),
        }
    }

    #[test]
    fn dedup_group_all_deleted_blocked() {
        match check_dedup_group(3, 3) {
            Verdict::Block(_) => {}
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn dedup_group_one_kept_passes() {
        match check_dedup_group(3, 2) {
            Verdict::Pass => {}
            other => panic!("expected Pass, got {:?}", other),
        }
    }

    #[test]
    fn normalize_handles_backslash_and_case() {
        assert_eq!(normalize(Path::new("C:\\Windows\\System32\\")), "c:/windows/system32");
        assert_eq!(normalize(Path::new("D:/Users/Bob")), "d:/users/bob");
    }

    #[test]
    fn blocks_program_files() {
        match check(Path::new("C:\\Program Files\\SomeApp\\uninstall.exe")) {
            Verdict::Block(_) => {}
            other => panic!("expected Block, got {:?}", other),
        }
    }
}
