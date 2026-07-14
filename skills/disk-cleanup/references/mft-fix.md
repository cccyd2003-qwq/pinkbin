# MFT 扫描故障与 `--elevate` 提权说明

本文档记录 MFT 快速路径的已知问题、根因、规避方案,以及 `--elevate` 提权机制的工作流。Agent 遇到扫描相关异常时可参考。

## 1. MFT 是什么,为什么默认不用

NTFS 卷的 Master File Table($MFT)是卷上所有文件/目录的元数据索引。直接读 $MFT 比递归遍历目录快得多(WizTree-class):百万文件的 C 盘扫描可从分钟级降到秒级。

但本 skill **默认走 walkdir,不走 MFT**,原因:

### 根因:`ntfs` crate 段错误

`ntfs` crate(v0.4)在解析某些异常 MFT 记录时会触发 **unsafe 段的 access violation**(SIGSEGV)。Rust 的 `std::panic::catch_unwind` 只能捕获 panic,**接不住段错误**——段错误会直接让整个 CLI 进程 abort,本次扫描无任何输出。

异常 MFT 记录的常见来源:
- 卷有损坏扇区(CHKDSK 报错)
- ReFS / exFAT 卷被误判为 NTFS
- 某些被 VSS 锁定的卷影副本
- 第三方文件系统过滤驱动干扰

### 规避方案

`ScanOptions.use_mft` 字段默认 `false`,只有 CLI 显式传 `--mft` 时才尝试 MFT。失败时(返回 Err)自动 fallback 到 walkdir。

**但段错误不是 Err,是进程 abort**——所以 `--mft` 一旦崩溃,本次扫描彻底无输出。这是已知风险,由调用方承担。

## 2. 何时考虑 `--mft`

只有满足**全部**条件才建议:

1. 用户**主动抱怨**扫描太慢(不是 agent 自己觉得慢)
2. 用户**明确接受**崩溃风险(扫描可能直接 abort 无输出)
3. 用户**愿意点 UAC 提示**(读卷设备 `\\.\C:` 需要管理员权限)
4. 扫描目标是 NTFS 系统盘(C:\)且文件数 > 50 万

不满足任意一条 → 走默认 walkdir。

## 3. `--mft` 用法

```
skills/disk-cleanup/bin/pinkbin-cli.exe --elevate analyze "C:\\" --top 50 --mft
```

- `--elevate` 是全局 flag,必须在子命令之前
- `--mft` 是 `scan` / `analyze` 子命令的 flag
- 二者必须同时使用(MFT 需要管理员)

输出 JSON 的 `stats` 字段会显示:
- `mode: "mft"` 或 `"walkdir"`(实际走了哪条路径)
- `mft_attempted: true/false`
- `mft_succeeded: true/false`(false 表示 fallback 到 walkdir)
- `mft_ms` / `walk_ms` / `total_ms`(各阶段耗时)

## 4. `--elevate` 提权机制

许多操作需要管理员权限:`scan --mft` / `hibernate off` / `pagefile migrate` / `restore delete-all` / `execute winsxs` / `execute windows-old`。

### 工作流

```
agent 调用:pinkbin-cli.exe --elevate <subcommand> ...
    ↓
main() 检测 --elevate flag
    ↓
elevate::is_elevated() 检测当前权限
    ↓
若已提权 → 移除 --elevate,正常执行
若未提权 → 创建临时输出文件,ShellExecuteExW "runas" 重启自己
    ↓
用户点 UAC 同意 → 子进程(管理员)启动
    ↓
子进程检测 --elevated-output <tmpfile>(父进程注入)
    ↓
elevate::redirect_stdout_to_file() 把 stdout 重定向到 tmpfile
    ↓
子进程正常执行,输出写到 tmpfile
    ↓
父进程 WaitForSingleObject 等待子进程退出
    ↓
父进程读取 tmpfile 透传到自己 stdout,exit code 透传
    ↓
删除 tmpfile,父进程退出
```

### 失败处理

用户拒绝 UAC 或系统策略禁止提权时:

```json
{
  "error": "elevation_failed",
  "message": "提权重启失败: ShellExecuteExW 'runas' 失败: ... (用户可能拒绝了 UAC 提示)",
  "hint": "用户可能拒绝了 UAC 提示,或系统策略禁止提权。可让用户手动以管理员身份运行。"
}
```

agent 看到这个 JSON → **不重试** `--elevate` → 改为引导用户:
> "请右键点击 cmd 或 PowerShell,选择'以管理员身份运行',然后执行以下命令:..."

### 重要:stdout 重定向时机

Rust 的 stdout 在第一次使用时会缓存 `STD_OUTPUT_HANDLE`。子进程必须在任何 `println!` / `serde_json::to_writer(stdout)` 之前调用 `redirect_stdout_to_file()`,否则 `SetStdHandle` 调用对已缓存的 stdout 无效。

本 skill 的 `main()` 在 `Cli::parse()` 之后立即检查 `--elevated-output`,在任何输出之前完成重定向。

## 5. 跨平台注意

- `--elevate` 只在 Windows 平台支持(用 `ShellExecuteExW` + `OpenProcessToken`)
- 非 Windows 平台 `is_elevated()` 返回 `true`(避免无限循环),`relaunch_elevated()` 返回 `Unsupported` 错误
- `--mft` 只在 Windows + NTFS 卷有效;非 Windows / 非 NTFS 自动 fallback 到 walkdir

## 6. 排障表

| 症状 | 可能原因 | 处理 |
|---|---|---|
| `--mft` 扫描中 CLI 进程消失(无输出) | `ntfs` crate 段错误 abort | 改用 walkdir(去掉 `--mft`) |
| `--mft` 返回 `mft_succeeded: false` 但有输出 | MFT 打开失败(非管理员/非 NTFS),已 fallback | 正常,实际走了 walkdir |
| `--elevate` 返回 `elevation_failed` JSON | 用户拒绝 UAC 或系统策略禁止 | 引导用户手动以管理员身份运行 |
| `--elevate` 子进程输出未透传 | tmpfile 路径无写权限 / 磁盘满 | 检查 `%TEMP%` 可写 |
| `analyze` 输出 `stats.mode: "walkdir"` 但很慢 | 大目录 walkdir 本身慢 | 考虑 `--mft`(需用户同意)或缩小扫描范围 |
