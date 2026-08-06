use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use include_dir::{include_dir, Dir};
use pinkbin_advisor::{
    advise as advise_provider, test_connection as test_advisor_connection, AdvisorRequest,
    AdvisorResponse, Provider,
};
use pinkbin_executor::{execute, Plan, UndoEntry};
use pinkbin_scaffold::{
    compile_all, detect_compiled, detect_for, expand_env, load_dir, parse_toml, CompiledScaffold,
    RecycleGranularity, Scaffold,
};
use pinkbin_scanner::{sample_paths, scan_with_stats, Node, ScanOptions, ScanStats};

use tauri::{AppHandle, Emitter, Manager, State};

// Compile-time embed of repo-root scaffolds/. Used as the lowest-priority
// fallback in load_all_scaffolds so a portable raw exe (no resource_dir,
// arbitrary cwd) still ships with all packaged scaffolds.
static EMBEDDED_SCAFFOLDS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../../scaffolds");

#[derive(serde::Serialize, Clone)]
struct ScanProgressEvent {
    files_seen: u64,
    bytes_seen: u64,
    current_path: String,
}

struct AppState {
    scaffolds: Mutex<Vec<Scaffold>>,
    advisor: Mutex<Option<Provider>>,
    quarantine_root: PathBuf,
    undo_log: PathBuf,
}

#[tauri::command]
async fn scan_path(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<Node, String> {
    let p = PathBuf::from(&path);
    let app_for_progress = app.clone();
    let app_for_stats = app.clone();
    let cmd_t0 = std::time::Instant::now();

    // Compile scaffolds outside spawn_blocking so we don't carry the AppState
    // Mutex across thread boundaries. The compiled form is `Send` and used by
    // the post-scan walk to fill `Node.scaffold_id`.
    let compiled = compile_all(&state.scaffolds.lock().unwrap().clone());

    let result = tokio::task::spawn_blocking(move || {
        let scan_result = scan_with_stats(p, ScanOptions::default(), |progress| {
            let _ = app_for_progress.emit(
                "scan-progress",
                ScanProgressEvent {
                    files_seen: progress.files_seen,
                    bytes_seen: progress.bytes_seen,
                    current_path: progress.current_path.clone(),
                },
            );
        });
        // After the scan returns, walk the tree once to fill scaffold_id and
        // apply the depth-based breadth caps that the frontend's tagScaffolds
        // used to apply. Doing both here in one pass with pre-compiled
        // GlobSets replaces the previous per-directory IPC storm.
        let tag_t0 = std::time::Instant::now();
        scan_result.map(|(mut node, stats)| {
            tag_and_truncate(&mut node, &compiled, 0);
            (node, stats, tag_t0.elapsed().as_millis() as u64)
        })
    })
    .await
    .map_err(|e| e.to_string())?;

    match result {
        Ok((node, stats, tag_ms)) => {
            let cmd_ms = cmd_t0.elapsed().as_millis() as u64;
            tracing::info!(
                "scan_path: cmd_ms={} scanner_ms={} tag_ms={} overhead={}ms",
                cmd_ms,
                stats.total_ms,
                tag_ms,
                cmd_ms.saturating_sub(stats.total_ms).saturating_sub(tag_ms),
            );
            let _ = app_for_stats.emit(
                "scan-stats",
                &ScanStatsEvent::from((cmd_ms, tag_ms, &stats)),
            );
            Ok(node)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Walks `node` in place, filling `scaffold_id` for directories and truncating
/// each level's children to the same depth-based caps the old frontend
/// `tagScaffolds` applied (depth<2 → 100, depth<4 → 50, else → 20).
///
/// Order matters: we recurse into ALL children first to propagate scaffold tags
/// fully, then apply a tag-aware truncation that keeps any subtree containing a
/// match before falling back to "biggest N" by size. Without this, a deep
/// scaffold target (e.g. `C:\Users\<u>\Documents\xwechat_files`) or one nested
/// inside a populated install dir (`Weixin\<v>\WeChatPlayer.bin`) could be
/// truncated out at deeper scan roots where the cap shrinks to 20.
fn tag_and_truncate(node: &mut Node, compiled: &[CompiledScaffold], depth: usize) {
    if node.is_dir {
        node.scaffold_id = detect_compiled(compiled, std::path::Path::new(&node.path));
    }
    for c in &mut node.children {
        tag_and_truncate(c, compiled, depth + 1);
    }
    let cap = if depth < 2 {
        100
    } else if depth < 4 {
        50
    } else {
        20
    };
    if node.children.len() > cap {
        // Partition tagged subtrees first (preserving their original size-desc
        // order), then fill remaining slots from the rest. Cap the tagged group
        // at `cap` too, so a freak case with > cap matches at one level still
        // produces a bounded tree.
        let (tagged, rest): (Vec<Node>, Vec<Node>) =
            node.children.drain(..).partition(has_scaffold_tag);
        let mut survivors: Vec<Node> = tagged.into_iter().take(cap).collect();
        let need = cap.saturating_sub(survivors.len());
        survivors.extend(rest.into_iter().take(need));
        // Restore size-desc order for display (partition mixed tagged in front).
        survivors.sort_by_key(|c| std::cmp::Reverse(c.size));
        node.children = survivors;
    }
}

/// True if `n` itself, or any descendant, has a scaffold_id assigned. Used by
/// `tag_and_truncate` to decide which subtrees must survive truncation.
fn has_scaffold_tag(n: &Node) -> bool {
    n.scaffold_id.is_some() || n.children.iter().any(has_scaffold_tag)
}

#[derive(serde::Serialize, Clone)]
struct ScanStatsEvent {
    mode: String,
    mft_attempted: bool,
    mft_succeeded: bool,
    mft_ms: u64,
    walk_ms: u64,
    build_tree_ms: u64,
    /// Time inside `scan_with_stats` (mft_ms + walk_ms + build_tree_ms + overhead).
    scanner_total_ms: u64,
    /// Time spent in the post-scan tag-and-truncate pass (replaces frontend tagScaffolds).
    tag_ms: u64,
    /// Time inside the `scan_path` command (scanner_total_ms + tag_ms + spawn_blocking overhead).
    cmd_total_ms: u64,
    files_seen: u64,
    bytes_seen: u64,
    dirs_in_acc: u64,
}

impl From<(u64, u64, &ScanStats)> for ScanStatsEvent {
    fn from((cmd_ms, tag_ms, s): (u64, u64, &ScanStats)) -> Self {
        Self {
            mode: s.mode.clone(),
            mft_attempted: s.mft_attempted,
            mft_succeeded: s.mft_succeeded,
            mft_ms: s.mft_ms,
            walk_ms: s.walk_ms,
            build_tree_ms: s.build_tree_ms,
            scanner_total_ms: s.total_ms,
            tag_ms,
            cmd_total_ms: cmd_ms,
            files_seen: s.files_seen,
            bytes_seen: s.bytes_seen,
            dirs_in_acc: s.dirs_in_acc,
        }
    }
}

#[tauri::command]
fn list_scaffolds(state: State<'_, AppState>) -> Vec<Scaffold> {
    state.scaffolds.lock().unwrap().clone()
}

/// Fast size-only walk used to seed the progress-bar denominator before the
/// real scan starts. Single jwalk pass, sums file sizes, no other state.
#[tauri::command]
async fn estimate_size(path: String) -> Result<u64, String> {
    let p = PathBuf::from(&path);
    tokio::task::spawn_blocking(move || -> u64 {
        let mut total: u64 = 0;
        for entry in pinkbin_walker(&p).into_iter().flatten() {
            if entry.file_type().is_file() {
                if let Ok(md) = entry.metadata() {
                    total = total.saturating_add(md.len());
                }
            }
        }
        total
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(serde::Serialize, Clone)]
struct VolumeInfo {
    total_bytes: u64,
    used_bytes: u64,
    free_bytes: u64,
}

#[tauri::command]
fn volume_info(path: String) -> Result<VolumeInfo, String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut wide: Vec<u16> = std::ffi::OsStr::new(&path).encode_wide().collect();
        // Trim to drive root for the Win32 API.
        let root: Vec<u16> = if wide.len() >= 2 && wide[1] == b':' as u16 {
            vec![wide[0], wide[1], b'\\' as u16, 0]
        } else {
            wide.push(0);
            wide
        };
        let mut free_to_caller: u64 = 0;
        let mut total: u64 = 0;
        let mut total_free: u64 = 0;
        // SAFETY: calling documented Win32 API with valid wide-char buffers.
        let ok = unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetDiskFreeSpaceExW(
                    lpDirectoryName: *const u16,
                    lpFreeBytesAvailable: *mut u64,
                    lpTotalNumberOfBytes: *mut u64,
                    lpTotalNumberOfFreeBytes: *mut u64,
                ) -> i32;
            }
            GetDiskFreeSpaceExW(
                root.as_ptr(),
                &mut free_to_caller,
                &mut total,
                &mut total_free,
            )
        };
        if ok == 0 {
            return Err("GetDiskFreeSpaceExW failed".into());
        }
        Ok(VolumeInfo {
            total_bytes: total,
            used_bytes: total.saturating_sub(total_free),
            free_bytes: total_free,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("volume_info only implemented on Windows".into())
    }
}

#[tauri::command]
fn detect_scaffold(state: State<'_, AppState>, path: String) -> Option<String> {
    detect_for(
        &state.scaffolds.lock().unwrap(),
        std::path::Path::new(&path),
    )
}

#[derive(serde::Serialize, Clone)]
struct ScopeSize {
    scope_id: String,
    /// Bytes that would be cleaned at the current `scope_days` setting (i.e.
    /// files matching the glob AND older than retention). This is what the
    /// "X 待清理" pill in the modal renders.
    bytes: u64,
    file_count: u64,
    /// Bytes inside the scope **regardless of retention** — useful so the UI
    /// can show "你共有 12 GB 视频，0 GB 超过 90 天可清". Without this the
    /// modal can't distinguish "scope is empty" from "everything is within
    /// retention" (which used to render as a misleading "空").
    total_bytes: u64,
    total_files: u64,
}

/// Walk `root_path` and tally how many bytes / files each `[[scope]]` glob
/// True when `path`'s first `wxid_*` segment is in `allow`. Paths with no
/// `wxid_*` segment (e.g. `all_users/`, `%APPDATA%/Tencent/xwechat/log/`)
/// always pass — those are cross-account or roaming-only data that aren't
/// wxid-scoped. `None` or an empty allow-list disables the filter entirely.
fn path_passes_wxid(path: &Path, wxid_filter: Option<&[String]>) -> bool {
    let Some(allowed) = wxid_filter else {
        return true;
    };
    if allowed.is_empty() {
        return true;
    }
    for component in path.components() {
        if let Some(s) = component.as_os_str().to_str() {
            if s.starts_with("wxid_") {
                return allowed.iter().any(|w| w == s);
            }
        }
    }
    true
}

/// True when `path` has an `envs/<name>` segment whose `<name>` is in `allow`,
/// OR has no `envs/` segment at all (paths from non-env scopes pass through —
/// `pkgs/cache/foo`, `<conda-root>/python.exe`, etc.). Mirrors `path_passes_wxid`'s
/// "filter only narrows the targeted layer" semantics so a single
/// `execute_scope` call can carry both filters across mixed scopes.
/// `None` or empty allow-list disables the filter entirely.
fn path_passes_env(path: &Path, env_filter: Option<&[String]>) -> bool {
    let Some(allowed) = env_filter else {
        return true;
    };
    if allowed.is_empty() {
        return true;
    }
    let mut comps = path.components().peekable();
    while let Some(c) = comps.next() {
        if let Some(s) = c.as_os_str().to_str() {
            if s.eq_ignore_ascii_case("envs") {
                if let Some(next) = comps.peek() {
                    if let Some(name) = next.as_os_str().to_str() {
                        return allowed.iter().any(|n| n == name);
                    }
                }
                return false;
            }
        }
    }
    true
}

/// 系统级"垃圾/卷元数据"目录名（lowercase，比较时不区分大小写）。任何 scaffold
/// 路径扫描遇到这些目录直接整树跳过——回收站里的东西是用户已经决定先放着的，
/// 把它们当成可清理 cache 是事故；System Volume Information 是 VSS / 索引快照，
/// 既不可清理也容易触发权限拒绝拖慢扫描。
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

/// 本文件里所有 scaffold 侧路径扫描共用的 walker 构造器。两条策略集中在这里：
/// (a) `skip_hidden(false)`——很多 app cache 落在 dotted 目录里，必须能进；
/// (b) `process_read_dir` 在读目录时直接 prune 系统垃圾箱/卷元数据子树，
///     比"扫完再过滤路径"省一个数量级 IO，并彻底排除"glob 撞回收站"事故面。
/// scanner crate 的"主页大盘占用扫描"故意不走这里——那边用户期望看到回收站占用。
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

/// Walk `root` and return directories whose path matches `glob_set`, after
/// pruning any candidate whose ancestor is also matched. Used by directory-
/// granularity scopes (recycle the directory as one unit, not file-by-file).
///
/// **Why ancestor dedup**: globset is configured with `literal_separator(false)`
/// for backwards compat with media-bucket file scopes — meaning `*` crosses
/// `/`. A glob like `**/pkgs/*` matches both `pkgs/numpy` AND `pkgs/numpy/info`;
/// dedup keeps only the shallowest match per subtree so the recycle plan
/// touches each logical unit exactly once. `path == root` is dropped
/// unconditionally — even if a misconfigured glob hits root, recycling the
/// scan root would nuke the user's whole conda install.
fn find_matching_dirs(
    root: &Path,
    glob_set: &globset::GlobSet,
    wxid_filter: Option<&[String]>,
    env_filter: Option<&[String]>,
    older_than_days: Option<u32>,
) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for entry in pinkbin_walker(root).into_iter().flatten() {
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        if path == root {
            continue;
        }
        if !path_passes_wxid(&path, wxid_filter) || !path_passes_env(&path, env_filter) {
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
        if glob_set.is_match(&path_str) {
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
    keep
}

/// True when `metadata.modified()` is older than `now - days * 86400s`.
/// `days = None` skips the filter. Files whose mtime can't be read pass —
/// we don't silently drop data the user expects to see because of a transient
/// OS error.
fn mtime_older_than(metadata: &std::fs::Metadata, days: Option<u32>) -> bool {
    let Some(d) = days else { return true };
    let Ok(modified) = metadata.modified() else {
        return true;
    };
    let threshold = SystemTime::now()
        .checked_sub(Duration::from_secs(d as u64 * 86_400))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    modified <= threshold
}

/// in `scaffold_id` would match. Used by the Studio panel to show per-scope
/// occupancy alongside the generic "largest sub-items" view. Files matching
/// multiple scopes are counted once per matching scope (the same physical
/// bytes — overlap means cleaning either scope reclaims them).
#[tauri::command]
async fn scope_sizes(
    state: State<'_, AppState>,
    scaffold_id: String,
    root_path: String,
    scope_days: Option<HashMap<String, u32>>,
    wxid_filter: Option<Vec<String>>,
    env_filter: Option<Vec<String>>,
) -> Result<Vec<ScopeSize>, String> {
    let scaffold = state
        .scaffolds
        .lock()
        .unwrap()
        .iter()
        .find(|s| s.id == scaffold_id)
        .cloned()
        .ok_or_else(|| format!("scaffold not found: {scaffold_id}"))?;

    // Partition scopes by granularity. File scopes share one walk; directory
    // scopes each get find_matching_dirs + per-dir size sum. Mixed-granularity
    // scaffolds walk twice but each walk only does its own work.
    struct ScopeBuild {
        id: String,
        set: globset::GlobSet,
        granularity: RecycleGranularity,
    }
    let mut builds: Vec<ScopeBuild> = Vec::with_capacity(scaffold.scopes.len());
    for sc in &scaffold.scopes {
        let pattern = expand_env(&sc.glob);
        let glob = globset::GlobBuilder::new(&pattern)
            .literal_separator(false)
            .case_insensitive(true)
            .build()
            .map_err(|e| format!("scope `{}` has invalid glob `{}`: {e}", sc.id, sc.glob))?;
        let mut b = globset::GlobSetBuilder::new();
        b.add(glob);
        builds.push(ScopeBuild {
            id: sc.id.clone(),
            set: b.build().map_err(|e| e.to_string())?,
            granularity: sc.recycle_granularity,
        });
    }

    let root = PathBuf::from(&root_path);
    let wxid_filter_owned = wxid_filter;
    let env_filter_owned = env_filter;
    let scope_days_owned = scope_days;
    tokio::task::spawn_blocking(move || -> Vec<ScopeSize> {
        let mut results: Vec<ScopeSize> = Vec::with_capacity(builds.len());
        let file_indices: Vec<usize> = builds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.granularity == RecycleGranularity::File)
            .map(|(i, _)| i)
            .collect();
        let dir_indices: Vec<usize> = builds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.granularity == RecycleGranularity::Directory)
            .map(|(i, _)| i)
            .collect();

        // ── File scopes: single walk, tally per-file across all file scopes.
        // Track BOTH "eligible at the requested retention" (`tally`) and
        // "total in scope ignoring retention" (`total`) — UI uses the gap to
        // explain "X GB total · 0 GB older than 90d will be cleaned" instead
        // of the previous misleading "空".
        let mut tally: Vec<(u64, u64)> = vec![(0, 0); builds.len()];
        let mut total: Vec<(u64, u64)> = vec![(0, 0); builds.len()];
        if !file_indices.is_empty() {
            for entry in pinkbin_walker(&root).into_iter().flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.path();
                let metadata = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if !path_passes_wxid(&path, wxid_filter_owned.as_deref())
                    || !path_passes_env(&path, env_filter_owned.as_deref())
                {
                    continue;
                }
                let path_str = path.to_string_lossy().replace('\\', "/");
                let size = metadata.len();
                for &i in &file_indices {
                    let b = &builds[i];
                    if !b.set.is_match(&path_str) {
                        continue;
                    }
                    total[i].0 = total[i].0.saturating_add(size);
                    total[i].1 = total[i].1.saturating_add(1);
                    let days = scope_days_owned
                        .as_ref()
                        .and_then(|m| m.get(&b.id).copied());
                    if !mtime_older_than(&metadata, days) {
                        continue;
                    }
                    tally[i].0 = tally[i].0.saturating_add(size);
                    tally[i].1 = tally[i].1.saturating_add(1);
                }
            }
        }

        // ── Directory scopes: each gets its own dir walk + size sum.
        for &i in &dir_indices {
            let b = &builds[i];
            // Total: dirs matching the glob ignoring retention.
            let total_dirs = find_matching_dirs(
                &root,
                &b.set,
                wxid_filter_owned.as_deref(),
                env_filter_owned.as_deref(),
                None,
            );
            let mut t_bytes: u64 = 0;
            for d in &total_dirs {
                t_bytes = t_bytes.saturating_add(dir_size_excluding(d, &[]));
            }
            total[i] = (t_bytes, total_dirs.len() as u64);
            // Eligible: same but with retention applied.
            let days = scope_days_owned
                .as_ref()
                .and_then(|m| m.get(&b.id).copied());
            let dirs = find_matching_dirs(
                &root,
                &b.set,
                wxid_filter_owned.as_deref(),
                env_filter_owned.as_deref(),
                days,
            );
            let mut bytes: u64 = 0;
            let mut count: u64 = 0;
            for d in &dirs {
                bytes = bytes.saturating_add(dir_size_excluding(d, &[]));
                count = count.saturating_add(1);
            }
            tally[i] = (bytes, count);
        }

        // Build output preserving builds order.
        for (i, b) in builds.into_iter().enumerate() {
            results.push(ScopeSize {
                scope_id: b.id,
                bytes: tally[i].0,
                file_count: tally[i].1,
                total_bytes: total[i].0,
                total_files: total[i].1,
            });
        }
        results
    })
    .await
    .map_err(|e| e.to_string())
}

/// Resolve a scaffold scope's glob to actual file paths under `root_path` and
/// run the executor on just those files. Use this instead of feeding the
/// matched root directly into `execute_plan` — the latter would delete the
/// whole folder, ignoring the scope's glob. `dry_run = true` returns what
/// would be touched without performing the action.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // every arg is a distinct user-facing knob; bundling into a struct is just shuffling
async fn execute_scope(
    state: State<'_, AppState>,
    scaffold_id: String,
    scope_id: String,
    root_path: String,
    dry_run: bool,
    older_than_days: Option<u32>,
    wxid_filter: Option<Vec<String>>,
    env_filter: Option<Vec<String>>,
) -> Result<Vec<UndoEntry>, String> {
    let (scaffold, scope) = {
        let scaffolds = state.scaffolds.lock().unwrap();
        let sc = scaffolds
            .iter()
            .find(|s| s.id == scaffold_id)
            .cloned()
            .ok_or_else(|| format!("scaffold not found: {scaffold_id}"))?;
        let scope = sc
            .scopes
            .iter()
            .find(|s| s.id == scope_id)
            .cloned()
            .ok_or_else(|| format!("scope not found: {scaffold_id}/{scope_id}"))?;
        (sc, scope)
    };

    let pattern = expand_env(&scope.glob);
    let glob = globset::GlobBuilder::new(&pattern)
        .literal_separator(false)
        .case_insensitive(true)
        .build()
        .map_err(|e| format!("scope `{}` has invalid glob: {e}", scope.id))?;
    let mut b = globset::GlobSetBuilder::new();
    b.add(glob);
    let set = b.build().map_err(|e| e.to_string())?;

    let root = PathBuf::from(&root_path);
    let wxid_filter_owned = wxid_filter;
    let env_filter_owned = env_filter;
    let granularity = scope.recycle_granularity;

    let matched: Vec<PathBuf> = tokio::task::spawn_blocking(move || -> Vec<PathBuf> {
        match granularity {
            RecycleGranularity::Directory => find_matching_dirs(
                &root,
                &set,
                wxid_filter_owned.as_deref(),
                env_filter_owned.as_deref(),
                older_than_days,
            ),
            RecycleGranularity::File => {
                let mut out = Vec::new();
                for entry in pinkbin_walker(&root).into_iter().flatten() {
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    let p = entry.path();
                    let metadata = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    if !path_passes_wxid(&p, wxid_filter_owned.as_deref())
                        || !path_passes_env(&p, env_filter_owned.as_deref())
                        || !mtime_older_than(&metadata, older_than_days)
                    {
                        continue;
                    }
                    let s = p.to_string_lossy().replace('\\', "/");
                    if set.is_match(&s) {
                        out.push(p);
                    }
                }
                out
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    if matched.is_empty() {
        return Ok(Vec::new());
    }

    // Directory granularity is locked to Recycle regardless of scope.mode —
    // an entire directory removal is high cost to undo (rebuild conda env =
    // minutes to hours, project node_modules = redownload everything),
    // recoverability via Recycle Bin is non-negotiable. File granularity
    // honors the scope's declared mode (recycle/quarantine/delete) as before.
    let action = match (granularity, scope.mode) {
        (RecycleGranularity::Directory, _) => pinkbin_executor::Action::Recycle,
        (RecycleGranularity::File, pinkbin_scaffold::Mode::Recycle) => {
            pinkbin_executor::Action::Recycle
        }
        (RecycleGranularity::File, pinkbin_scaffold::Mode::Quarantine) => {
            pinkbin_executor::Action::Quarantine
        }
        (RecycleGranularity::File, pinkbin_scaffold::Mode::Delete) => {
            pinkbin_executor::Action::Delete
        }
    };
    let plan = Plan {
        action,
        paths: matched,
        reason: format!("Pinkbin scaffold {}/{} (Studio)", scaffold.id, scope.id),
    };
    execute(&plan, dry_run, &state.undo_log, &state.quarantine_root).map_err(|e| e.to_string())
}

/// Per-env metadata for the conda card's env list. `last_active_ts` is the
/// mtime of `<env>/conda-meta/history`, conda's own "last operation" timestamp
/// (install/remove/update). Pure `conda activate` does NOT update history —
/// "every-day-but-don't-install" envs may show stale; mode = recycle keeps
/// that recoverable. `default_checked` is the backend's recommendation
/// (!is_base && stale > 90d) that the UI uses to seed checkbox state.
#[derive(serde::Serialize, Clone)]
struct CondaEnv {
    name: String,
    path: String,
    size_bytes: u64,
    last_active_ts: Option<u64>,
    is_base: bool,
    default_checked: bool,
}

const CONDA_STALE_DAYS: u64 = 90;

#[tauri::command]
async fn list_conda_envs(conda_root: String) -> Result<Vec<CondaEnv>, String> {
    let root = PathBuf::from(&conda_root);
    if !root.exists() {
        return Err(format!("conda root does not exist: {conda_root}"));
    }
    tokio::task::spawn_blocking(move || -> Vec<CondaEnv> {
        let mut out = Vec::new();
        let now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stale_cutoff_secs = CONDA_STALE_DAYS * 86_400;

        // Base env occupies <root>/ itself. Exclude envs/ and pkgs/ subtrees
        // when summing — those have their own scope cards and shouldn't be
        // double-counted in the base row's size.
        let envs_subdir = root.join("envs");
        let pkgs_subdir = root.join("pkgs");
        let base_history = root.join("conda-meta/history");
        let base_last = read_mtime_secs(&base_history);
        let base_size = dir_size_excluding(&root, &[envs_subdir.clone(), pkgs_subdir]);
        out.push(CondaEnv {
            name: "base".to_string(),
            path: root.to_string_lossy().replace('\\', "/"),
            size_bytes: base_size,
            last_active_ts: base_last,
            is_base: true,
            default_checked: false,
        });

        // User envs at <root>/envs/<name>/
        if let Ok(rd) = std::fs::read_dir(&envs_subdir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if !p.is_dir() {
                    continue;
                }
                let name = match p.file_name() {
                    Some(n) => n.to_string_lossy().into_owned(),
                    None => continue,
                };
                let history = p.join("conda-meta/history");
                let last = read_mtime_secs(&history);
                let size = dir_size_excluding(&p, &[]);
                let default_checked = match last {
                    Some(ts) => now_secs.saturating_sub(ts) > stale_cutoff_secs,
                    // No history → no signal. Conservative: don't auto-select.
                    None => false,
                };
                out.push(CondaEnv {
                    name,
                    path: p.to_string_lossy().replace('\\', "/"),
                    size_bytes: size,
                    last_active_ts: last,
                    is_base: false,
                    default_checked,
                });
            }
        }
        out
    })
    .await
    .map_err(|e| e.to_string())
}

fn read_mtime_secs(p: &Path) -> Option<u64> {
    let md = std::fs::metadata(p).ok()?;
    let modified = md.modified().ok()?;
    modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Recursive byte sum under `dir`, skipping any file whose path starts with
/// one of the `excludes` prefixes (after normalizing slashes + lowercasing).
/// Used to compute base env size without counting the envs/ + pkgs/ subtrees.
fn dir_size_excluding(dir: &Path, excludes: &[PathBuf]) -> u64 {
    let exclude_prefixes: Vec<String> = excludes
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/").to_lowercase())
        .collect();
    let mut total: u64 = 0;
    for entry in pinkbin_walker(dir).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry
            .path()
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase();
        if exclude_prefixes.iter().any(|e| p.starts_with(e)) {
            continue;
        }
        if let Ok(md) = entry.metadata() {
            total = total.saturating_add(md.len());
        }
    }
    total
}

#[tauri::command]
async fn advise(
    state: State<'_, AppState>,
    req: AdvisorRequest,
) -> Result<AdvisorResponse, String> {
    let provider = state
        .advisor
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "advisor not configured — open Settings".to_string())?;
    advise_provider(&provider, &req)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn inspect_path(path: String, sample_count: usize) -> Vec<String> {
    sample_paths(&path, sample_count)
}

/// Open the OS file manager and reveal `path`. On Windows this uses
/// `explorer.exe /select,...` for files (so the file is highlighted) or just
/// the directory itself for directories. On macOS it's `open -R`. On Linux
/// it's `xdg-open` of the parent directory (no portable "select" verb).
#[tauri::command]
fn reveal_in_explorer(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("path does not exist: {path}"));
    }
    #[cfg(target_os = "windows")]
    {
        if p.is_dir() {
            std::process::Command::new("explorer")
                .arg(p)
                .spawn()
                .map_err(|e| e.to_string())?;
        } else {
            std::process::Command::new("explorer")
                .arg(format!("/select,{}", p.display()))
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(p)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if p.is_dir() {
            p.to_path_buf()
        } else {
            p.parent().unwrap_or(p).to_path_buf()
        };
        std::process::Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[tauri::command]
fn execute_plan(
    state: State<'_, AppState>,
    plan: Plan,
    dry_run: bool,
) -> Result<Vec<UndoEntry>, String> {
    execute(&plan, dry_run, &state.undo_log, &state.quarantine_root).map_err(|e| e.to_string())
}

/// Inspect the Steam install (registry + default paths). Returns a full
/// inventory: every library root, every installed/ghost game, with
/// recommendation flags pre-computed in the backend per design doc §6.5.
/// Frontend never has to recompute the dormancy heuristic.
#[tauri::command]
async fn list_steam_games() -> Result<pinkbin_steam_inspector::SteamInventory, String> {
    tokio::task::spawn_blocking(pinkbin_steam_inspector::inspect)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("steam inspect failed: {e:#}"))
}

/// Lazily enumerate every Workshop item under one game's
/// `<library>/steamapps/workshop/content/<appid>/`. Each item entry includes
/// the recursive size and folder mtime — slow enough that we do this on
/// click rather than during the bulk inspect.
#[tauri::command]
async fn list_steam_workshop_items(
    library_root: String,
    appid: u32,
) -> Result<Vec<pinkbin_steam_inspector::WorkshopItem>, String> {
    let path = PathBuf::from(library_root);
    tokio::task::spawn_blocking(move || pinkbin_steam_inspector::list_workshop_items(&path, appid))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("workshop scan failed: {e:#}"))
}

/// Resolve workshop item IDs to human-readable titles via Steam's public
/// `ISteamRemoteStorage/GetPublishedFileDetails` endpoint. No API key, no
/// auth — just a batched POST. Returns ID → title for every item Steam
/// reports `result: 1` (OK); deleted/private items are simply absent from
/// the map and the frontend renders the bare ID as fallback.
///
/// Network resilience: in mainland China `api.steampowered.com` is often
/// reachable only through a proxy, so we honor the Windows system proxy
/// (`HKCU\...\Internet Settings\ProxyEnable + ProxyServer`) — most VPN
/// clients (Clash, V2RayN, Shadowrocket-PC, etc.) write that key when
/// they're on. One automatic retry with 800ms backoff smooths transient
/// flakes. Frontend caches successful results in localStorage so demos
/// don't need to re-fetch over an unstable connection.
///
/// Privacy: the IDs we send are the same ones the user already received
/// from Steam by virtue of subscribing. We're not sending paths, names of
/// other games, or anything Steam doesn't already have. This goes to
/// Steam directly — never to an LLM (where opaque numeric IDs would be
/// useless anyway).
#[tauri::command]
async fn fetch_workshop_titles(ids: Vec<u64>) -> Result<HashMap<u64, String>, String> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    #[derive(serde::Deserialize)]
    struct Resp {
        response: RespInner,
    }
    #[derive(serde::Deserialize)]
    struct RespInner {
        #[serde(default)]
        publishedfiledetails: Vec<Item>,
    }
    #[derive(serde::Deserialize)]
    struct Item {
        publishedfileid: String,
        result: i32,
        #[serde(default)]
        title: Option<String>,
    }

    let mut form: Vec<(String, String)> = Vec::with_capacity(ids.len() + 1);
    form.push(("itemcount".to_string(), ids.len().to_string()));
    for (i, id) in ids.iter().enumerate() {
        form.push((format!("publishedfileids[{i}]"), id.to_string()));
    }

    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(10));
    if let Some(proxy_str) = system_https_proxy() {
        match reqwest::Proxy::all(&proxy_str) {
            Ok(p) => {
                tracing::info!("workshop title fetch: using system proxy {}", proxy_str);
                builder = builder.proxy(p);
            }
            Err(e) => {
                tracing::warn!(
                    "workshop title fetch: ignoring malformed system proxy {:?}: {}",
                    proxy_str,
                    e
                );
            }
        }
    }
    let client = builder
        .build()
        .map_err(|e| format!("steam api client build failed: {e}"))?;

    let url = "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";
    let mut last_err: Option<String> = None;
    let mut resp_opt: Option<reqwest::Response> = None;
    for attempt in 0..2u32 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
        match client.post(url).form(&form).send().await {
            Ok(r) => {
                resp_opt = Some(r);
                last_err = None;
                break;
            }
            Err(e) => {
                tracing::warn!("workshop title fetch attempt {} failed: {}", attempt + 1, e);
                last_err = Some(e.to_string());
            }
        }
    }
    let resp = resp_opt.ok_or_else(|| {
        format!(
            "Steam 服务器无响应（重试 1 次后仍失败）: {}",
            last_err.unwrap_or_else(|| "unknown".to_string())
        )
    })?;
    if !resp.status().is_success() {
        return Err(format!("Steam 服务器返回 HTTP {}", resp.status()));
    }
    let parsed: Resp = resp
        .json()
        .await
        .map_err(|e| format!("Steam 服务器响应解析失败: {e}"))?;
    let mut out: HashMap<u64, String> = HashMap::new();
    for item in parsed.response.publishedfiledetails {
        if item.result != 1 {
            continue; // 9 = deleted, 16 = banned, etc. — fall back to ID display.
        }
        if let (Ok(id), Some(title)) = (item.publishedfileid.parse::<u64>(), item.title) {
            if !title.is_empty() {
                out.insert(id, title);
            }
        }
    }
    Ok(out)
}

/// Read the system HTTPS proxy if one is configured, normalized into a URL
/// reqwest can consume. Windows-only — on mac/linux reqwest already honors
/// `HTTPS_PROXY` env var by default, which is the standard convention.
#[cfg(windows)]
fn system_https_proxy() -> Option<String> {
    let raw = read_windows_proxy_server()?;
    parse_proxy_server(&raw)
}

#[cfg(not(windows))]
fn system_https_proxy() -> Option<String> {
    None
}

/// Read `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`
/// and return the `ProxyServer` REG_SZ value when `ProxyEnable` is non-zero.
/// Returns the raw string Steam-side parsing will normalize.
#[cfg(windows)]
fn read_windows_proxy_server() -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
        REG_SZ,
    };

    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\0"
        .encode_utf16()
        .collect();

    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey)
            != ERROR_SUCCESS
        {
            return None;
        }

        // ProxyEnable (DWORD) — 0 = disabled.
        let mut proxy_enable: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let mut ty: u32 = 0;
        let name: Vec<u16> = "ProxyEnable\0".encode_utf16().collect();
        let res = RegQueryValueExW(
            hkey,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            &mut proxy_enable as *mut _ as *mut u8,
            &mut size,
        );
        if res != ERROR_SUCCESS || ty != REG_DWORD || proxy_enable == 0 {
            let _ = RegCloseKey(hkey);
            return None;
        }

        // ProxyServer (REG_SZ).
        let mut buf = [0u16; 512];
        let mut len = (buf.len() * 2) as u32;
        let mut ty: u32 = 0;
        let name: Vec<u16> = "ProxyServer\0".encode_utf16().collect();
        let res = RegQueryValueExW(
            hkey,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut ty,
            buf.as_mut_ptr() as *mut u8,
            &mut len,
        );
        let _ = RegCloseKey(hkey);
        if res != ERROR_SUCCESS || ty != REG_SZ {
            return None;
        }
        let u16_len = (len as usize / 2).saturating_sub(1);
        let s = OsString::from_wide(&buf[..u16_len]);
        s.to_str().map(|s| s.to_string())
    }
}

/// Normalize the IE `ProxyServer` value into a single URL reqwest can use.
/// Accepts the two formats Windows writes:
///   "127.0.0.1:7890"                                 → http://127.0.0.1:7890
///   "http=127.0.0.1:7890;https=127.0.0.1:7890;..."   → pick the https= entry
#[cfg(windows)]
fn parse_proxy_server(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let with_scheme = |s: &str| -> String {
        if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("socks5://") {
            s.to_string()
        } else {
            format!("http://{s}")
        }
    };
    if raw.contains('=') {
        // Per-protocol form. Prefer https=, fall back to http=.
        let parts: Vec<&str> = raw.split(';').map(|p| p.trim()).collect();
        if let Some(p) = parts.iter().find_map(|p| p.strip_prefix("https=")) {
            return Some(with_scheme(p));
        }
        if let Some(p) = parts.iter().find_map(|p| p.strip_prefix("http=")) {
            return Some(with_scheme(p));
        }
        return None;
    }
    Some(with_scheme(raw))
}

#[cfg(all(test, windows))]
mod proxy_parse_tests {
    use super::parse_proxy_server;

    #[test]
    fn single_value_gets_http_scheme() {
        assert_eq!(
            parse_proxy_server("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890"),
        );
    }

    #[test]
    fn already_scheme_kept_as_is() {
        assert_eq!(
            parse_proxy_server("http://10.0.0.1:8080").as_deref(),
            Some("http://10.0.0.1:8080"),
        );
    }

    #[test]
    fn per_protocol_prefers_https() {
        assert_eq!(
            parse_proxy_server("http=127.0.0.1:7890;https=127.0.0.1:7891;ftp=127.0.0.1:7892")
                .as_deref(),
            Some("http://127.0.0.1:7891"),
        );
    }

    #[test]
    fn per_protocol_falls_back_to_http_only() {
        assert_eq!(
            parse_proxy_server("http=127.0.0.1:7890;ftp=127.0.0.1:7891").as_deref(),
            Some("http://127.0.0.1:7890"),
        );
    }

    #[test]
    fn empty_returns_none() {
        assert_eq!(parse_proxy_server(""), None);
        assert_eq!(parse_proxy_server("   "), None);
    }
}

/// Hand off to Steam via its custom URI scheme. Action whitelist + numeric
/// id means the URL surface can't be poisoned by arbitrary frontend
/// strings — this is the only way a Steam Inspector destructive intent
/// (uninstall) leaves the app, and it's Steam itself that runs the action.
/// `id` is appid for game actions, or workshop file id for `url/CommunityFilePage`.
#[tauri::command]
fn open_steam_url(action: String, appid: u64) -> Result<(), String> {
    // Whitelist actions; `url/CommunityFilePage` is the workshop-item page.
    let url = match action.as_str() {
        "uninstall" | "rungameid" | "validate" | "nav" => format!("steam://{action}/{appid}"),
        "workshop_page" => format!("steam://url/CommunityFilePage/{appid}"),
        other => return Err(format!("unsupported steam action: {other}")),
    };

    #[cfg(target_os = "windows")]
    {
        // `cmd /c start "" "<url>"` is the standard Windows recipe for
        // launching a registered URL handler without inheriting the parent
        // window. The empty `""` is the start command's required title arg.
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn provider_from_config(
    provider: String,
    api_key: Option<String>,
    model: String,
    base_url: Option<String>,
) -> Result<Provider, String> {
    let p = match provider.as_str() {
        "openai" => Provider::OpenAI {
            api_key: api_key.ok_or_else(|| "api_key required".to_string())?,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
        },
        "anthropic" => Provider::Anthropic {
            api_key: api_key.ok_or_else(|| "api_key required".to_string())?,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
        },
        "ollama" => Provider::Ollama {
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434".to_string()),
            model,
        },
        "gemini" => Provider::Gemini {
            api_key: api_key.ok_or_else(|| "api_key required".to_string())?,
            model,
            base_url: base_url
                .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string()),
        },
        other => return Err(format!("unknown provider: {other}")),
    };
    Ok(p)
}

#[tauri::command]
fn set_advisor(
    state: State<'_, AppState>,
    provider: String,
    api_key: Option<String>,
    model: String,
    base_url: Option<String>,
) -> Result<(), String> {
    let p = provider_from_config(provider, api_key, model, base_url)?;
    *state.advisor.lock().unwrap() = Some(p);
    Ok(())
}

#[tauri::command]
async fn test_advisor(
    provider: String,
    api_key: Option<String>,
    model: String,
    base_url: Option<String>,
) -> Result<(), String> {
    let p = provider_from_config(provider, api_key, model, base_url)?;
    test_advisor_connection(&p).await.map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from(".pinkbin"));
            std::fs::create_dir_all(&data_dir).ok();
            let undo_log = data_dir.join("undo.jsonl");
            let quarantine_root = data_dir.join("quarantine");

            let scaffolds = load_all_scaffolds(app.handle());
            tracing::info!("loaded {} scaffolds", scaffolds.len());

            app.manage(AppState {
                scaffolds: Mutex::new(scaffolds),
                advisor: Mutex::new(None),
                quarantine_root,
                undo_log,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_path,
            estimate_size,
            list_scaffolds,
            detect_scaffold,
            scope_sizes,
            execute_scope,
            list_conda_envs,
            advise,
            inspect_path,
            reveal_in_explorer,
            execute_plan,
            set_advisor,
            test_advisor,
            volume_info,
            list_steam_games,
            list_steam_workshop_items,
            fetch_workshop_titles,
            open_steam_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn load_all_scaffolds(handle: &AppHandle) -> Vec<Scaffold> {
    use std::collections::HashMap;
    let mut by_id: HashMap<String, Scaffold> = HashMap::new();
    let mut candidates: Vec<PathBuf> = Vec::new();
    // Resource dir takes precedence — packaged scaffolds. Then user override
    // (app_data_dir) — community-contributed scaffolds win over bundled ones
    // with the same id.
    if let Ok(p) = handle.path().resource_dir() {
        candidates.push(p.join("scaffolds"));
    }
    candidates.push(PathBuf::from("scaffolds"));
    candidates.push(PathBuf::from("../../scaffolds"));
    candidates.push(PathBuf::from("../../../scaffolds"));
    if let Ok(p) = handle.path().app_data_dir() {
        candidates.push(p.join("scaffolds"));
    }
    for p in &candidates {
        if !p.exists() {
            continue;
        }
        match load_dir(p) {
            Ok(v) => {
                for s in v {
                    by_id.insert(s.id.clone(), s);
                }
            }
            Err(e) => tracing::warn!("could not load scaffolds from {:?}: {}", p, e),
        }
    }
    // Lowest-priority fallback: if no external source provided a given id,
    // fill it from the compile-time embed. This is what makes the portable
    // raw exe (cwd=Downloads, no resource_dir/scaffolds) usable.
    for f in EMBEDDED_SCAFFOLDS.files() {
        if f.path().extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Some(text) = f.contents_utf8() else {
            continue;
        };
        match parse_toml(text) {
            Ok(s) => {
                by_id.entry(s.id.clone()).or_insert(s);
            }
            Err(e) => {
                tracing::warn!("embedded scaffold parse error in {:?}: {}", f.path(), e)
            }
        }
    }
    let mut out: Vec<Scaffold> = by_id.into_values().collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Verifies `pinkbin_walker` skips system trash / volume metadata directories
    /// at directory-read time. Without this prune a scope glob with a leading
    /// `**` (literal_separator=false makes `**` cross `/`) can match files
    /// inside `$Recycle.Bin/<SID>/$R*/...` because Windows preserves the
    /// original directory tree there — meaning a "clean WeChat cache" preview
    /// would list files the user already chose to put in the trash.
    #[test]
    fn pinkbin_walker_skips_system_trash_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let normal = root.join("normal_dir");
        fs::create_dir_all(&normal).unwrap();
        fs::write(normal.join("keep.txt"), b"x").unwrap();

        for trashy in &[
            "$RECYCLE.BIN",
            "System Volume Information",
            ".Trash",
            ".Trashes",
        ] {
            let d = root.join(trashy);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("inside.txt"), b"x").unwrap();
        }

        let mut files: Vec<String> = pinkbin_walker(root)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_string_lossy().replace('\\', "/"))
            .collect();
        files.sort();

        assert!(
            files.iter().any(|p| p.ends_with("/normal_dir/keep.txt")),
            "walker must still visit non-system dirs, got: {files:?}"
        );
        for trashy in &[
            "$RECYCLE.BIN",
            "System Volume Information",
            ".Trash",
            ".Trashes",
        ] {
            assert!(
                !files.iter().any(|p| p.contains(trashy)),
                "walker leaked into pruned dir `{trashy}`: {files:?}"
            );
        }
    }

    #[test]
    fn pinkbin_walker_prune_is_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lower = root.join("$recycle.bin");
        fs::create_dir_all(&lower).unwrap();
        fs::write(lower.join("inside.txt"), b"x").unwrap();

        let leaked = pinkbin_walker(root)
            .into_iter()
            .flatten()
            .any(|e| e.file_type().is_file());
        assert!(
            !leaked,
            "lowercase $recycle.bin variant must also be pruned"
        );
    }
}
