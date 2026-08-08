# Pinkbin · 给 Claude 的工作约定

> 这份文件是仓库级硬约束，每次 Claude Code 会话会自动读。**改 scaffold / 加 scaffold 时 Claude 必须按本文件 + `.claude/commands/add-scaffold.md` 走流程**。

## 项目背景

Pinkbin 是一个磁盘清理桌面 app（Tauri 2 + React + Rust）。核心是一组 **scaffold TOML**（`scaffolds/<id>.toml`）——每份描述一个常用软件的清理目标（缓存、日志、可选的历史媒体）和它的红线。Studio 面板按 scaffold 渲染卡片，用户可以按 scope 单独清理。

## Hard rules（不可违反）

1. **Scope glob 红线**：任何 `[[scope]]` 的 glob **不允许**命中以下路径片段：
   - `*.db`、`*.db-wal`、`*.db-shm`（聊天/账号 DB）
   - `**/db_storage/**`（WeChat 4.x DB 群）
   - `**/Msg/**`、`**/MultiMsg/**`（WeChat 3.x 聊天数据）
   - `**/Accounts/**`、`**/All Users/**`、`**/login/**`、`**/config/**`（账号状态）
   - `**/Favorite*/**`、`**/Fav/**`（用户收藏）
   - `**/key/**`、`**/crypto/**`（加密物料）
   - 各家 IM 的"用户原始文件夹"（详见 `docs/scaffold-requirements/<category>.md` 的红线节）

2. **Safety test 强制**：每份 scaffold 必须在 `crates/scaffold/tests/<id>_safety.rs` 落一个集成测试，包含**正向断言**（每个 scope 至少一条命中路径）+ **红线断言**（一组红线路径必须 zero match）。模板见 `crates/scaffold/tests/_templates/scaffold_safety.rs`。

3. **UI 防御三件套**：任何调用破坏性 Tauri 命令（`execute_plan` / `execute_scope`）的前端组件必须满足：
   - 被 `<ErrorBoundary>` 包裹（参考 `apps/desktop/src/components/ErrorBoundary.tsx`）
   - 有**两步确认**或 dry-run 预览，**不允许直接 `window.confirm`**（Tauri webview 行为不稳定）
   - 默认 `mode = "recycle"`（走系统回收站可恢复），`delete` 模式只在用户显式选择时使用

4. **隐私**：枚举用户数据目录时**只用 Glob/`ls` 列文件夹名**，**绝不 `Read`** 聊天 DB / 媒体文件 / 账号 key 等用户内容。

## Discovery loop（动手前的纪律）

写新 scaffold 或大改现有 scaffold 之前，**先读这三个文件，理解当前状态**：

1. `crates/scaffold/src/lib.rs` —— TOML schema、env 展开、detect 逻辑
2. `apps/desktop/src/components/Studio.tsx` —— 当前 UI 怎么渲染 scope（看 expanded card 那一段）
3. `apps/desktop/src/types.ts` 的 `Scaffold` / `Scope` 类型 —— 前后端 schema 镜像

**漏掉这步的代价**：你会按"想象中的 UI"设计 TOML，结果 UI 根本不渲染相关字段，回头返工。

## 实测回环（Phase 7 经验）

每次"真实路径勘测"完成后，**必须回头修订需求文档**——勘测往往会推翻 Phase 1-2 的假设。例如 WeChat 4.x 的 `msg/` 目录在 3.x 是聊天 DB（红线），在 4.x 是用户接收的文件（L2 可清）。这种发现要写成需求文档的"实测附录"节，**在写 TOML 之前**完成。

## Schema 一致性

Rust 的 `Scaffold` 结构体用 `serde` 做了 TOML ↔ JSON 双向序列化：
- TOML 输入用 `[[scope]]`（单数），通过 `#[serde(rename(deserialize = "scope"))]` 接收
- JSON 输出给前端用 `scopes`（复数），通过 `#[serde(rename(serialize = "scopes"))]` 暴露

**任何对 `Scaffold` / `Scope` / `Mode` / `Prompt` 的字段调整**：
- 同步更新 `apps/desktop/src/types.ts` 的镜像类型
- 验证：`cargo check -p pinkbin-desktop` + `pnpm -C apps/desktop exec tsc --noEmit` 都干净

## 命令速查

```bash
# Scaffold lint（必须 0 error）
cargo run -p pinkbin-scaffold-lint -- scaffolds/<id>.toml
# 全部 safety test
cargo test -p pinkbin-scaffold
# 桌面端 dev
pnpm tauri dev
# 类型检查
pnpm -C apps/desktop exec tsc --noEmit
# 前端构建
pnpm -C apps/desktop exec vite build
```

## Agent skills

### Issue tracker

Issues live in GitHub Issues for `guoyiwen/pinkbin`, managed with `gh`. See `docs/agents/issue-tracker.md`.

### Triage labels

Use the default canonical labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout: root `CONTEXT.md` and `docs/adr/`. See `docs/agents/domain.md`.

## 风格

- **写文档/注释**：中文 prose + 英文代码、路径、术语（`scope` / `glob` / `dry-run` / `recycle` 等不翻译）。例：`scope 命中 *.db 视为红线`。
- **commit message**：title 简短描述（中文或英文皆可，单条 commit 内统一）；body 用同款中英风格，引用具体 file path 和 commit-hash。
- **不要主动加注释**说明"做了什么"——代码自解释；只在解释**为什么**（不变量、绕过的 bug）时加注释。

## 工作流入口

- 新增 scaffold：在 Claude Code 里输入 `/add-scaffold <app-id>`
- review 现有 scaffold：`/scaffold-review <app-id>`
- 完整 14-phase 工作流见 `.claude/commands/add-scaffold.md`
