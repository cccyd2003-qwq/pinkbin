//! Cross-platform disk scanner. Returns a tree of directories with size/file_count.
//!
//! v0.1.1: parallel walk via jwalk (rayon under the hood) + per-leaf file
//! children + progress callback. v0.2 will swap in direct NTFS MFT read on
//! Windows for sub-3s C: drive scans.

use jwalk::WalkDir as JWalk;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// 系统级"垃圾/卷元数据"目录名（lowercase）。scanner 扫盘时整树跳过。
/// 不只是性能：让回收站进 Node tree 会被前端 fallback 的 name_contains
/// 当成"伪 app 数据 root"——用户曾把 xwechat_files 删进回收站后，
/// `C:\$Recycle.Bin\<SID>\$R*\xwechat_files` 会被识别为活的 WeChat 数据
/// 触发误删。System Volume Information 同时还能避开 VSS 权限拒绝噪音。
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

#[cfg(windows)]
mod mft;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub file_count: u64,
    pub children: Vec<Node>,
    #[serde(default)]
    pub scaffold_id: Option<String>,
    #[serde(default)]
    pub top_extensions: Vec<ExtShare>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtShare {
    pub ext: String,
    pub bytes: u64,
    pub count: u64,
}

#[derive(Debug, Default)]
struct DirAcc {
    size: u64,
    file_count: u64,
    ext_bytes: HashMap<String, u64>,
    ext_count: HashMap<String, u64>,
    files: Vec<(String, u64)>, // (file name, size) — only kept on the immediate parent
}

pub struct ScanOptions {
    pub follow_symlinks: bool,
    pub max_depth: Option<usize>,
    /// How many files to keep per directory in the returned tree. None = all (memory hog on large dirs).
    pub keep_files_per_dir: Option<usize>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            max_depth: None,
            keep_files_per_dir: Some(500),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScanProgress {
    pub files_seen: u64,
    pub bytes_seen: u64,
    pub current_path: String,
}

/// Phase-level timings for a scan. Diagnostic only — emit via the Tauri command
/// alongside the tree so the UI / packaged binary can show "where the time went"
/// without needing RUST_LOG=info.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanStats {
    pub mode: String, // "mft" | "walkdir"
    pub mft_attempted: bool,
    pub mft_succeeded: bool,
    pub mft_ms: u64,  // total time spent in the MFT branch (success or fallback)
    pub walk_ms: u64, // jwalk consume loop (only set in walkdir mode)
    pub build_tree_ms: u64, // build_tree recursion + 2nd read_dir pass
    pub total_ms: u64,
    pub files_seen: u64,
    pub bytes_seen: u64,
    pub dirs_in_acc: u64, // accs.len() — proxy for memory pressure (walkdir mode only)
}

pub fn scan<P: AsRef<Path>>(root: P) -> anyhow::Result<Node> {
    scan_with(root, ScanOptions::default(), |_| {})
}

pub fn scan_with<P, F>(root: P, opts: ScanOptions, on_progress: F) -> anyhow::Result<Node>
where
    P: AsRef<Path>,
    F: Fn(&ScanProgress) + Send + Sync,
{
    scan_with_stats(root, opts, on_progress).map(|(n, _)| n)
}

/// Same as `scan_with`, but also returns phase-level timings. Internal API for
/// the desktop app's diagnostics bar — keeps `scan` / `scan_with` unchanged.
pub fn scan_with_stats<P, F>(
    root: P,
    opts: ScanOptions,
    on_progress: F,
) -> anyhow::Result<(Node, ScanStats)>
where
    P: AsRef<Path>,
    F: Fn(&ScanProgress) + Send + Sync,
{
    let root = root.as_ref().to_path_buf();
    let total_t0 = Instant::now();
    let mut stats = ScanStats::default();
    tracing::info!("scan: start root={:?}", root);

    // Try the MFT fast path on Windows when the root is on an NTFS volume.
    #[cfg(windows)]
    {
        if let Some(letter) = drive_letter_of(&root) {
            let subroot = if is_drive_root(&root) {
                None
            } else {
                Some(root.as_path())
            };
            let progress = &on_progress;
            stats.mft_attempted = true;
            let mft_t0 = Instant::now();
            // The `ntfs` crate can panic on non-NTFS volumes (e.g. CI runners,
            // ReFS, removable media) instead of returning an Err. Catch it so
            // we always fall back to walkdir cleanly. AssertUnwindSafe is OK
            // because we don't observe partial state on panic.
            let mft_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                mft::scan_volume(letter, subroot, |records, bytes| {
                    progress(&ScanProgress {
                        files_seen: records,
                        bytes_seen: bytes,
                        current_path: format!("MFT record {}", records),
                    });
                })
            }))
            .unwrap_or_else(|_| {
                Err(anyhow::anyhow!(
                    "MFT scan panicked (likely non-NTFS volume)"
                ))
            });
            match mft_result {
                Ok(n) => {
                    stats.mft_ms = mft_t0.elapsed().as_millis() as u64;
                    stats.mft_succeeded = true;
                    stats.mode = "mft".into();
                    stats.files_seen = n.file_count;
                    stats.bytes_seen = n.size;
                    stats.total_ms = total_t0.elapsed().as_millis() as u64;
                    tracing::info!(
                        "scan: mode=mft mft_ms={} total_ms={} files={} bytes={}",
                        stats.mft_ms,
                        stats.total_ms,
                        stats.files_seen,
                        stats.bytes_seen,
                    );
                    progress(&ScanProgress {
                        files_seen: n.file_count,
                        bytes_seen: n.size,
                        current_path: "done (mft)".into(),
                    });
                    return Ok((n, stats));
                }
                Err(e) => {
                    stats.mft_ms = mft_t0.elapsed().as_millis() as u64;
                    tracing::warn!(
                        "MFT scan failed after {} ms, falling back to walkdir: {e:#}",
                        stats.mft_ms
                    );
                }
            }
        }
    }

    stats.mode = "walkdir".into();
    let files_seen = Arc::new(AtomicU64::new(0));
    let bytes_seen = Arc::new(AtomicU64::new(0));
    let last_emit = Arc::new(AtomicU64::new(0));

    // Phase 1: parallel walk, collect (path, size) pairs for every file.
    let mut walker = JWalk::new(&root)
        .skip_hidden(false)
        .follow_links(opts.follow_symlinks)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(5),
        })
        .process_read_dir(|_, _, _, children| {
            children.retain(|res| {
                let Ok(entry) = res else { return true };
                if !entry.file_type.is_dir() {
                    return true;
                }
                !is_pruned_system_dir(&entry.file_name)
            });
        });
    if let Some(d) = opts.max_depth {
        walker = walker.max_depth(d);
    }

    let mut accs: HashMap<PathBuf, DirAcc> = HashMap::new();
    let walk_t0 = Instant::now();

    for entry in walker.into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "(none)".into());
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        // attribute to immediate parent (with files list) and walk upward (totals only)
        if let Some(parent) = path.parent() {
            let acc = accs.entry(parent.to_path_buf()).or_default();
            acc.files.push((file_name, size));
        }
        let mut cur = path.parent();
        while let Some(dir) = cur {
            let acc = accs.entry(dir.to_path_buf()).or_default();
            acc.size += size;
            acc.file_count += 1;
            *acc.ext_bytes.entry(ext.clone()).or_insert(0) += size;
            *acc.ext_count.entry(ext.clone()).or_insert(0) += 1;
            if dir == root || !dir.starts_with(&root) {
                break;
            }
            cur = dir.parent();
        }

        let total_files = files_seen.fetch_add(1, Ordering::Relaxed) + 1;
        bytes_seen.fetch_add(size, Ordering::Relaxed);
        // Throttle progress to ~every 5k files to avoid IPC saturation.
        if total_files.wrapping_sub(last_emit.load(Ordering::Relaxed)) >= 5000 {
            last_emit.store(total_files, Ordering::Relaxed);
            on_progress(&ScanProgress {
                files_seen: total_files,
                bytes_seen: bytes_seen.load(Ordering::Relaxed),
                current_path: path.to_string_lossy().to_string(),
            });
        }
    }

    stats.walk_ms = walk_t0.elapsed().as_millis() as u64;
    stats.files_seen = files_seen.load(Ordering::Relaxed);
    stats.bytes_seen = bytes_seen.load(Ordering::Relaxed);
    stats.dirs_in_acc = accs.len() as u64;
    tracing::info!(
        "scan: walk done walk_ms={} files={} bytes={} dirs_in_acc={}",
        stats.walk_ms,
        stats.files_seen,
        stats.bytes_seen,
        stats.dirs_in_acc,
    );

    on_progress(&ScanProgress {
        files_seen: stats.files_seen,
        bytes_seen: stats.bytes_seen,
        current_path: "done".into(),
    });

    let build_t0 = Instant::now();
    let tree = build_tree(&root, &accs, opts.keep_files_per_dir);
    stats.build_tree_ms = build_t0.elapsed().as_millis() as u64;
    stats.total_ms = total_t0.elapsed().as_millis() as u64;
    tracing::info!(
        "scan: mode=walkdir walk_ms={} build_tree_ms={} total_ms={} dirs_in_acc={}",
        stats.walk_ms,
        stats.build_tree_ms,
        stats.total_ms,
        stats.dirs_in_acc,
    );
    Ok((tree, stats))
}

fn build_tree(dir: &Path, accs: &HashMap<PathBuf, DirAcc>, keep_files: Option<usize>) -> Node {
    let acc = accs.get(dir);
    let size = acc.map(|a| a.size).unwrap_or(0);
    let file_count = acc.map(|a| a.file_count).unwrap_or(0);
    let top_extensions = acc
        .map(|a| {
            let mut v: Vec<_> = a
                .ext_bytes
                .iter()
                .map(|(k, &b)| ExtShare {
                    ext: k.clone(),
                    bytes: b,
                    count: a.ext_count.get(k).copied().unwrap_or(0),
                })
                .collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.bytes));
            v.truncate(8);
            v
        })
        .unwrap_or_default();

    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.to_string_lossy().to_string());

    let mut children: Vec<Node> = Vec::new();

    // Subdirectories — recurse. build_tree 用 std::fs::read_dir 重新枚举子目录
    // （不复用 jwalk 的输出），所以 prune 必须在这里再做一次——否则即便 jwalk
    // 跳过了 $Recycle.Bin，build_tree 仍会把它列为 dir 节点放进 Node tree，
    // 前端 fallbackByNameContains 仍会把回收站里的伪 root 当成 app 数据。
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if is_pruned_system_dir(&entry.file_name()) {
                    continue;
                }
                children.push(build_tree(&entry.path(), accs, keep_files));
            }
        }
    }

    // Files — pull the largest from this dir's acc and emit as leaf nodes.
    if let Some(a) = acc {
        let mut files = a.files.clone();
        files.sort_by_key(|f| std::cmp::Reverse(f.1));
        let limit = keep_files.unwrap_or(usize::MAX);
        for (fname, fsize) in files.into_iter().take(limit) {
            let fpath = dir.join(&fname);
            children.push(Node {
                name: fname,
                path: fpath.to_string_lossy().to_string(),
                is_dir: false,
                size: fsize,
                file_count: 1,
                children: Vec::new(),
                scaffold_id: None,
                top_extensions: Vec::new(),
            });
        }
    }

    children.sort_by_key(|c| std::cmp::Reverse(c.size));

    Node {
        name,
        path: dir.to_string_lossy().to_string(),
        is_dir: true,
        size,
        file_count,
        children,
        scaffold_id: None,
        top_extensions,
    }
}

#[cfg(windows)]
fn drive_letter_of(p: &Path) -> Option<char> {
    let s = p.to_string_lossy();
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        let c = bytes[0] as char;
        if c.is_ascii_alphabetic() {
            return Some(c);
        }
    }
    None
}

#[cfg(windows)]
fn is_drive_root(p: &Path) -> bool {
    let s = p.to_string_lossy();
    matches!(s.as_ref(), "C:" | "D:" | "E:" | "F:" | "G:" | "H:")
        || (s.len() == 3
            && s.as_bytes()[1] == b':'
            && (s.as_bytes()[2] == b'\\' || s.as_bytes()[2] == b'/'))
}

/// Pull up to `n` sample paths from a directory, ordered shallowest-first.
pub fn sample_paths<P: AsRef<Path>>(root: P, n: usize) -> Vec<String> {
    let root = root.as_ref();
    walkdir::WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_entry(|e| !e.file_type().is_dir() || !is_pruned_system_dir(e.file_name()))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .take(n)
        .map(|e| e.path().to_string_lossy().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_temp_dir() {
        let dir = tempdir_path();
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/file1.txt"), b"hello").unwrap();
        fs::write(dir.join("a/b/file2.txt"), b"world!").unwrap();

        let node = scan(&dir).unwrap();
        assert_eq!(node.size, 11);
        assert_eq!(node.file_count, 2);
        // file leaves should appear as children of their directory
        let a = node.children.iter().find(|c| c.name == "a").unwrap();
        assert!(a
            .children
            .iter()
            .any(|c| !c.is_dir && c.name == "file1.txt"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_prunes_system_trash_dirs() {
        let dir = tempdir_path();
        fs::create_dir_all(dir.join("normal")).unwrap();
        fs::write(dir.join("normal/keep.txt"), b"x").unwrap();
        for trashy in &[
            "$RECYCLE.BIN",
            "System Volume Information",
            ".Trash",
            ".Trashes",
        ] {
            let d = dir.join(trashy);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("inside.txt"), b"x").unwrap();
        }

        let node = scan(&dir).unwrap();

        let names: Vec<String> = node.children.iter().map(|c| c.name.clone()).collect();
        assert!(names.contains(&"normal".to_string()), "got: {names:?}");
        for trashy in &[
            "$RECYCLE.BIN",
            "System Volume Information",
            ".Trash",
            ".Trashes",
        ] {
            assert!(
                !names.iter().any(|n| n.eq_ignore_ascii_case(trashy)),
                "scanner leaked `{trashy}` into Node tree, names: {names:?}"
            );
        }
        assert_eq!(node.file_count, 1, "only `normal/keep.txt` should count");
        let _ = fs::remove_dir_all(&dir);
    }

    fn tempdir_path() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "pinkbin-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
