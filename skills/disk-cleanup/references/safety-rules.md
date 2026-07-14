# 安全红线(20 条)

绝对不可违反。按类别分组,每条带说明 + 触发场景 + 正确做法。

## 一、删除操作(5 条)

| # | 规则 | 说明 |
|---|---|---|
| 1 | **永远走回收站** | `--dry-run false` 时默认进回收站,除非用户明确说"直接删/硬删" |
| 2 | **永远先 preview 再 execute** | 给用户看要删什么,等用户确认后才执行 |
| 3 | **删除前必须告知释放空间** | 等用户确认将释放多少空间后才执行 |
| 4 | **`--hard-delete` 几乎不用** | 且对 directory-granularity scope 会被 CLI 拒绝 |
| 5 | **`--allow-system` 不主动推荐** | 这是最后的逃生口,不是常规操作;preview 的 `guard_blocked` 非空时,直接告诉用户"这些路径受系统保护,不可清理",不尝试绕过 |

## 二、系统路径保护(2 条)

| # | 规则 | 说明 |
|---|---|---|
| 6 | **永不碰系统关键路径** | `C:\Windows`、`C:\Windows\System32`、`C:\Windows\SysWOW64`、`C:\Windows\WinSxS`、`C:\Windows\Boot`、`C:\Boot`、`C:\EFI`、`C:\$Recycle.Bin`、`C:\System Volume Information`、`C:\Program Files`、`C:\Program Files (x86)`、`C:\ProgramData` —— guard 自动拦截 |
| 7 | **WinSxS 只能走 DISM** | system_cmd 模式,绝不能直接删 WinSxS 目录下的文件(guard 会 Block);`/ResetBase` 不可逆,必须让用户明确确认"不需要回滚更新" |

## 三、用户数据(1 条)

| # | 规则 | 说明 |
|---|---|---|
| 8 | **用户数据不可删** | `Documents`/`Pictures`/`Music`/`Videos`/`Desktop` 下的东西(guard 会 Warn,但仍需 agent 判断是否阻止) |

## 四、重复文件(1 条)

| # | 规则 | 说明 |
|---|---|---|
| 9 | **dedup 只检测不删除** | 重复文件可能是用户有意保留的备份,删除前必须逐组让用户确认;WinSxS 硬链接副本由 guard 拦截,不可绕过 |

## 五、软件卸载(2 条)

| # | 规则 | 说明 |
|---|---|---|
| 10 | **不主动调 `uninstall_string`** | 它可能弹 UAC/GUI 或执行危险操作;卸载引导用户走系统设置,卸载后用 `leftovers` 扫残留 |
| 11 | **uninstall guard block 时绝不执行** | 卸载串含 format/diskpart 等危险命令时,告知用户该卸载串危险,不可执行;多匹配时不猜测,让用户提供精确名称 |

## 六、系统命令(2 条)

| # | 规则 | 说明 |
|---|---|---|
| 12 | **system_cmd scope 需管理员权限时,必须提前告知用户** | `requires_admin: true` 时,非管理员运行会失败;不要试图绕过 UAC |
| 13 | **WinSxS ResetBase 不可逆** | 必须让用户明确确认"不需要回滚更新"才执行 |

## 七、系统优化(4 条)

| # | 规则 | 说明 |
|---|---|---|
| 14 | **关闭休眠前必须告知影响** | 关闭 hibernate 后无法休眠、快速启动也会失效;`hibernate size` 的 percent 必须 50-100 |
| 15 | **还原点删除不可逆** | `restore delete-all` 会删除所有还原点,之后无法通过系统还原回滚;执行前必须让用户明确确认 |
| 16 | **pagefile 迁移需重启生效** | 执行后告知用户必须重启;目标盘必须有足够空间(至少物理内存大小) |
| 17 | **migrate 只分析不执行** | 软件迁移风险高(注册表/快捷方式/依赖路径),引导用户用软件自带迁移功能或卸载重装,不要用 robocopy + 改注册表 |

## 八、软件迁移(1 条)

| # | 规则 | 说明 |
|---|---|---|
| 18 | **migrate-app 不删原目录** | 迁移后保留原目录,让用户验证新路径可用后再手动删;guard block 的源路径(System32/Windows 等)绝不迁移;迁移前必须 dry-run 让用户知情硬编码路径风险 |

## 九、MFT 与提权(2 条)

| # | 规则 | 说明 |
|---|---|---|
| 19 | **`--mft` 默认禁用,不主动推荐** | `ntfs` crate 在异常 MFT 记录上会段错误导致 CLI abort,`catch_unwind` 接不住;只在用户明确抱怨扫描慢且接受崩溃风险时才建议;`--mft` 必须配合 `--elevate`(读卷设备需管理员) |
| 20 | **`--elevate` 失败时降级到引导用户手动运行** | CLI 输出 `{"error":"elevation_failed",...}` 表示用户拒绝了 UAC 或系统策略禁止提权,此时不要重试 `--elevate`,改为引导用户右键终端"以管理员身份运行" |

## 红线触发的 guard 行为

| Verdict | 含义 | 执行方行为 |
|---|---|---|
| `Pass` | 路径安全(系统临时/缓存) | 直接执行 |
| `Warn` | 用户数据/含 msiexec 等需注意 | 提示用户但不阻塞 |
| `Block` | 系统关键路径/危险命令 | **绝不执行**,告知用户原因 |
| `NeedsReview` | 未识别路径 | 交给 agent 判断 |

`--allow-system` flag 是唯一逃生口:开启后 Protected 路径降级为 Warn(仍留痕),不再 Block。**不主动推荐**。
