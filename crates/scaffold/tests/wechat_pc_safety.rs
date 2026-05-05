//! Regression test for `scaffolds/wechat-pc.toml`: every scope glob must match
//! the paths it advertises, and **must not** match WeChat red lines (chat DBs,
//! account state, favorites, Moments, etc.). This file is the executable form
//! of the red-line clauses in `docs/scaffold-requirements/messaging.md`.
//!
//! Also asserts the [match] block's behavior — basename + must_have_child must
//! tag real data dirs (`xwechat_files`, `WeChat Files`) without false-positiving
//! on third-party dirs whose name happens to contain "wechat" (e.g. WPS Office's
//! `uploadwechatfile`, the install dir's `WeChatPlayer.bin`).

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // out of crates/scaffold
    p.pop(); // out of crates
    p
}

fn load_wechat_pc() -> pinkbin_scaffold::Scaffold {
    let path = workspace_root().join("scaffolds/wechat-pc.toml");
    let text = std::fs::read_to_string(&path).expect("read wechat-pc.toml");
    toml::from_str(&text).expect("parse wechat-pc.toml")
}

fn build_set(pattern: &str) -> globset::GlobSet {
    let g = globset::GlobBuilder::new(pattern)
        .literal_separator(false)
        .case_insensitive(true)
        .build()
        .unwrap_or_else(|e| panic!("bad glob `{pattern}`: {e}"));
    let mut b = globset::GlobSetBuilder::new();
    b.add(g);
    b.build().unwrap()
}

/// Mirror the env-var expansion used by `scaffold::expand_env` for `%VAR%` syntax.
fn expand(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(end) = bytes[i + 1..].iter().position(|&b| b == b'%') {
                let var = std::str::from_utf8(&bytes[i + 1..i + 1 + end]).unwrap_or("");
                if let Ok(v) = std::env::var(var) {
                    out.push_str(&v.replace('\\', "/"));
                    i += end + 2;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn matching_scopes<'a>(scopes: &'a [(String, globset::GlobSet)], path: &str) -> Vec<&'a str> {
    scopes
        .iter()
        .filter_map(|(id, gs)| {
            if gs.is_match(path) {
                Some(id.as_str())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn wechat_pc_globs_are_safe() {
    // Force env vars so the test is reproducible regardless of host.
    std::env::set_var("USERPROFILE", "C:/Users/test");
    std::env::set_var("APPDATA", "C:/Users/test/AppData/Roaming");

    let scaffold = load_wechat_pc();
    let scopes: Vec<(String, globset::GlobSet)> = scaffold
        .scopes
        .iter()
        .map(|s| (s.id.clone(), build_set(&expand(&s.glob))))
        .collect();

    // Every scope id should appear at least once in either positives or the
    // schema-only set. We assert positives below; a missing positive would
    // mean an unreachable scope.
    let positives: &[(&str, &str)] = &[
        ("chat-media-cache", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/cache/2026-05/Message/abc123/foo.dat"),
        ("web-resource-cache", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/cache/2026-04/HttpResource/x.png"),
        ("web-resource-cache", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/cache/2026-04/WeAppIcon/01/y.png"),
        ("sticker-cache", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/cache/2026-05/Emoticon/sticker.dat"),
        ("sticker-cache", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/business/emoticon/pack/x"),
        ("temp-files", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/temp/ImageTemp/x.jpg"),
        ("avatar-cache", "C:/Users/test/Documents/xwechat_files/all_users/head_imgs/aa/foo"),
        ("apm-records", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/apm_record/process_duration/x"),
        ("received-files", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/file/2026-05/note.pdf"),
        // 4.x 实测：image/video 共用 msg/video/<YYYY-MM>/ 目录，按后缀分流。
        // 真实命名包含 `<hash>.mp4`（视频）、`<hash>_raw.mp4`（高清版）、
        // `<hash>.jpg`（图片）、`<hash>_thumb.jpg`（视频缩略图）。
        ("received-videos", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/video/2026-05/abc123.mp4"),
        ("received-videos", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/video/2026-05/abc123_raw.mp4"),
        ("received-images", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/video/2026-05/photo123.jpg"),
        ("received-images", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/video/2026-05/abc123_thumb.jpg"),
        ("received-images", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/video/2026-05/sticker.png"),
        ("voice-attachments", "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/attach/abc/voice.amr"),
        ("chat-backups", "C:/Users/test/Documents/xwechat_files/Backup/wxid_aaa/data.bak"),
        ("app-logs-crashes", "C:/Users/test/AppData/Roaming/Tencent/xwechat/log/player/x.log"),
        ("app-logs-crashes", "C:/Users/test/AppData/Roaming/Tencent/xwechat/crashinfo/reports/x.dmp"),
        ("app-update-leftover", "C:/Users/test/AppData/Roaming/Tencent/xwechat/update/download/x.exe"),
        ("app-update-leftover", "C:/Users/test/AppData/Roaming/Tencent/xwechat/confsdk/x.cfg"),
        // 3.x 两套并存路径：path_one 顶层（Image/Image, Video, Files, Attachment）
        // + path_two FileStorage/* —— 都要被对应 scope 命中。
        ("image-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/Image/2026-05/img.dat"),
        ("image-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/Image/Image/2026-05/img.dat"),
        ("video-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/Video/clip.mp4"),
        ("video-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/Video/2026-05/clip.mp4"),
        ("file-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/File/note.pdf"),
        ("file-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/Files/2026-05/note.pdf"),
        ("voice-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/Voice2/2026-05/v.amr"),
        ("voice-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/Voice/old.amr"),
        ("msg-attach-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/MsgAttach/abc/Image/Thumb_x.dat"),
        ("sticker-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/Stickers/pack/x.png"),
        ("sticker-cache-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/Emotion/x.gif"),
        ("temp-files-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/Temp/dl_part.tmp"),
        ("cache-misc-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/Cache/x"),
        ("cache-misc-3x", "C:/Users/test/Documents/WeChat Files/wxid_legacy/Attachment/2026-05/thumb.dat"),
        ("app-logs-crashes-3x", "C:/Users/test/AppData/Roaming/Tencent/WeChat/Log/2026/x.log"),
        ("app-logs-crashes-3x", "C:/Users/test/AppData/Roaming/Tencent/WeChat/Logs/x.log"),
        ("app-logs-crashes-3x", "C:/Users/test/AppData/Roaming/Tencent/WeChat/CrashReport/x.dmp"),
        ("app-update-leftover-3x", "C:/Users/test/AppData/Roaming/Tencent/WeChat/Update/setup.exe"),
    ];

    for (expected_id, p) in positives {
        let hits = matching_scopes(&scopes, p);
        assert!(
            hits.contains(expected_id),
            "expected scope `{expected_id}` to match `{p}`, but matched {hits:?}",
        );
    }

    // 4.x 图片/视频共用 msg/video/ 树，按后缀分流。两个桶必须互斥——
    // 不允许 .mp4 同时落到 received-images，也不允许 .jpg 落到 received-videos。
    let cross_bucket: &[(&str, &str)] = &[
        (
            "received-images",
            "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/video/2026-05/abc.mp4",
        ),
        (
            "received-images",
            "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/video/2026-05/abc_raw.mp4",
        ),
        (
            "received-videos",
            "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/video/2026-05/abc.jpg",
        ),
        (
            "received-videos",
            "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/msg/video/2026-05/abc_thumb.jpg",
        ),
    ];
    for (forbidden_id, p) in cross_bucket {
        let hits = matching_scopes(&scopes, p);
        assert!(
            !hits.contains(forbidden_id),
            "scope `{forbidden_id}` must NOT match `{p}` (cross-bucket leak); got {hits:?}",
        );
    }

    // Red lines: chat history, account state, favorites, Moments, web-view K-V,
    // plugin/runtime dirs, and 3.x equivalents. None of these may match any scope.
    let red_lines: &[&str] = &[
        // 4.x chat DBs
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/db_storage/MMKV/data.db",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/db_storage/message/msg.db",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/db_storage/contact/contact.db",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/db_storage/emoticon/em.db",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/db_storage/favorite/fav.db",
        "C:/Users/test/Documents/xwechat_files/all_users/sqlite/lock.ini",
        "C:/Users/test/Documents/xwechat_files/all_users/sqlite/global.db",
        // 4.x account / login state
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/config/account.cfg",
        "C:/Users/test/Documents/xwechat_files/all_users/login/login.dat",
        "C:/Users/test/Documents/xwechat_files/all_users/config/global.cfg",
        // 4.x user-irreplaceable: favorites, moments, web-view K-V, editor
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/business/favorite/fav.dat",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/business/favorite/image/saved.png",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/business/sns/moments.db",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/business/sns/image/moment_pic.png",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/db_storage/image/index.db",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/business/xweb/mmkv/x.kv",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/business/xeditor/state.dat",
        "C:/Users/test/Documents/xwechat_files/wxid_aaa_bbb/business/migrate/x",
        // 4.x Roaming red lines
        "C:/Users/test/AppData/Roaming/Tencent/xwechat/All Users/x",
        "C:/Users/test/AppData/Roaming/Tencent/xwechat/XPlugin/plugin.dll",
        "C:/Users/test/AppData/Roaming/Tencent/xwechat/login/auth.dat",
        "C:/Users/test/AppData/Roaming/Tencent/xwechat/config/app.cfg",
        "C:/Users/test/AppData/Roaming/Tencent/xwechat/ilink/x",
        "C:/Users/test/AppData/Roaming/Tencent/xwechat/radium/x",
        "C:/Users/test/AppData/Roaming/Tencent/xwechat/net/x",
        "C:/Users/test/AppData/Roaming/Tencent/xwechat/net_1/x",
        "C:/Users/test/AppData/Roaming/Tencent/xwechat/uh/x",
        // 3.x red lines
        "C:/Users/test/Documents/WeChat Files/wxid_legacy/Msg/MSG_DB.sqlite",
        "C:/Users/test/Documents/WeChat Files/wxid_legacy/MultiMsg/x.db",
        "C:/Users/test/Documents/WeChat Files/wxid_legacy/config/account.cfg",
        "C:/Users/test/Documents/WeChat Files/wxid_legacy/Accounts/x.dat",
        "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/Fav/x",
        "C:/Users/test/Documents/WeChat Files/wxid_legacy/Favorites/x",
        // 3.x soft red line: CustomEmotion is the user's saved sticker panel,
        // not a system cache. sticker-cache-3x must NOT touch this.
        "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/CustomEmotion/123abc.gif",
        "C:/Users/test/Documents/WeChat Files/wxid_legacy/FileStorage/CustomEmotion/sub/x.png",
        // 3.x roaming red lines: account/login state, plugin runtime
        "C:/Users/test/AppData/Roaming/Tencent/WeChat/AccInfo.dat",
        "C:/Users/test/AppData/Roaming/Tencent/WeChat/All Users/x",
        "C:/Users/test/AppData/Roaming/Tencent/WeChat/login/auth.dat",
        "C:/Users/test/AppData/Roaming/Tencent/WeChat/XPlugin/plugin.dll",
    ];

    let mut violations = Vec::new();
    for p in red_lines {
        let hits = matching_scopes(&scopes, p);
        if !hits.is_empty() {
            violations.push(format!("`{p}` -> {hits:?}"));
        }
    }
    assert!(
        violations.is_empty(),
        "wechat-pc.toml glob hit red lines:\n  {}",
        violations.join("\n  ")
    );
}

/// The [match] block must tag real WeChat data roots and reject install dirs
/// or third-party dirs whose name happens to contain "wechat". This guards
/// against the WPS Office / WeChatPlayer.bin false positives that prompted
/// the must_have_child = ["all_users"] tightening.
///
/// We test against the LIVE filesystem via `tempdir`-style fixtures: the
/// must_have_child check uses `path.join("all_users").exists()`, so we have
/// to actually create the marker subdirectory for positives and leave it out
/// for negatives.
#[test]
fn wechat_pc_matcher_distinguishes_data_from_installs() {
    let scaffolds = vec![load_wechat_pc()];
    let tmp = std::env::temp_dir().join(format!(
        "pinkbin-wechat-matcher-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    // Positive: 4.x data root with `all_users/` child.
    let pos_4x = tmp.join("Documents/xwechat_files");
    std::fs::create_dir_all(pos_4x.join("all_users")).unwrap();

    // Positive: 3.x data root with `All Users/` child (Windows NTFS resolves
    // path.join("all_users") case-insensitively to this; on case-sensitive
    // platforms this positive is skipped — we only assert on Windows).
    let pos_3x = tmp.join("Documents/WeChat Files");
    std::fs::create_dir_all(pos_3x.join("All Users")).unwrap();

    // Negative: WPS Office "uploadwechatfile" — basename contains "wechat" but
    // is not a WeChat data root. No `all_users` subfolder.
    let neg_wps = tmp.join("AppData/Local/Kingsoft/WPS/uploadwechatfile");
    std::fs::create_dir_all(&neg_wps).unwrap();

    // Negative: WeChat install dir's `WeChatPlayer.bin` resource folder.
    let neg_install = tmp.join("Program Files/Tencent/Weixin/4.1.9.30/WeChatPlayer.bin");
    std::fs::create_dir_all(&neg_install).unwrap();

    // Negative: a sibling that has "wechat" in the name AND happens to also
    // contain something — but no `all_users`. Make sure we don't false-positive.
    let neg_random = tmp.join("Random/MyWeChatBackup");
    std::fs::create_dir_all(neg_random.join("photos")).unwrap();

    let assert_match = |path: &Path, expected: Option<&str>, label: &str| {
        let got = pinkbin_scaffold::detect_for(&scaffolds, path);
        assert_eq!(
            got.as_deref(),
            expected,
            "{label}: detect_for({path:?}) = {got:?}, expected {expected:?}",
        );
    };

    assert_match(&pos_4x, Some("wechat-pc"), "real 4.x data root");
    #[cfg(windows)]
    assert_match(
        &pos_3x,
        Some("wechat-pc"),
        "real 3.x data root (case-insensitive must_have_child)",
    );
    assert_match(&neg_wps, None, "WPS uploadwechatfile must not match");
    assert_match(
        &neg_install,
        None,
        "WeChatPlayer.bin install dir must not match",
    );
    assert_match(&neg_random, None, "MyWeChatBackup must not match");

    // Also confirm compile_all + detect_compiled agree (the production hot path).
    let compiled = pinkbin_scaffold::compile_all(&scaffolds);
    let assert_compiled = |path: &Path, expected: Option<&str>, label: &str| {
        let got = pinkbin_scaffold::detect_compiled(&compiled, path);
        assert_eq!(
            got.as_deref(),
            expected,
            "{label} (compiled): detect_compiled({path:?}) = {got:?}, expected {expected:?}",
        );
    };
    assert_compiled(&pos_4x, Some("wechat-pc"), "real 4.x data root");
    assert_compiled(&neg_wps, None, "WPS uploadwechatfile must not match");
    assert_compiled(
        &neg_install,
        None,
        "WeChatPlayer.bin install dir must not match",
    );
    assert_compiled(&neg_random, None, "MyWeChatBackup must not match");

    // Cleanup — best-effort, ignore errors.
    let _ = std::fs::remove_dir_all(&tmp);
}
