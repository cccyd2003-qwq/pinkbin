# pinkbin-cli 二进制

本目录**不包含** exe 文件（编译产物不进 git，走 Release 分发）。

## 下载

从 GitHub Release 下载对应平台的二进制：

```bash
# Windows x86_64（推荐）
curl -L -o pinkbin-cli.exe \
  https://github.com/cccyd2003-qwq/pinkbin/releases/download/v0.1.2-qwenpaw/pinkbin-cli.exe
```

下载后放到本目录，确保路径为：`skills/disk-cleanup/bin/pinkbin-cli.exe`

## 从源码构建

```bash
# 需要 Rust + mingw-w64（Windows 交叉编译）
cargo build --release -p pinkbin-cli --target x86_64-pc-windows-gnu
cp target/x86_64-pc-windows-gnu/release/pinkbin-cli.exe skills/disk-cleanup/bin/
```

## 校验

下载后建议校验 SHA256：

```bash
sha256sum pinkbin-cli.exe
```

预期值见 Release 说明。