# Pinkbin 清理产品规格

## Problem Statement

Windows 用户知道磁盘快满了，却通常不知道空间被什么占用、哪些内容可以安全清理、清理后会不会影响应用或个人数据。

现有工具各自只解决一段链路：传统清理器把内容压成“垃圾”，磁盘分析器只展示目录大小，卸载器只关注已安装应用。用户仍然要自己完成“看懂路径 → 判断归属 → 估计后果 → 决定是否删除”这段最危险、最费认知的工作。

Pinkbin 当前已有磁盘扫描、应用专项包、规则化 scope、回收/隔离执行和 AI 解释基础，但产品入口仍偏向开发者工具和目录浏览，缺少一个普通用户也能直接完成的清理工作流。

## Solution

Pinkbin 是一款开源、Windows-first 的可解释磁盘清理工具。它同时提供两个入口：

- **快速清理**：扫描所有本地磁盘上的已知、边界明确的低风险位置，直接给出按风险优先组织的结果卡片。
- **深度清理**：按应用或工作流深入查看开发环境、游戏、模型、通信软件和媒体相关内容，允许下钻到具体路径。内部规则由专项包提供。

产品的核心承诺是“释放空间前先看懂空间”，而不是“让电脑变快”或“自动替你决定删除什么”。每项清理建议都必须说明对象是什么、为什么可能可清理、会影响什么、是否可重建、是否可恢复以及当前风险标签。

标准用户流程如下：

```text
首次快速扫描
  → 风险优先结果卡片
  → 按类别/深度清理查看详情
  → 语义空间图或路径列表下钻
  → 复核页调整选择
  → 隔离删除
  → 实际释放空间、部分成功状态和恢复入口
```

## User Stories

### 首次使用与快速清理

1. As a 普通 Windows 用户, I want to 在首次启动时直接开始快速扫描, so that 我不用先理解所有功能就能看到产品价值。
2. As a 用户, I want to 在扫描开始前看到扫描范围和隐私说明, so that 我知道 Pinkbin 会查看哪些目录以及哪些数据不会离开本机。
3. As a 用户, I want to 扫描所有本地磁盘上的已知安全目录, so that 我不必为每个磁盘重复配置路径。
4. As a 用户, I want to 在快速扫描期间看到文件数量、扫描路径和可理解的进度, so that 我能区分“还在扫描”和“卡住了”。
5. As a 用户, I want to 在扫描失败或权限不足时看到明确原因, so that 我知道是权限、磁盘状态还是被占用文件导致结果不完整。
6. As a 用户, I want to 在普通权限下完成基础扫描, so that Pinkbin 不会在启动时索要不必要的管理员权限。
7. As a 高级用户, I want to 在需要时主动开启完整磁盘扫描, so that 我可以查找未知的大目录而不强迫所有用户承担全盘扫描成本。
8. As a 用户, I want to 看到扫描总量、已识别内容量和预计可释放空间是三个不同的数字, so that 我不会把“硬盘占用”误认为“可清理空间”。

### 结果理解与空间分析

9. As a 用户, I want to 先看到“可直接清理 / 需要确认 / 仅建议查看”三组结果, so that 我可以先处理低风险内容。
10. As a 用户, I want to 每个结果卡片显示预计可释放空间, so that 我能按收益决定先处理什么。
11. As a 用户, I want to 每个结果卡片显示对象是什么、为什么被识别出来以及清理后会怎样, so that 我不需要凭文件夹名猜测。
12. As a 用户, I want to 看到结果属于系统、应用、开发、游戏、模型或媒体哪一类, so that 我可以按自己的工作方式理解空间结构。
13. As a 用户, I want to 从结果卡片下钻到具体路径, so that 我能验证 Pinkbin 的判断是否符合我的电脑实际情况。
14. As a 用户, I want to 查看路径、大小、文件数和关键扩展名摘要, so that 我能在不打开文件内容的情况下判断它的性质。
15. As a 用户, I want to 使用语义空间图查看大目录, so that 目录面积可以帮助我快速定位空间大户。
16. As a 用户, I want to 点击语义空间图中的区域进入对应路径, so that 图形和具体清理建议保持联动。
17. As a 用户, I want to 查看用户内容但只看到“仅建议查看”标签, so that 我知道视频、照片、文档占用了空间，同时不会被误导去删除它们。
18. As a 用户, I want to 在未知项上看到“不确定”而不是伪造的确定分类, so that 我可以正确评估工具的边界。
19. As a 用户, I want to 对未知项执行“打开资源管理器 / 询问 AI / 始终保留”, so that 我可以自己完成判断并校准工具。
20. As a 用户, I want to 看到扫描覆盖范围和未扫描原因, so that 我知道报告是否因为权限或路径限制而不完整。

### 快速清理选择与复核

21. As a 普通用户, I want to 低风险、可重建内容默认被选择, so that 常见清理不需要逐个勾选。
22. As a 用户, I want to 中风险内容必须经过主动选择, so that “建议清理”不会被误解为“已经授权删除”。
23. As a 用户, I want to 高风险和用户内容只能查看不能进入默认清理集合, so that Pinkbin 的默认行为保持保守。
24. As a 用户, I want to 按深度清理范围、scope 或类别调整选择, so that 我可以一次清理一个明确的范围。
25. As a 用户, I want to 从深度清理范围继续下钻到单个路径, so that 我可以保留某个位置而清理同类的其他位置。
26. As a 用户, I want to 在复核页看到所有即将处理的路径和总大小, so that 最终确认具备可审查性。
27. As a 用户, I want to 在复核页看到每项风险、原因、可重建性和可能影响, so that 我能做有根据的取舍。
28. As a 用户, I want to 一键取消某个结果卡片或单个路径, so that Pinkbin 不会强迫我接受整套建议。
29. As a 用户, I want to 在执行前看到“预计可释放空间”随选择变化, so that 我知道取消某项会牺牲多少收益。
30. As a 用户, I want to 看到默认选择规则的说明, so that 我知道为什么某些项自动勾选而另一些没有。

### 清理执行、失败与恢复

31. As a 用户, I want to 清理默认走隔离删除或系统回收站, so that 误操作有恢复机会。
32. As a 用户, I want to 看到清理任务按项目显示等待中、执行中、已完成、已跳过和失败, so that 部分成功不会被压成一个模糊的百分比。
33. As a 用户, I want to 某个文件被占用时任务跳过它并告诉我原因, so that Pinkbin 不会擅自关闭应用或终止进程。
34. As a 用户, I want to 在其他项目继续执行时单独重试失败项, so that 一个锁定文件不会阻塞整个清理批次。
35. As a 用户, I want to 在清理完成后看到实际释放空间, so that 我能和执行前的估算进行比较。
36. As a 用户, I want to 看到成功、跳过和失败的项目明细, so that 我能知道报告为什么没有达到预期数字。
37. As a 用户, I want to 在历史/恢复区域回看清理记录, so that 我能追溯一次操作的范围、时间和结果。
38. As a 用户, I want to 在隔离期内一键恢复被清理的项目, so that 我可以处理误删或应用异常。
39. As a 用户, I want to 隔离期结束后仍由我明确触发永久处置, so that Pinkbin 不会在后台自动制造不可逆结果。
40. As a 用户, I want to 清理失败时保留可诊断的错误信息, so that 我可以重试或提交可复现的问题。

### 专项包与应用覆盖

41. As a 用户, I want to 看到所有内置专项包, so that 我知道 Pinkbin 能处理哪些应用和工作流。
42. As a 用户, I want to 稳定范围参与默认体验, so that 已验证的缓存清理能提供即时价值。
43. As a 用户, I want to 实验范围明确标记并需要主动开启, so that 新能力不会悄悄改变默认结果。
44. As a 用户, I want to 一个应用的不同 scope 有独立的风险标签, so that 缓存、索引、账号状态和用户内容不会被混为一谈。
45. As a 用户, I want to 看到专项包的版本、来源、更新时间和安全说明, so that 我能判断规则是否可信。
46. As a 用户, I want to 主动检查专项包更新, so that 应用目录变化可以在不等待完整应用发布的情况下得到修复。
47. As a 用户, I want to 在更新后回滚专项包, so that 新规则出现误判时有退路。
48. As a 开源贡献者, I want to 提交带正向命中和红线断言的专项包, so that 新应用支持不会以误删风险换覆盖率。
49. As a 开源维护者, I want to 对专项包做版本化审核和回归验证, so that 社区贡献可以被安全地纳入产品。
50. As a 用户, I want to 快速清理覆盖已验证的系统、浏览器、应用残留、通信软件、开发缓存、游戏缓存和 IDE 缓存, so that 首次使用就能覆盖最常见的空间来源。
51. As a 用户, I want to 在实验范围中探索 Docker/WSL、旧 Conda 环境、AI 模型和视频素材, so that 产品最终可以覆盖大体积但复杂的场景。

### 隐私、AI 与自定义

52. As a 用户, I want to 不登录账号也能完成扫描和清理, so that 清理本地磁盘不依赖远程服务。
53. As a 用户, I want to 扫描报告默认只保存在本地, so that 路径元数据不会被上传或同步。
54. As a 用户, I want to 能主动清除本地扫描报告和历史记录, so that 我可以控制本机留下的痕迹。
55. As a 用户, I want to 默认使用本地规则和本地扫描, so that 清理判断不依赖云端可用性。
56. As a 用户, I want to AI 只解释未知项而不是替我选择或执行删除, so that AI 的不确定性不会直接变成破坏性动作。
57. As a 用户, I want to 云端 AI 请求只携带脱敏 AI 元数据, so that 文件内容、哈希、用户名和账号信息不会外发。
58. As a 用户, I want to 在发送云端 AI 请求前看到要发送的摘要, so that 外部分析是明确的用户行为。
59. As a 高级用户, I want to 添加自定义扫描根目录, so that 非标准安装位置也能被纳入分析。
60. As a 高级用户, I want to 配置排除目录和始终保留规则, so that Pinkbin 可以适应我的工作流而不要求我修改内置规则。
61. As a 用户, I want to 不能在 UI 中直接编辑任意删除 Glob, so that 高级控制不会绕过专项包治理。

### 质量与验收

62. As a 用户, I want to 稳定范围的默认建议在真实 Windows 机器上不会命中系统保护路径或浏览器隐私数据, so that 我可以逐步建立信任。
63. As a 用户, I want to 在一台真实电脑上识别至少 5 类空间内容, so that Pinkbin 的结果不只是单一缓存清理。
64. As a 用户, I want to 稳定范围的分类准确率目标超过 80%, so that 语义分类确实优于只看目录树。
65. As a 用户, I want to 在硬盘确实存在可清理内容时于 30 分钟内释放至少 50GB, so that 清理结果有可感知的收益。
66. As a 维护者, I want to 每个稳定 scope 都有正向匹配和红线测试, so that 规则扩展可以持续回归。
67. As a 维护者, I want to 清理执行支持部分成功、失败重试和恢复验证, so that 文件系统边界情况不会破坏整个任务。

## Implementation Decisions

- Keep the existing scanner, scaffold/rule-pack, advisor, executor, and local report concepts as the main seams. Add the product surface as a read model over scan facts and cleanup recommendations rather than making the UI re-scan or re-classify the tree.
- Keep the cleaner UI behind one read-model seam: `api.scan` provides evidence, `buildScanContext` and `buildLocalCandidates` apply the existing local rules, and `buildCleanerReadModel` adapts the result for A/B/C. The UI must not reclassify paths or promote an unreviewed scope to default selection.
- Model the product around `ScanContext`, `专项包`, `scope`, `CleanupCandidate`, `CleanupPlan`, `清理任务状态`, and `本地扫描报告`. The scan result is evidence; the recommendation is derived; the plan is explicitly user-owned after人工审核.
- Use two task entry points: 快速清理 for known bounded roots and low-risk defaults, and 深度清理 for category/app exploration and experimental ranges. 深度清理 is the UI name;专项包 remains the internal rule-pack boundary.
- Keep the main navigation as 首页、空间分析、深度清理、历史/恢复、设置. Scan, review, and execution are task states inside those areas.
- Assign the three prototype directions to concrete product roles: A is the default 首页 because its task cards and risk-first summary minimize decision cost; B is 空间分析 because its semantic map makes large-space ownership and drill-down legible; C is the advanced 深度清理 / execution-review workbench, not a competing home layout.
- Present results in a risk-first hierarchy, with category and pack grouping below it. Use result cards for conclusions, path lists for audit, and a语义空间图 for full-disk exploration.
- Keep user content discoverable but `仅建议查看`; never let it enter the default cleanup selection.
- Default low-risk rebuildable content to selected, require explicit selection for medium risk, and keep high-risk content view-only unless a future reviewed flow says otherwise.
- Use recoverable隔离删除 with a seven-day隔离期 and explicit user disposal. The execution layer must preserve per-item state and continue independent work after a被占用项 or permission failure.
- Request administrator access only for capabilities that need it, such as an optional full-disk/MFT path. A denied request must leave a usable narrower or slower scan.
- Keep system protected paths and browser privacy data out of cleanup candidates at the rule-generation boundary; UI warnings are not the only protection.
- Run deterministic rules locally by default. Optional AI is explanation-only, uses脱敏 AI 元数据, and cannot author executable deletion rules or approve execution.
- Treat专项包治理 as a first-class boundary: packages are versioned, attributed, testable, reviewable, and manually updateable. Stability is assigned per cleanup scope, not blindly per application.
- The initial stable/experimental split is the one captured in `CONTEXT.md`; experimental scopes remain visible but opt-in.
- The UI prototype is read-only and uses in-memory demo evidence. It is not allowed to call destructive commands; it exists to choose the result-page and navigation shape before production implementation.

## Testing Decisions

- Tests assert observable safety and user outcomes, not internal implementation details. A good test answers what candidates are produced, what is selected by default, what is sent externally, and what the user can restore or retry.
- Rule-pack tests must cover at least one positive match for every scope and a red-line set that produces zero matches for system paths, browser privacy data, account state, chat databases, and user-owned content.
- Recommendation tests must verify the mapping from evidence to risk label, default selection, explanation, and suggested handling. Unknown items must remain visible as uncertain and must not enter the default plan.
- Scan tests must verify bounded quick-scan coverage, optional full scan behavior, permission fallback, protected-path exclusion, and accurate separation of total size from预计可释放空间.
- Plan/executor tests must verify the two-step review boundary, default recoverable mode, per-item states, partial success, locked-file reporting, retry, restore, and explicit disposal after the隔离期.
- Privacy tests must verify no file contents or hashes are sent to the advisor, path redaction for optional cloud analysis, local-only report behavior, and explicit user consent for external analysis.
- Frontend tests should exercise user-visible flows using the same seams as the existing scaffold, modal, progress, and error-boundary tests when those exist. The prototype itself remains throwaway and is validated by visual/manual QA rather than treated as production test coverage.

## Out of Scope

- Registry cleaning, startup optimization, service modification, “speed boost” claims, antivirus/security claims, or automatic process termination.
- Automatic deletion, background cleanup, resident monitoring, or silent permanent deletion.
- Passwords, cookies, browser history, autofill, login sessions, chat databases, account state, application configuration, private documents, photos, videos, project source, and saved game data as default cleanup candidates.
- Data recovery beyond Pinkbin's own recoverable隔离删除 and restore record.
- Cloud accounts, mandatory cloud AI, mandatory telemetry, or server-side storage of scan reports.
- Automatic AI-generated deletion rules or arbitrary Glob editing in the UI.
- macOS/Linux support in the first release.
- Treating duplicate-file detection or a pure treemap as the product's core value.

## Further Notes

- The product should be judged by safe and accurate recommendations, not by the number of cleanup suggestions. A smaller trustworthy result beats a larger noisy one.
- The broad “全部专项包” goal is intentionally constrained by stable/experimental scope labels and rule-pack governance. The first implementation should prove the shared workflow with representative stable and experimental scopes before expanding coverage indefinitely.
- The prototype direction is decided: compose A (首页), B (空间分析), and C (深度清理 / 执行前复核) into one coherent navigation model. Keep the evidence and interactions read-only until the production seams for scanning, planning, isolation, and restore are connected.
