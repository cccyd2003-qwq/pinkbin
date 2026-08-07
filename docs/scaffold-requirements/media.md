# 音乐播放器类清理需求

## 1. 范围

| 优先级 | 应用 | 状态 |
|---|---|---|
| P0 | 网易云音乐 PC 客户端 | 本轮新增 [`scaffolds/netease-cloud-music.toml`](../../scaffolds/netease-cloud-music.toml) |

本轮只覆盖 Windows Win32 客户端。UWP / Microsoft Store 容器和用户在设置里自定义的下载目录暂不自动识别，避免为了定位缓存而读取用户配置或音频元数据。

## 2. 路径候选与实测布局

默认数据根候选：

- `%LOCALAPPDATA%/NetEase/CloudMusic`（当前 Windows 客户端的主要数据根；大小写不敏感）
- `%APPDATA%/NetEase/CloudMusic`（旧版本或特殊安装方式）
- `**/NetEase/CloudMusic`（改盘符、便携版）

在本机只列目录名得到的布局如下；没有读取任何用户文件内容：

```text
CloudMusic/
├── Cache/Cache/                 旧版播放缓存（可重新获取）
├── Statics/                     静态资源缓存
├── Temp/                        临时文件 / 未完成的临时片段
├── Log/                         运行日志
├── cloudmusic.elog              主程序日志
├── update/                      更新下载残留
├── Library/                     本地库与 webdb，保留
├── webdata/file/                播放列表、最近播放等状态，保留
├── dumps/                       含 cookie_json，保留
├── webapp91x64/                 Chromium 内核目录
│   ├── Cache/ Code Cache/ GPUCache/  可重建缓存
│   ├── Local Storage/ IndexedDB/ databases/ blob_storage/  状态，保留
│   └── Session Storage/         状态，保留
├── aioresource/                 AI / 音频资源，保留
└── localdata / localware        本地状态，保留
```

## 3. 分级与红线

### L1：可重建缓存

- `Cache/Cache/**`：播放过程中产生的缓存，默认保留 30 天。
- `Statics/**`：静态资源，重新打开页面会再次下载。
- `webapp*/{Cache,Code Cache,GPUCache}/**`：Chromium 网络、代码和 GPU 缓存。
- `Temp/**`：临时文件。
- `Log/**`、`cloudmusic.elog`：运行日志。
- `update/**`：更新下载残留，删除后需要时重新下载。

所有 scope 使用 `recycle`，不做永久删除。

### L3：不纳入任何 scope

- 已下载歌曲和用户指定下载目录（例如 `.mp3`、`.flac`、`.ncm`）。
- `Library/`、`webdata/`、`localdata`、`localware`：本地库、播放列表、最近播放、设置等状态。
- `dumps/cookie_json`：登录 / Cookie 材料。
- `webapp*/Local Storage/`、`Session Storage/`、`IndexedDB/`、`databases/`、`blob_storage/`：网页应用状态，可能影响登录或客户端状态。
- `aioresource/`：模型或客户端资源。

清理前需退出网易云音乐，否则打开中的文件可能无法进入回收站；清理播放缓存后，封面或已缓存歌曲可能需要重新加载。
