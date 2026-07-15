# 工作流详细流程

所有命令从 workspace root 执行。二进制路径:`skills/disk-cleanup/bin/pinkbin-cli.exe`

为简洁起见,下文示例用 `BIN` 代指 `skills/disk-cleanup/bin/pinkbin-cli.exe`。

## 1. 扫盘

用户说"扫一下 C 盘"/"看看 D 盘"时:

```
skills/disk-cleanup/bin/pinkbin-cli.exe analyze "C:\\" --top 50
```

解析返回 JSON 中的 `summary.top_dirs` 和 `summary.scaffold_matches`。向用户报告:
- 整体占用(root.size)+ 文件数(root.file_count)
- top 5 大目录(路径 + 大小)
- 已识别的可清理项(`scaffold_matches`:scaffold_id + path + size)

**扫描速度说明**:
- 大目录(C 盘 100 万+ 文件)首次扫描可能需几十秒到几分钟,告知用户这正常
- `stats.mode` 字段会显示 `"mft"`(成功使用 MFT 快速路径)或 `"walkdir"`(降级到普通遍历)
- **MFT 快速路径默认开启**:Windows 上优先尝试 NTFS $MFT 直读,大目录扫描可从分钟级降到秒级
- 失败时自动降级到 walkdir,`catch_unwind` 兜底,不影响正常输出

详见 [mft-fix.md](mft-fix.md)。

## 2. 看懂(深度分析)

用户问"xxx 文件夹是什么"/"这个能删吗"时:

```
BIN inspect "<path>" --samples 20
```

拿到元数据 JSON,你自己判断:
- 看 `top_extensions`:`.log`/`.tmp`/`.cache` → 缓存,低风险
- 看 `sample_paths`:含 `Documents`/`Pictures`/`Desktop` → 用户数据,不可删
- 看 `top_children`:目录名含 `cache`/`temp`/`logs` → 可清理子目录
- `scaffold_hint` 非 null → 有专用清理脚本,转 step 3

给出结构化建议:
```
**是什么**: xxx
**能否删除**: 可以/谨慎/不可
**风险等级**: 低/中/高
**建议**: 具体怎么清
```

## 3. 列出可用 scaffold

用户问"能清理什么"时:

```
BIN scaffolds
```

返回所有已加载的 scaffold 列表(id/name/risk/scopes)。向用户展示可清理类型。

## 4. 预览清理

用户确认要清理某 scaffold 时,**先预览**:

```
BIN preview <scaffold_id> "<path>" --scope <scope_id>
```

返回 `matched` 列表(每个匹配的文件/目录 + 大小)+ `total_bytes` + `guard_blocked` + `guard_warnings`。

- `guard_blocked`:guard 拦截的路径(执行时会拒绝)。如果非空,告知用户这些路径受保护,不可清理。
- `guard_warnings`:guard 警告的路径(用户数据等,可执行但需确认)。如果非空,**必须**特别提醒用户。

**必须**把这个预览结果给用户看,等用户确认后才执行。

## 5. 执行清理

用户确认后:

```
BIN execute <scaffold_id> "<path>" --scope <scope_id> --dry-run false
```

返回 `executed: true` + `matched_count` + `total_bytes` + `undo_log` 路径 + `guard_warnings`。
告知用户清理完成,如需恢复从回收站还原。

如果 execute 返回 `"executed": false, "reason": "guard blocked protected paths"`,说明计划中有系统保护路径被拦。**不要**尝试用 `--allow-system` 绕过,除非用户明确要求且理解风险。

## 6. 重复文件检测

用户说"找重复文件"/"清理重复"时:

```
BIN dedup "<path>" --min-size 1024 --top 50
```

返回 `groups` 数组,每组含相同 SHA-256 的文件列表 + `waste_bytes`(可回收空间)。三阶段算法(size → head-hash → full-hash)保证准确性。

向用户报告:
- 总重复组数(`total_groups`)、总重复文件数(`total_dup_files`)、可回收空间(`total_waste_bytes`)
- top 5 大重复组(路径 + 大小 + 浪费空间)

**重要**:dedup 只**检测**,不删除。删除决策必须由用户逐组确认,因为:
- 重复文件可能是用户有意保留的备份
- 系统文件(如 WinSxS 中的硬链接副本)不可删,guard 会拦截
- 建议用户确认后再手动删除,或用文件管理器人工核对

## 7. 已安装软件盘点

用户说"装了哪些软件"/"看看装了什么"时:

```
BIN inventory
```

读取 Windows 注册表三个 Uninstall 键,返回所有已装应用(`apps` 数组),每项含 name/version/publisher/install_path/estimated_size_mb/uninstall_string。

向用户报告:
- 总安装数(`total`)
- 按占用空间排序的 top 10(名称 + 大小 + 发布者)
- 标注 per-user 安装(可能是绿色软件,卸载需谨慎)

如用户要卸载某软件,**不要**直接调 `uninstall_string`——它可能弹 UAC 或 GUI。建议用户走"设置 → 应用"或软件自带卸载器。卸载后可用 `leftovers` 扫残留。

## 8. 卸载残留扫描

用户说"卸载完 xxx 还有残留吗"时:

```
BIN leftovers "<app_name>"
```

扫描 `%APPDATA%` / `%LOCALAPPDATA%` / `%PROGRAMDATA%` 下名称匹配的目录,返回 `found_paths`(路径 + 大小)+ `total_size_bytes`。

向用户报告:
- 找到的残留路径 + 各自占用空间
- 总占用

残留清理建议:
- 多数残留是配置/缓存,可安全删除
- 但**不要**主动删除,先让用户确认每个路径
- 用户确认后,可建议手动删除或用 `execute` + 合适 scaffold(若匹配)

## 9. WinSxS 组件清理(深度瘦身)

用户说"清理 WinSxS"/"组件存储太大"/"DISM 清理"时:

```
BIN preview winsxs "C:\Windows\WinSxS" --scope start-component-cleanup
```

返回 `mode: "system_cmd"` + 每个 scope 的 `command`/`requires_admin`/`target_size_bytes`(WinSxS 当前大小,只读扫描)。

两个 scope:
- `start-component-cleanup`:安全清理被取代组件(推荐)
- `start-component-cleanup-resetbase`:深度清理 + ResetBase(**无法回滚更新**,仅系统稳定 30 天以上用)

向用户报告:
- WinSxS 当前大小(`target_size_bytes`)
- 将运行的命令(`command`)
- `requires_admin: true` → **必须提醒用户以管理员身份运行 CLI**(或加 `--elevate`)

用户确认后:
```
BIN --elevate execute winsxs "C:\Windows\WinSxS" --scope start-component-cleanup --dry-run false
```

返回 `results` 数组,每项含 `executed`/`exit_code`/`stdout`/`stderr`。DISM 输出较长,截取关键行给用户(如"Component Store Cleanup: ...")。

**重要**:
- WinSxS 绝不能直接删文件(guard 会 Block),只能走 DISM
- ResetBase 不可逆,必须让用户明确确认
- 需管理员权限,非管理员运行 DISM 会失败(exit_code 非 0)

## 10. Windows.old 清理(深度瘦身)

用户说"清理 Windows.old"/"旧系统文件"/"升级后残留"时:

```
BIN preview windows-old "C:\Windows.old"
```

返回 `mode: "system_cmd"` + `command`(takeown + icacls + rd 三步)+ `requires_admin: true`。

向用户报告:
- Windows.old 当前大小(若存在)
- 将运行的命令(接管所有权 → 授权 → 删除)
- `requires_admin: true` → **必须提醒用户以管理员身份运行**(或加 `--elevate`)
- **删除后无法回滚到旧版 Windows**

用户确认后:
```
BIN --elevate execute windows-old "C:\Windows.old" --dry-run false
```

如果执行失败(rd 权限不足),建议用户用 Windows 自带「磁盘清理」工具(`cleanmgr`)→ 「清理系统文件」→ 勾选「以前的 Windows 安装」。

## 11. 休眠文件管控(系统优化)

用户说"关闭休眠"/"hibernate"/"休眠文件太大"时:

查询状态(只读):
```
BIN hibernate status
```
返回 `enabled`/`hiberfil_size_bytes`/`powercfg_a_output`。

关闭休眠(释放 hiberfil.sys,通常几 GB):
```
BIN --elevate hibernate off
```
**需管理员权限**。关闭后无法使用休眠功能(快速启动仍可用,因为它依赖 hiberfil.sys,关闭后快速启动也会失效)。

设置 hiberfil.sys 大小(50-100% 物理内存):
```
BIN --elevate hibernate size 75
```
**需管理员权限**。减小 hiberfil.sys 大小但不完全关闭。

向用户报告:
- 当前 hiberfil.sys 大小(若存在)
- 关闭后可释放的空间
- 关闭休眠的影响(无法休眠、快速启动失效)

## 12. 虚拟内存迁移(系统优化)

用户说"迁移页面文件"/"pagefile 迁到 D 盘"/"虚拟内存迁移"时:

查询当前配置(只读):
```
BIN pagefile status
```
返回 `pagefiles` 数组(每个含 name/drive/allocated_base_size_mb)+ `auto_managed`。

迁移到目标盘:
```
BIN --elevate pagefile migrate D
```
**需管理员权限,需重启生效**。步骤:关闭自动管理 → 删除 C 盘 pagefile → 在 D 盘创建新 pagefile。

向用户报告:
- 当前 pagefile 配置(盘符 + 大小)
- 迁移后 C 盘可释放的空间
- **需重启才能生效**
- 建议目标盘有足够空间(至少物理内存大小)

## 13. 系统还原点清理(系统优化)

用户说"清理还原点"/"卷影副本太多"/"restore point"时:

查询状态(需管理员完整查看):
```
BIN --elevate restore status
```
返回 `shadow_count`/`used_space_bytes`/`allocated_space_bytes`。

删除所有还原点:
```
BIN --elevate restore delete-all
```
**需管理员权限,不可逆**。`vssadmin delete shadows /all /quiet`。

向用户报告:
- 当前还原点数量 + 占用空间
- 删除后**无法通过系统还原回滚**
- 建议保留至少 1 个最近还原点(但此命令删除全部,用户若需保留应用 Windows 系统保护界面手动管理)

## 14. 可迁移软件分析(系统优化)

用户说"哪些软件可以迁到 D 盘"/"C 盘软件太大"时:

```
BIN migrate
```

扫描 `C:\Program Files`、`C:\Program Files (x86)`、`%LOCALAPPDATA%\Programs`,返回 `candidates` 数组(每个软件 name/install_path/size_bytes,按大小降序)+ `c_program_files_total_bytes`。

向用户报告:
- C 盘 Program Files 总占用
- top 10 大软件(名称 + 大小)
- 建议迁移哪些(大小 > 1GB 的优先)

**注意**:此命令只**分析**,不执行迁移。软件迁移风险高(涉及注册表、快捷方式、依赖路径),建议用户:
- 优先用软件自带的"更改安装位置"功能
- 或卸载后重装到目标盘
- 不要用 robocopy + 改注册表的方式(容易破坏软件)

## 15. 软件卸载(深度瘦身)

用户说"卸载 xxx 软件"/"帮我卸载"时:

**第一步:确认目标软件**(先查 inventory 确认名称)
```
BIN inventory | grep -i "xxx"
```

**第二步:预览卸载命令**(默认 dry-run)
```
BIN uninstall "精确软件名" --dry-run true
```

返回 `matched` 数组(匹配的软件)+ `guard_verdict`:
- `pass`:卸载串安全,可直接执行
- `warn`:卸载串含 msiexec/powershell 等,正常但需确认来源可信
- `block`:卸载串含危险命令(format/diskpart 等),**绝对不可执行**
- `skip`:无匹配或多匹配,不执行

如果 `matched` 有多项,告知用户名称不唯一,需提供更精确的名称。

**第三步:执行卸载**(用户确认后)
```
BIN uninstall "精确软件名" --dry-run false
```

或静默卸载(对 MSI/NSIS 追加 /quiet 或 /S):
```
BIN uninstall "精确软件名" --silent --dry-run false
```

返回 `execution` 含 `command_run`/`executed`/`exit_code`/`stdout`/`stderr`。GUI 卸载器会弹窗,需用户在 GUI 中完成;静默卸载则无界面。

**第四步:扫残留**(卸载完成后)
```
BIN leftovers "软件名"
```

向用户报告残留路径,用户确认后建议手动删除。

**重要**:
- guard `block` 时**绝不**执行,告知用户该卸载串含危险命令
- 多匹配时**绝不**猜测,让用户提供精确名称
- 卸载后务必跑 `leftovers` 扫残留

## 16. 软件迁移执行(系统优化)

用户确认要迁移某软件(从 `migrate` 分析结果中选定)时:

**预览迁移步骤**(默认 dry-run)
```
BIN migrate-app "C:\Program Files\App" D --dry-run true
```

返回 4 个步骤的预览:
1. `robocopy`:复制目录到 `D:\App`
2. `update_registry`:修改注册表 InstallLocation 指向新路径
3. `update_shortcuts`:更新开始菜单快捷方式
4. `keep_source`:保留原目录(验证新路径可用后手动删)

返回 `guard_verdict`:
- `pass`:源路径非系统保护路径,可迁移
- `block`:源路径在保护列表(System32/Windows 等),**绝不**迁移
- `warn`/`needs_review`:需用户确认

**执行迁移**(用户确认后)
```
BIN migrate-app "C:\Program Files\App" D --dry-run false
```

每步返回 `executed`/`exit_code`/`stdout`/`stderr`。

**迁移后引导**:
- 告知用户先测试新路径的软件是否正常启动
- 验证可用后,可手动删除原目录(或用 `execute` + 合适 scaffold)
- 如新路径软件异常,undo log 记录了迁移操作,可手动回滚(删目标目录 + 恢复注册表)

**重要**:
- guard `block` 时**绝不**迁移
- 迁移不删原目录(避免失败导致软件损坏)
- 部分软件内部硬编码了安装路径,迁移后可能无法启动 —— 这是已知风险,务必先 dry-run 让用户知情

## 提权机制(`--elevate`,全局 flag)

许多操作需要管理员权限:`hibernate off` / `pagefile migrate` / `restore delete-all` / `execute winsxs` / `execute windows-old`。两种用法:

1. **推荐**:让 agent 自动调 `--elevate`,CLI 会弹 UAC,用户同意后自动以管理员身份重启自己并执行:
   ```
   BIN --elevate hibernate off
   ```
   - 用户拒绝 UAC 时,CLI 输出 `{"error":"elevation_failed",...}` JSON,告知用户手动以管理员身份运行
   - 提权后子进程 stdout 重定向到临时文件,父进程等待并透传,agent 解析 JSON 不受影响

2. **备选**:让用户手动以管理员身份打开终端运行命令(适合用户想看实时输出):
   - 引导用户右键 cmd/PowerShell → "以管理员身份运行"
   - 然后执行不带 `--elevate` 的命令
