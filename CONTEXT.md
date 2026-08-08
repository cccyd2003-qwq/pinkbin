# Pinkbin

Pinkbin is an open-source, Windows-first desktop tool for reclaiming disk space. It serves both ordinary users who want routine cleanup and advanced users who need an explanation of large application, development, game, and media data.

## Product language

**通用清理**:
Routine cleanup of clearly rebuildable system or application data and leftovers. It is the low-friction path for ordinary users, but it never means "delete everything that looks unused".

**专项清理**:
Reviewing application- or workflow-specific space such as development environments, game data, model files, and media assets. It exposes context and consequences before the user chooses what to remove.

**快速清理**:
The ordinary-user entry that surfaces low-risk cleanup recommendations and an estimated reclaimable amount with minimal configuration.

**专项包**:
An application- or workflow-specific set of recognition and cleanup rules with its own safety boundaries, explanations, and review behavior. Avoid: 插件, 脚本包.

**专项包体系**:
The extensible product boundary for adding application- or workflow-specific cleanup without changing the meaning of the universal cleanup path. Each package must be independently reviewable for recognition accuracy and safety.

**稳定包**:
An official or accepted专项包 whose recognition, safety boundaries, and cleanup behavior have passed the product's review and regression checks. It may participate in the default experience when its items meet the applicable risk rules.

**实验包**:
A visible but explicitly non-default专项包 whose behavior or coverage is still being validated. It requires deliberate user opt-in and must disclose its limited confidence.

**稳定范围**:
The reviewed cleanup scope inside a专项包 that may participate in the default experience when its own risk rules permit it. Stability is evaluated at scope level, not assumed for every file owned by an application.

**实验范围**:
The reviewed-but-not-yet-default cleanup scope inside a专项包. It is visible for discovery and testing, but requires explicit opt-in and cannot be silently added to a快速清理 selection.

**首发稳定范围**:
The initial reviewed set: Windows temporary data, browser caches, uninstall leftovers, WeChat/QQ caches, Node/Python/Conda package caches, Steam/Epic shader caches, and ordinary VS Code/JetBrains caches. User databases, local history, active environments, and other stateful data remain outside default cleanup.

**首发实验范围**:
The initial opt-in set: Docker/WSL images and volumes, stale Conda environments, AI model caches, video-editor caches, and video assets. These are visible for discovery but require separate validation and deliberate selection.

**稳定版发布门槛**:
A stable cleanup scope must pass safety tests, restore tests, and validation on real Windows machines before it can participate in the default experience. Experimental scopes may ship visibly only when they are opt-in and clearly labeled.

**可重建内容**:
Data that an application or development workflow can recreate from an existing source, package registry, or installation process. Avoid: 垃圾, 无用文件.

**用户内容**:
Personal or work-owned documents, photos, videos, downloads, project source, and other data whose value cannot be inferred safely from its path or age. User content is discoverable but is not a default cleanup recommendation.

**应用残留**:
Files or folders left behind after an application is removed or no longer owns the associated data. A path is not application residue merely because its name resembles an application name.

**清理建议**:
An explanation of what an item is, why it may be removable, what it may affect, and how recoverable it is. A recommendation is not permission to delete.

**预计可释放空间**:
The estimated space that the currently selected cleanup set would return to the drive. It is an estimate until execution completes and must be distinguishable from total scanned size.

**结果卡片**:
The first-level summary for a cleanup category or专项包, showing its estimated reclaimable space, risk label, explanation, and primary review action. It is a conclusion surface, not a raw file listing.

**风险优先结果**:
The result-page hierarchy in which action safety is the first grouping and business category is the second. It lets ordinary users act on safe findings before exploring complex paths.

**复核页**:
The explicit pre-execution view that lists selected paths, estimated reclaimable space, risk, and consequences before隔离删除 begins.

**实际释放空间**:
The measured space returned after a cleaning task completes, reported separately from预计可释放空间 and accompanied by any skipped or failed items.

**语义空间图**:
A space visualization that combines size with the product's business categories and allows the user to drill down to concrete paths. Avoid: 纯树状图, 只按扩展名分组.

**仅建议查看**:
The action boundary for user content and other high-uncertainty findings: the item is visible in analysis and may be explained or opened, but it cannot enter a default cleanup selection.

**主导航**:
The durable product areas: 首页, 空间分析, 专项包, 历史/恢复, and 设置. Scan and cleanup execution remain task flows within these areas rather than permanent top-level destinations.

**页面职责决策**:
首页采用 A 的任务卡首页，负责给普通用户一个低认知负担的快速清理入口；空间分析采用 B 的语义空间地图，负责解释磁盘占用来源并支持下钻；C 的审核工作台作为专项清理和执行前复核的高级界面，不作为首页方向。

**低风险内容**:
Content whose removal is normally reversible or whose source can reliably recreate it, such as temporary files, caches, and build artifacts. Low risk permits default selection, not silent execution.

**高风险内容**:
Content that may contain user work, private history, application state, or data whose loss cannot be reliably reversed. High-risk content requires explicit user selection and explanation.

**风险标签**:
The user-facing safety language for a cleanup item: 可直接清理, 需要确认, or 仅建议查看. It communicates the action boundary without exposing internal numeric scoring.

**隔离删除**:
The recoverable cleanup action: selected items are moved to a recoverable location and recorded so the user can review or restore them. Avoid: 永久删除.

**快速扫描**:
The default scan that checks known, bounded locations with low-risk recognition rules. Full-disk exploration is an explicit follow-up action, not a requirement for first launch.

**人工审核**:
The user-controlled step between explanation and execution where selections, risk, estimated reclaimable space, and consequences can be changed. It cannot be skipped by an automatic cleanup path.

**选择单元**:
The smallest user-facing item that can be selected for cleanup. It is normally a专项包 scope or category, with path-level candidates available when the user needs finer control.

**清理任务状态**:
The per-item execution state: 等待中, 执行中, 已完成, 已跳过, or 失败. A batch result must preserve these states instead of reducing partial success to one percentage.

**规则识别**:
Deterministic classification based on known application signatures, paths, metadata, and safety constraints. It is the default source of cleanup recommendations.

**AI 解释**:
Optional assistance for unknown locations, using sanitized metadata rather than file contents. AI may explain uncertainty, but it cannot approve or execute deletion.

**未知项**:
A scanned path whose identity, ownership, or cleanup consequence is not sufficiently supported by rules. An unknown item remains visible with uncertainty and cannot become a cleanup recommendation without人工审核.

**用户内容发现**:
Showing personal or work-owned content in space analysis so the user understands what occupies the drive, without making that content a default cleanup candidate.

**首次快速扫描**:
The first-launch path that explains its bounded scope and privacy posture, then starts快速扫描 without requiring an account, a tutorial, or manual directory selection.

**本地优先**:
The privacy posture in which scanning and rules run without an account or mandatory network connection; cloud AI is never required, and any external analysis is explicitly initiated by the user.

**本地扫描报告**:
The locally stored record of a scan's recognized items, explanations, selections, and execution outcome. It contains metadata needed for review and recovery, not file contents, and can be cleared by the user.

**历史/恢复**:
The user-facing area for revisiting local scan and cleanup records, inspecting隔离期 contents, and initiating a restore. It is part of the product's trust model, not merely a diagnostic log.

**安全自定义**:
User control over additional scan roots, excluded locations, and always-keep rules without exposing executable deletion logic or arbitrary rule editing.

**手动清理**:
The product's MVP operating mode: scanning and execution start from explicit user actions, with no resident monitor and no automatic deletion.

**专项包更新**:
An explicit, user-visible update of a reviewed专项包 independent of the main application release. The user can see its version and provenance and can roll back if needed.

**隔离期**:
The default seven-day period during which隔离删除 results remain recoverable. Expiry does not silently cause permanent deletion; disposal requires an explicit user action.

**规则包治理**:
The review boundary for专项包: each package has a version, provenance, safety tests, and documented scope; community changes enter through review, and AI cannot author executable deletion rules automatically.

**Windows-first**:
Windows 10/11 is the first supported platform and the product's initial validation surface. Cross-platform support is a later expansion, not an MVP constraint.

**按需管理员扫描**:
The permission model in which ordinary scanning works without elevation, while only capabilities that genuinely need administrator access request it at the moment they are used. A denied elevation falls back to a slower or narrower scan with an explanation.

**被占用项**:
A selected path that cannot be processed because another process or the operating system holds it open. It is reported as a recoverable failure, never force-closed, and can be retried separately.

**浏览器隐私数据**:
Passwords, cookies, history, autofill, sessions, and similar state that can identify or sign in a user. These are outside the default browser cleanup boundary; browser cleaning means cache and temporary data only.

**脱敏 AI 元数据**:
The minimal external-analysis payload: a redacted relative path plus non-content signals such as size, extension, timestamps, and known signatures. It excludes file contents, hashes, and account identifiers.

**系统保护路径**:
Operating-system locations that are never cleanup candidates, including protected Windows directories, system volume metadata, and recycle-bin internals. Warnings cannot override this boundary.

**部分成功**:
A batch outcome where completed, skipped, and failed items coexist. The product reports each state and continues independent work instead of treating the whole batch as an all-or-nothing operation.
