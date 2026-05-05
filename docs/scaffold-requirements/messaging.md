# 即时通讯（Messaging / IM）类清理需求

> 本文件是后端编写 `scaffolds/<id>.toml` 时的需求清单。每条需求都已映射到 TOML 字段建议，写 scaffold 时请逐条对齐。

## 1. 范围

| 优先级 | 应用 | 状态 |
|------|------|------|
| P0   | WeChat (PC) | 本轮重写 [scaffolds/wechat-pc.toml](../../scaffolds/wechat-pc.toml) |
| P0   | QQ | 已有 [scaffolds/qq-pc.toml](../../scaffolds/qq-pc.toml)，待用本文档 review |
| P0   | DingTalk | 已有 [scaffolds/dingtalk.toml](../../scaffolds/dingtalk.toml)，待用本文档 review |
| P1   | Feishu / Slack / Discord / Telegram / Teams | 已有 scaffold，待用本文档 review |

## 2. 数据三级分级 + 默认行为

### L1 可重生缓存（点完即可由应用自动重建）

| 子类 | 描述 | 默认勾选 | 保留期 | 备注 |
|-----|-----|---------|-------|-----|
| 图片缓存 | 聊天图片预览 / 缩略图 | ✅ | 30 天 | 删后看老聊天会"图片已过期" |
| 视频缓存 | 视频文件本体 | ✅ | 7 天 | 体积大、过期重下成本高 |
| 表情/贴图 | 自动下载的表情包（非用户收藏的自定义） | ✅ | 30 天 | 系统包/官方包重启自动恢复 |
| Web/Cef 缓存 | 内置浏览器/小程序运行时缓存 | ✅ | 全量清（`prompt = none`） | 不影响功能 |
| 小程序包 (Applet) | 小程序下载的代码包 | ✅ | 全量清 | 下次打开自动重下 |
| 运行日志 (Logs) | App 自身日志 | ✅ | 全量清 | 仅工程师排障用 |
| Crash dumps | 崩溃转储文件 | ✅ | 全量清 | 上报后无用 |

### L2 可选历史数据（用户原始内容，删除不可恢复）

| 子类 | 描述 | 默认勾选 | 保留期 | 备注 |
|-----|-----|---------|-------|-----|
| 接收的文件 | 别人发给你的文档/压缩包 | ✅ | 30 天 | 在聊天里仍可"重新下载"按钮触发 |
| 语音消息 | amr/silk 语音文件 | ✅ | 30 天 | 删后老聊天显示"语音不存在" |
| Backup 备份文件夹 | 聊天迁移/备份产物 | ❌ | `prompt = confirm` 显式确认 | 默认不动；用户主动勾选并打勾 confirm 才清 |

### L3 红线（任何 scope glob 都不允许命中）

- **聊天 DB**：`**/*.db`、`**/*.db-wal`、`**/*.db-shm`
- **聊天目录**：`**/Msg/**`、`**/MSG_*/**`、`**/MultiMsg/**`
- **账号状态/设置**：`**/Accounts/**`、`**/All Users/**`、`**/config/**`
- **加密相关**：`**/key/**`、`**/crypto/**`、`**/*.dat`（疑似 token）
- **用户收藏**（软红线，建议同样豁免）：`**/Favorite*/**`、`**/Fav/**`、`**/FileStorage/Fav/**`

## 3. 通用 prompt 形态

| Bucket 类型 | `prompt.kind` | default | UI label |
|-----|-----|------|------|
| L1 有保留期的（图/视频/表情） | `days` | 30 / 7 / 30 | "Delete X older than (days)" |
| L1 全量的（Cef/Applet/Logs/Dumps） | `none` | – | – |
| L2 接收文件/语音 | `days` | 30 | "Delete X older than (days)" |
| L2 Backup | `confirm` | `false` | "I understand this deletes my chat backups" |

## 4. 用户偏好

- **多账号**：一台机登过多个账号时，**所有账号一起清**。glob 用 `**/wxid_*/<bucket>/**` 形态（QQ 用 `**/<qq号>/<bucket>/**`），不为多账号选择做额外 UI。
- **mode**：所有 scope 默认 `mode = "recycle"`，走系统回收站，给用户后悔机会。
- **risk 等级**：整个 scaffold `risk = "low"`，因为我们承诺不动 L3。

## 5. 给后端的 TOML 设计提示（WeChat 示例）

下表是 WeChat 一份理想 TOML 的 scope 蓝图。其他 IM 应用按相同分级套用，目录名替换为各家约定。

| scope id | label | glob 骨架 | mode | prompt |
|----------|-------|-----------|------|--------|
| `image-cache` | Image cache | `**/wxid_*/FileStorage/Image/**` | recycle | `{ kind = "days", default = 30 }` |
| `video-cache` | Video cache | `**/wxid_*/FileStorage/Video/**` | recycle | `{ kind = "days", default = 7 }` |
| `sticker-cache` | Sticker / 表情包缓存 | `**/wxid_*/FileStorage/CustomEmotion/**`, `**/wxid_*/Stickers/**` | recycle | `{ kind = "days", default = 30 }` |
| `web-cache` | Web/Cef 缓存 | `**/wxid_*/CefCache/**`, `**/WMPF*/**` | recycle | `{ kind = "none" }` |
| `applet-cache` | 小程序包 | `**/wxid_*/Applet/**`, `**/wxid_*/MiniProgram/**` | recycle | `{ kind = "none" }` |
| `logs` | 运行日志 | `**/wxid_*/Logs/**`, `**/diag/**` | recycle | `{ kind = "none" }` |
| `crash-dumps` | Crash dumps | `**/CrashReport*/**`, `**/Dumps/**` | recycle | `{ kind = "none" }` |
| `file-received` | 接收的文件 | `**/wxid_*/FileStorage/File/**` | recycle | `{ kind = "days", default = 30 }` |
| `voice-cache` | 语音消息 | `**/wxid_*/FileStorage/Voice2/**`, `**/wxid_*/FileStorage/MsgAttach/**` | recycle | `{ kind = "days", default = 30 }` |
| `backup` | 聊天备份 | `**/wxid_*/Backup/**` | recycle | `{ kind = "confirm", default = false }` |

> ⚠️ 上表 glob 骨架是**初稿**，最终落到 TOML 里的 glob 必须由后端基于真实磁盘上的目录树验证后裁剪：实际目录可能是 `Cache/CefCache` 而不是 `CefCache`、`MsgAttach` 可能不存在等。Phase B（路径勘测）会修正。

## 6. Disclaimer 文案要点（写进 TOML `disclaimer` 字段）

- 明确"绝不删聊天记录、账号配置、收藏"
- 明确"删除后老聊天里的图片/视频/语音可能显示为不存在"
- 明确"全部走系统回收站，30 天内可还原"
- 不要使用"安全"二字，避免造成误导

## 7. WeChat 实测路径映射（Phase B 勘测结论）

Phase B 在一台 Windows 机器上对 WeChat 4.x（`Weixin.exe`）实际目录做了枚举，结论：**3.x 和 4.x 的目录布局完全不同**，Section 5 的蓝图表只适用 3.x，4.x 必须按下表重写。

### 7.1 数据根

| 版本 | 默认数据根 |
|----|----------|
| 3.x | `%USERPROFILE%/Documents/WeChat Files` |
| 4.x | `%USERPROFILE%/Documents/xwechat_files` |
| 自定义盘 | 任意盘下同名目录（`**/xwechat_files`、`**/WeChat Files` 都需 detect） |

### 7.2 4.x 目录树（实测）

```
xwechat_files/
├── Backup/<wxid>/                       L2 Backup（默认 off）
├── all_users/
│   ├── config/                          L3 红线
│   ├── head_imgs/                       L1 头像缓存
│   ├── login/                           L3 红线
│   └── sqlite/                          L3 红线（共享 DB）
└── <wxid_xxx_xxx>/
    ├── apm_record/process_duration/     L1 APM 遥测
    ├── business/
    │   ├── emoticon/                    L1 表情包下载
    │   ├── favorite/                    L3 红线（用户收藏）
    │   ├── migrate/                     keep
    │   ├── sns/                         keep（朋友圈数据）
    │   ├── xeditor/                     keep（编辑器状态）
    │   └── xweb/mmkv/                   L3 红线（K-V 状态，可能含 token）
    ├── cache/<YYYY-MM>/
    │   ├── Emoticon/                    L1 表情缓存
    │   ├── HttpResource/                L1 Http 资源
    │   ├── Message/<chat-hash>/         L1 聊天图/语音/媒体缓存
    │   └── WeAppIcon/<2-hex>/           L1 小程序图标
    ├── config/                          L3 红线
    ├── db_storage/                      L3 红线（聊天 DB 群）
    ├── msg/                             ⚠ 名字像红线但其实是用户内容
    │   ├── attach/<chat-hash>/          L2 语音/附件
    │   ├── file/<YYYY-MM>/              L2 接收文件
    │   ├── migrate/                     keep
    │   └── video/<YYYY-MM>/             L2 视频 + 图片混存（见 7.5）
    ├── resource/                        keep（一般空）
    └── temp/                            L1 临时（含 ImageTemp / InputTemp / head_image / session-id 子目录）
```

`%APPDATA%/Tencent/xwechat/`：

```
All Users/        L3 红线
XPlugin/          L3 红线（插件）
config/           L3 红线
confsdk/          L1 远程配置 SDK 缓存
crashinfo/        L1 崩溃报告（reports/ + attachments/）
ilink/            keep（网络/登录态）
log/              L1 运行日志（player/ + radium/ 等）
login/            L3 红线
net/, net_1/      keep（网络握手缓存，谨慎）
radium/           keep
uh/               keep
update/           L1（download/ + patch/，更新落地后可清）
```

### 7.3 Section 2 红线条目对 4.x 的修订

| 原红线 | 4.x 现实 | 修订 |
|------|--------|----|
| `**/Msg/**` | 4.x 的 `msg/` 是用户**接收文件**（file/）+ 视频（video/）+ 附件（attach/），**不是** DB | ❌ 删除该红线，改为针对具体 DB 路径 |
| `**/Accounts/**` | 4.x 没有 Accounts/，等价的是 `all_users/login`、`<wxid>/config/`、`Roaming/.../{login,config,All Users}` | 改为列出具体路径 |
| `**/*.db` | 仍然有效（位于 `db_storage/`、`all_users/sqlite/`、Roaming 下） | 保留 |

### 7.4 4.x 版 TOML scope 蓝图（最终落到 wechat-pc.toml）

| scope id | label | glob | mode | prompt |
|---------|------|------|-----|--------|
| `chat-media-cache` | 聊天图片/媒体缓存 | `**/xwechat_files/wxid_*/cache/*/Message/**` | recycle | days=30 |
| `web-resource-cache` | Web/小程序资源缓存 | `**/xwechat_files/wxid_*/cache/*/{HttpResource,WeAppIcon}/**` | recycle | none |
| `sticker-cache` | 表情/贴图缓存 | `**/xwechat_files/wxid_*/{cache/*/Emoticon,business/emoticon}/**` | recycle | days=30 |
| `temp-files` | 临时文件 | `**/xwechat_files/wxid_*/temp/**` | recycle | none |
| `avatar-cache` | 头像缓存 | `**/xwechat_files/all_users/head_imgs/**` | recycle | none |
| `apm-records` | APM 遥测 | `**/xwechat_files/wxid_*/apm_record/**` | recycle | none |
| `received-files` | 接收的文件 | `**/xwechat_files/wxid_*/msg/file/**` | recycle | days=30 |
| `received-videos` | 接收的视频 | `**/xwechat_files/wxid_*/msg/video/**/*.{mp4,mov,m4v,3gp,mkv,webm,avi,m4s}` | recycle | days=7 |
| `received-images` | 接收的图片 | `**/xwechat_files/wxid_*/msg/video/**/*.{jpg,jpeg,png,gif,webp,bmp,heic,heif}` | recycle | days=30 |
| `voice-attachments` | 语音/消息附件 | `**/xwechat_files/wxid_*/msg/attach/**` | recycle | days=30 |
| `chat-backups` | 聊天备份 | `**/xwechat_files/Backup/**` | recycle | confirm=false |
| `app-logs-crashes` | 应用日志/崩溃报告 | `%APPDATA%/Tencent/xwechat/{log,crashinfo}/**` | recycle | none |
| `app-update-leftover` | 安装包/远程配置缓存 | `%APPDATA%/Tencent/xwechat/{update,confsdk}/**` | recycle | none |

3.x 兼容 scope 4 个（图片/视频/文件/Cache）作为 legacy 同时保留，glob 用 `**/WeChat Files/wxid_*/FileStorage/{Image,Video,File,Cache}/**` 形态。

### 7.5 实测附录：4.x 图片/视频共用 video/ 目录

**勘测发现（2026-05）**：4.x 实际不存在 `msg/image/` 子目录。用户接收的图片和视频都堆在 `msg/video/<YYYY-MM>/` 下，按文件名后缀区分：

```
msg/video/2026-05/
├── <hash>.jpg               图片原文件
├── <hash>_thumb.jpg         视频缩略图（_thumb 后缀）
├── <hash>.mp4               视频
└── <hash>_raw.mp4           高清版视频（_raw 后缀）
```

**对 7.4 蓝图的修订**：原蓝图 `received-videos` 用 `msg/video/**` 会同时命中图片，`received-images` 当时给的 `msg/image/**` 在 4.x 是死路径。最终落到 wechat-pc.toml 的两个 scope 共用 `msg/video/**` 树，按 brace 后缀分流——简单一致，缩略图按后缀归图片桶，高清版按后缀归视频桶。

**为什么不"_thumb.jpg 跟着主视频走"**：这要么靠正则要么靠后端配对，把声明式 schema 复杂度推高一档；用户两个桶一起清就完美，单清一边留下的孤儿缩略图 WeChat 重生即可。符合 80% 哲学（简单解决 80% 场景，砍长尾）。

**safety test 覆盖**：`crates/scaffold/tests/wechat_pc_safety.rs` 已加 `_thumb.jpg`/`_raw.mp4` 正向断言 + 桶不交叉反向断言（mp4 不落 received-images，jpg 不落 received-videos）。

### 7.6 实测附录：3.x 顶层目录与 FileStorage 双布局并存

**信息来源（2026-05）**：`CleanMyWeChat` 项目 `get_fileNum` 函数显示，3.x 时代用户数据有两套并存的目录布局——早期用 `<wxid>/` 顶层目录，后期统一搬进 `<wxid>/FileStorage/`。CleanMyWeChat 的每个清理桶都同时扫两套：

| 类别（CleanMyWeChat 概念） | path_one（顶层老布局） | path_two（FileStorage 新布局） |
|----|----|----|
| 图片缩略图缓存（picCache） | `<wxid>/Attachment/` | `<wxid>/FileStorage/Cache/` |
| 接收文件（file） | `<wxid>/Files/` | `<wxid>/FileStorage/File/` |
| 接收图片（pic） | `<wxid>/Image/Image/` | `<wxid>/FileStorage/Image/` |
| 接收视频（video） | `<wxid>/Video/` | `<wxid>/FileStorage/Video/` |

**对 7.4/3.x legacy 蓝图的修订**：原蓝图只覆盖 path_two（FileStorage/*）。落到 wechat-pc.toml 的 4 个 3.x scope 用 brace 把双路径合并到同一 glob：

| scope id | glob |
|----|----|
| `image-cache-3x` | `**/WeChat Files/wxid_*/{Image/Image,FileStorage/Image}/**` |
| `video-cache-3x` | `**/WeChat Files/wxid_*/{Video,FileStorage/Video}/**` |
| `file-cache-3x` | `**/WeChat Files/wxid_*/{Files,FileStorage/File}/**` |
| `cache-misc-3x` | `**/WeChat Files/wxid_*/{Attachment,FileStorage/Cache}/**` |

`Attachment/` 按 CleanMyWeChat 的 picCache 语义并入 `cache-misc-3x`（与 `FileStorage/Cache/` 等价），不单开 scope——3.x 装机量正在萎缩，UI 卡片不必再多一张。

`voice-cache-3x` / `sticker-cache-3x` / `temp-files-3x` / `msg-attach-3x` 维持只扫 FileStorage/*，CleanMyWeChat 没揭示这几类有 path_one。

**safety test 覆盖**：每个改动 scope 都新增了一条 path_one 风格的正向命中（`Image/Image/2026-05/img.dat`、`Video/2026-05/clip.mp4`、`Files/2026-05/note.pdf`、`Attachment/2026-05/thumb.dat`）；红线断言不变（Msg/MultiMsg/config/Accounts/Favorites/FileStorage/CustomEmotion 仍 zero match）。
