//! Duplicate file detector — three-phase pipeline that minimizes IO.
//!
//! Phase 1: group by size (zero cost — metadata only)
//! Phase 2: hash first 4KB of each file in size-collision groups (1 read/file)
//! Phase 3: full SHA-256 of files that share both size AND head-hash
//!
//! Only Phase 3 reads the entire file, and it only runs on true candidates.
//! For a typical 500K-file disk, Phase 1 collapses 95%+ of files, Phase 2
//! eliminates another 90% of the remainder, so Phase 3 hashes <1% of files.
//!
//! The result is a list of `DupGroup`s, each containing files with identical
//! content. The caller (agent / CLI) decides which to keep — we never delete
//! anything here. Use `pinkbin_guard::check_dedup_group` before deleting.

use rayon::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A group of files with identical content.
#[derive(Debug, Serialize)]
pub struct DupGroup {
    /// Full SHA-256 hex of the shared content.
    pub hash: String,
    pub size: u64,
    pub files: Vec<DupFile>,
    /// How much space would be freed if we keep 1 copy and delete the rest.
    pub waste_bytes: u64,
}

/// A single file in a duplicate group.
#[derive(Debug, Serialize)]
pub struct DupFile {
    pub path: String,
    pub mtime: Option<String>,
}

/// Options for duplicate detection.
#[derive(Debug, Clone)]
pub struct DedupOptions {
    /// Skip files smaller than this (default 1 KB — tiny files are rarely
    /// worth deduplicating and pollute results).
    pub min_size: u64,
    /// How many bytes to hash in Phase 2 (default 4 KB).
    pub head_size: usize,
}

impl Default for DedupOptions {
    fn default() -> Self {
        Self {
            min_size: 1024,
            head_size: 4096,
        }
    }
}

/// Find all duplicate file groups under `root`.
///
/// Returns groups sorted by `waste_bytes` descending (biggest waste first),
/// so the caller can prioritize high-impact dedup.
pub fn find_duplicates(root: &Path, opts: &DedupOptions) -> Vec<DupGroup> {
    // ── Phase 1: group by size ──
    let by_size = phase1_group_by_size(root, opts.min_size);
    let total_size_groups = by_size.len();
    let size_candidates: Vec<(u64, Vec<PathBuf>)> = by_size
        .into_iter()
        .filter(|(_, paths)| paths.len() > 1)
        .collect();

    tracing::info!(
        "phase1: {} size groups with collisions (from {} total groups)",
        size_candidates.len(),
        total_size_groups
    );

    // ── Phase 2: head-hash within each size group ──
    let head_candidates = phase2_head_hash(&size_candidates, opts.head_size);
    tracing::info!(
        "phase2: {} head-hash collision groups",
        head_candidates.len()
    );

    // ── Phase 3: full hash ──
    let groups = phase3_full_hash(&head_candidates);
    tracing::info!("phase3: {} confirmed duplicate groups", groups.len());

    let mut groups = groups;
    groups.sort_by(|a, b| b.waste_bytes.cmp(&a.waste_bytes));
    groups
}

// ─────────────────────────── Phase 1 ───────────────────────────

fn phase1_group_by_size(root: &Path, min_size: u64) -> HashMap<u64, Vec<PathBuf>> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size = metadata.len();
        if size < min_size {
            continue;
        }
        by_size
            .entry(size)
            .or_default()
            .push(entry.path().to_path_buf());
    }
    by_size
}

// ─────────────────────────── Phase 2 ───────────────────────────

fn phase2_head_hash(
    size_groups: &[(u64, Vec<PathBuf>)],
    head_size: usize,
) -> Vec<HeadHashGroup> {
    size_groups
        .par_iter()
        .flat_map(|(size, paths)| {
            // Hash head of each file, group by (size, head_hash)
            let mut by_head: HashMap<String, Vec<PathBuf>> = HashMap::new();
            for p in paths {
                match hash_head(p, head_size) {
                    Some(h) => by_head.entry(h).or_default().push(p.clone()),
                    None => {} // skip unreadable files
                }
            }
            by_head
                .into_iter()
                .filter(|(_, v)| v.len() > 1)
                .map(|(head_hash, files)| HeadHashGroup {
                    size: *size,
                    head_hash,
                    files,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

struct HeadHashGroup {
    size: u64,
    head_hash: String,
    files: Vec<PathBuf>,
}

fn hash_head(path: &Path, head_size: usize) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; head_size];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    let hash = Sha256::digest(&buf);
    Some(hex_encode(&hash))
}

// ─────────────────────────── Phase 3 ───────────────────────────

fn phase3_full_hash(head_groups: &[HeadHashGroup]) -> Vec<DupGroup> {
    head_groups
        .par_iter()
        .flat_map(|hg| {
            let mut by_full: HashMap<String, Vec<PathBuf>> = HashMap::new();
            for p in &hg.files {
                match hash_full(p) {
                    Some(h) => by_full.entry(h).or_default().push(p.clone()),
                    None => {}
                }
            }
            by_full
                .into_iter()
                .filter(|(_, v)| v.len() > 1)
                .map(|(full_hash, files)| {
                    let waste = hg.size * (files.len() as u64 - 1);
                    DupGroup {
                        hash: full_hash,
                        size: hg.size,
                        waste_bytes: waste,
                        files: files
                            .iter()
                            .map(|p| DupFile {
                                path: p.to_string_lossy().replace('\\', "/"),
                                mtime: file_mtime_string(p),
                            })
                            .collect(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn hash_full(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536]; // 64 KB chunks
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hash = hasher.finalize();
    Some(hex_encode(&hash))
}

fn file_mtime_string(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let mtime = metadata.modified().ok()?;
    let secs = mtime
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(secs.to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn finds_exact_duplicates() {
        let dir = tempdir().unwrap();
        // Two identical files
        fs::write(dir.path().join("a.txt"), b"hello world, this is a test file").unwrap();
        fs::write(dir.path().join("b.txt"), b"hello world, this is a test file").unwrap();
        // One unique file
        fs::write(dir.path().join("c.txt"), b"completely different content here").unwrap();

        let opts = DedupOptions { min_size: 10, head_size: 4096 };
        let groups = find_duplicates(dir.path(), &opts);
        assert_eq!(groups.len(), 1, "should find exactly 1 dup group");
        assert_eq!(groups[0].files.len(), 2);
        assert!(groups[0].waste_bytes > 0);
    }

    #[test]
    fn ignores_files_below_min_size() {
        let dir = tempdir().unwrap();
        // 3-byte files — below default min_size of 1024
        fs::write(dir.path().join("a.txt"), b"abc").unwrap();
        fs::write(dir.path().join("b.txt"), b"abc").unwrap();

        let groups = find_duplicates(dir.path(), &DedupOptions::default());
        assert!(groups.is_empty(), "should not find dups below min_size");
    }

    #[test]
    fn different_content_same_size_not_dup() {
        let dir = tempdir().unwrap();
        // Same size, different content
        let content_a = b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; // 31 bytes
        let content_b = b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"; // 31 bytes
        fs::write(dir.path().join("a.bin"), content_a).unwrap();
        fs::write(dir.path().join("b.bin"), content_b).unwrap();

        let opts = DedupOptions { min_size: 10, head_size: 4096 };
        let groups = find_duplicates(dir.path(), &opts);
        assert!(groups.is_empty(), "same-size different-content should not be dup");
    }

    #[test]
    fn three_identical_files_one_group() {
        let dir = tempdir().unwrap();
        let content = b"identical content for testing dedup logic xxxxxx";
        fs::write(dir.path().join("a.txt"), content).unwrap();
        fs::write(dir.path().join("b.txt"), content).unwrap();
        fs::write(dir.path().join("c.txt"), content).unwrap();

        let opts = DedupOptions { min_size: 10, head_size: 4096 };
        let groups = find_duplicates(dir.path(), &opts);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 3);
        // waste = size * (3 - 1) = size * 2
        assert_eq!(groups[0].waste_bytes, content.len() as u64 * 2);
    }

    #[test]
    fn nested_directories_scanned() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("sub1").join("sub2");
        fs::create_dir_all(&sub).unwrap();
        let content = b"nested duplicate content for testing xxxxxxxxx";
        fs::write(dir.path().join("top.txt"), content).unwrap();
        fs::write(sub.join("deep.txt"), content).unwrap();

        let opts = DedupOptions { min_size: 10, head_size: 4096 };
        let groups = find_duplicates(dir.path(), &opts);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn groups_sorted_by_waste_descending() {
        let dir = tempdir().unwrap();
        // Small dup group
        let small = b"small dup content aa";
        fs::write(dir.path().join("s1.txt"), small).unwrap();
        fs::write(dir.path().join("s2.txt"), small).unwrap();
        // Large dup group
        let large = vec![b'X'; 5000];
        fs::write(dir.path().join("l1.txt"), &large).unwrap();
        fs::write(dir.path().join("l2.txt"), &large).unwrap();

        let opts = DedupOptions { min_size: 10, head_size: 4096 };
        let groups = find_duplicates(dir.path(), &opts);
        assert_eq!(groups.len(), 2);
        // Large group should be first (more waste)
        assert!(groups[0].waste_bytes > groups[1].waste_bytes);
    }

    #[test]
    fn empty_dir_no_crash() {
        let dir = tempdir().unwrap();
        let groups = find_duplicates(dir.path(), &DedupOptions::default());
        assert!(groups.is_empty());
    }
}
