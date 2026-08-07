//! Regression test for `scaffolds/netease-cloud-music.toml`.
//!
//! The scaffold is intentionally narrow: it covers only re-downloadable
//! caches, temp files, logs and update payloads. NetEase's `Library`,
//! `webdata`, and `dumps/cookie_json` areas can contain user state or login
//! material and must never be matched.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // out of crates/scaffold
    p.pop(); // out of crates
    p
}

fn load_scaffold() -> pinkbin_scaffold::Scaffold {
    let path = workspace_root().join("scaffolds/netease-cloud-music.toml");
    let text = std::fs::read_to_string(&path).expect("read netease-cloud-music.toml");
    toml::from_str(&text).expect("parse netease-cloud-music.toml")
}

fn build_set(pattern: &str) -> globset::GlobSet {
    let pattern = pinkbin_scaffold::expand_env(pattern);
    let glob = globset::GlobBuilder::new(&pattern)
        .literal_separator(false)
        .case_insensitive(true)
        .build()
        .unwrap_or_else(|e| panic!("bad glob `{pattern}`: {e}"));
    let mut builder = globset::GlobSetBuilder::new();
    builder.add(glob);
    builder.build().unwrap()
}

fn matching_scopes<'a>(
    scopes: &'a [(String, globset::GlobSet)],
    path: &str,
) -> Vec<&'a str> {
    scopes
        .iter()
        .filter_map(|(id, set)| set.is_match(path).then_some(id.as_str()))
        .collect()
}

#[test]
fn netease_cloud_music_globs_are_safe() {
    std::env::set_var("LOCALAPPDATA", "C:/Users/test/AppData/Local");
    std::env::set_var("APPDATA", "C:/Users/test/AppData/Roaming");

    let scaffold = load_scaffold();
    let scopes: Vec<(String, globset::GlobSet)> = scaffold
        .scopes
        .iter()
        .map(|scope| (scope.id.clone(), build_set(&scope.glob)))
        .collect();

    // Every scope gets at least one realistic positive path. Include both the
    // current versioned webapp name and the older unsuffixed form.
    let positives: &[(&str, &str)] = &[
        (
            "playback-cache",
            "C:/Users/test/AppData/Local/NetEase/CloudMusic/Cache/Cache/track-001.dat",
        ),
        (
            "static-assets",
            "C:/Users/test/AppData/Local/NetEase/CloudMusic/Statics/album-cover.dat",
        ),
        (
            "web-cache",
            "C:/Users/test/AppData/Local/NetEase/CloudMusic/webapp91x64/Cache/data_0",
        ),
        (
            "web-cache",
            "D:/Portable/NetEase/CloudMusic/webapp/Code Cache/js/index.bin",
        ),
        (
            "web-cache",
            "C:/Users/test/AppData/Local/NetEase/CloudMusic/webapp91x64/GPUCache/data_3",
        ),
        (
            "temp-files",
            "C:/Users/test/AppData/Local/NetEase/CloudMusic/Temp/session-123/part.tmp",
        ),
        (
            "app-logs",
            "C:/Users/test/AppData/Local/NetEase/CloudMusic/Log/BI/output/client.log",
        ),
        (
            "main-log",
            "C:/Users/test/AppData/Local/NetEase/CloudMusic/cloudmusic.elog",
        ),
        (
            "update-cache",
            "C:/Users/test/AppData/Local/NetEase/CloudMusic/update/orpheus_install.exe",
        ),
    ];

    for (expected_id, path) in positives {
        let hits = matching_scopes(&scopes, path);
        assert!(
            hits.contains(expected_id),
            "expected scope `{expected_id}` to match `{path}`, got {hits:?}",
        );
    }

    // Red lines: user library, playback state, cookies/login material,
    // persisted web state, app resources and downloaded music outside the
    // explicitly scoped cache buckets. None may match any scope.
    let red_lines: &[&str] = &[
        "C:/Users/test/AppData/Local/NetEase/CloudMusic",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/Library/library.dat",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/Library/webdb.dat",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/Library/webdb.dat-journal",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/webdata/file/playingList",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/webdata/file/recentListen",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/dumps/cookie_json",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/localdata",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/localware",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/aioresource/2332204/model_192k",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/webapp91x64/Cookies",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/webapp91x64/Local Storage/leveldb/000003.log",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/webapp91x64/Session Storage/leveldb/000003.log",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/webapp91x64/IndexedDB/orpheus/000003.log",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/webapp91x64/databases/user.db",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/webapp91x64/blob_storage/blob.bin",
        "C:/Users/test/AppData/Local/NetEase/CloudMusic/Cache/other/index.dat",
        "D:/Music/网易云下载/我的歌曲.flac",
        "D:/Music/网易云下载/我的歌曲.ncm",
        "C:/Users/test/Documents/CloudMusic/Library/library.dat",
    ];

    let mut violations = Vec::new();
    for path in red_lines {
        let hits = matching_scopes(&scopes, path);
        if !hits.is_empty() {
            violations.push(format!("`{path}` -> {hits:?}"));
        }
    }
    assert!(
        violations.is_empty(),
        "netease-cloud-music.toml glob hit red lines:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn netease_cloud_music_detection_requires_the_client_layout() {
    let scaffold = load_scaffold();
    let scaffolds = vec![scaffold];

    for path in [
        "C:/Users/test/AppData/Local/NetEase/CloudMusic",
        "D:/Portable/NetEase/CloudMusic",
        "C:/Users/test/AppData/Roaming/NetEase/CloudMusic",
    ] {
        assert_eq!(
            pinkbin_scaffold::detect_for(&scaffolds, std::path::Path::new(path)).as_deref(),
            Some("netease-cloud-music"),
            "missed real NetEase CloudMusic root {path}",
        );
    }

    for path in [
        "C:/Users/test/Documents/CloudMusic",
        "C:/Program Files/NetEase/CloudMusic.exe",
        "C:/Users/test/AppData/Local/Netease/OtherMusic",
    ] {
        assert_eq!(
            pinkbin_scaffold::detect_for(&scaffolds, std::path::Path::new(path)),
            None,
            "false-positive NetEase match for {path}",
        );
    }
}
