# MFT 快速路径说明

本文档记录 MFT 快速路径的默认行为、已知风险与降级机制。Agent 遇到扫描相关异常时可参考。

## 1. MFT 是什么

NTFS 卷的 Master File Table($MFT)是卷上所有文件/目录的元数据索引。直接读 $MFT 比递归遍历目录快得多(WizTree-class):百万文件的 C 盘扫描可从分钟级降到秒级。

## 2. 默认行为

MFT 快速路径**默认开启**,与原始 pinkbin 行为一致:

- 扫描时自动尝试 NTFS $MFT 直读
- `stats.mode` 字段会显示 `"mft"`(成功)或 `"walkdir"`(降级)
- 不再需要 `--mft` 标志——该标志已从 CLI 中移除
- 用户无需手动干预,也不建议用户主动关闭

## 3. 降级与安全机制

### panic 兜底

`ntfs` crate(v0.4)在解析某些异常 MFT 记录时会触发 panic。Rust 的 `std::panic::catch_unwind` 可以捕获这类 panic,自动降级到 walkdir 继续扫描,JSON 正常输出。

### 段错误风险(已知)

极少数情况下,`ntfs` crate 的 unsafe 代码可能触发 access violation(SIGSEGV),导致段错误。`catch_unwind` **接不住段错误**——段错误会直接让整个 CLI 进程 abort,本次扫描无任何输出。

这是原始 pinkbin 项目已知且接受的风险,本 skill 沿用相同策略:

- 段错误概率极低,仅在异常 MFT 记录上触发
- 速度收益远大于风险(秒级 vs 分钟级)
- 与原始 pinkbin 一致,不主动建议用户关闭

异常 MFT 记录的常见来源:
- 卷有损坏扇区(CHKDSK 报错)
- ReFS / exFAT 卷被误判为 NTFS
- 某些被 VSS 锁定的卷影副本
- 第三方文件系统过滤驱动干扰

## 4. 输出字段

扫描结果 JSON 的 `stats` 字段包含:
- `mode`: `"mft"` 或 `"walkdir"`(实际走了哪条路径)
- `mft_attempted: true/false`
- `mft_succeeded: true/false`(false 表示 fallback 到 walkdir)
- `mft_ms` / `walk_ms` / `total_ms`(各阶段耗时)

## 5. 跨平台注意

- MFT 快速路径只在 Windows + NTFS 卷有效
- 非 Windows / 非 NTFS 自动 fallback 到 walkdir,不影响正常使用

## 6. 排障表

| 症状 | 可能原因 | 处理 |
|---|---|---|
| 扫描中 CLI 进程消失(无输出) | `ntfs` crate 段错误 abort | 已知风险,重新运行即可(下次可能成功) |
| `stats.mode: "walkdir"` 但很慢 | MFT 打开失败,已 fallback 到 walkdir | 正常行为,实际走了 walkdir 遍历 |
| 返回 `mft_succeeded: false` 但有输出 | MFT 打开失败(非管理员/非 NTFS),已 fallback | 正常,无需处理 |