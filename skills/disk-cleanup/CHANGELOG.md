# Changelog

## [0.1.0] — 2026-07-13

### Added
- 首个 QwenPaw 兼容版本,从 TRAE Skill 适配而来
- YAML frontmatter(name/description/version/author/updated/changelog)
- 渐进式披露:核心 SKILL.md ~75 行 + references/{workflow,safety-rules,mft-fix}.md
- CLI `--elevate` flag:UAC 自动提权重启,替代外部提权脚本
- MFT 快速路径默认开启:NTFS $MFT 直读,与原始 pinkbin 行为一致
- 16 个子命令:scan / inspect / scaffolds / preview / execute / analyze / dedup / inventory / leftovers / hibernate / pagefile / restore / migrate / uninstall / migrate-app
- Guard 白名单:15 条系统保护路径 + uninstall_string 危险命令校验
- 8 个内置 scaffold(已 embed 进二进制):system-temp / app-logs / browser-cache / wechat-pc / social-im / conda / winsxs / windows-old
- 三阶段重复文件检测(size → head-hash → full SHA-256)
- undo log:每次 execute 追加 JSONL,可手动回滚

### Security
- 19 条安全红线,覆盖删除/系统路径/用户数据/WinSxS/uninstall/migrate-app/提权
- guard `block` 时绝不执行;`--allow-system` 是最后逃生口,不主动推荐
- migrate-app 不删原目录,验证新路径可用后用户手动删

### Known Issues
- `ntfs` crate 在异常 MFT 记录上可能触发 unsafe 段段错误,`catch_unwind` 兜底,降级到 walkdir
- 跨平台编译需 x86_64-pc-windows-gnu target + mingw-w64

### Future Work
- **升级 `ntfs` 0.4 → `ntfs-core` 0.7.1**(待稳定后):
  - `ntfs-core` 是 `#![forbid(unsafe_code)]` 零 unsafe 实现,panic-free on crafted input,7 个 cargo-fuzz target + 100% 行覆盖,与 Sleuth Kit 交叉验证
  - 可彻底消除 MFT 段错误风险
  - **当前不升级的理由**:
    1. 现状已足够安全:QwenPaw 实测中 BPB panic 被 `catch_unwind` 正确兜住,优雅降级到 walkdir,JSON 正常输出 —— 设计目标已达成
    2. MFT 默认开启,但 catch_unwind 兜底,panic 时自动降级到 walkdir
    3. API 完全不兼容(`ntfs::Ntfs` → `ntfs_core::NtfsFs::open`),需重写 `crates/scanner/src/mft.rs` ~200 行 + 重新交叉编译 + 重新测试
    4. ntfs-core 0.7.1 发布于 2026-07-07(7 天前),稳定性未经长期验证,等沉淀 1-2 个月
  - **升级触发条件**:ntfs-core 发布 ≥ 3 个月 且 无重大 issue 报告,或用户明确要求升级
  - 验证基线:QwenPaw 2026-07-14 测试报告(MFT 失败但降级正常,walkdir 20s 扫完 F 盘 201,798 文件)
