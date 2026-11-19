# Vellumi 0.6（Tauri）发布手册

`next/scripts/release.sh` 负责构建、测试和验证，**不会发布任何东西**。下面这些步骤会创建密钥、或者对外发布，需要你本人来做。

## 0. 一次性准备：更新签名密钥（启用自动更新前必须做）

Tauri 的自动更新用 minisign 签名。它和 Sparkle 用的 EdDSA 是两套不同的密钥。

```bash
cd next/apps/desktop && npx tauri signer generate -w ~/.tauri/vellumi-updater.key
```

- 把输出的**公钥**填进 `next/apps/desktop/src-tauri/tauri.updater.conf.json`，替换其中的 `REPLACE_WITH_PUBLIC_KEY`。
- **私钥**和口令要妥善保管，不要提交进仓库。私钥一旦丢失，已安装的用户就再也收不到更新。
- 以后每次发布前设置好环境变量，`release.sh` 检测到后会自动启用 updater，并生成带签名的更新包：

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/vellumi-updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD='…'
```

如果没有设置这两个变量，构建出来的版本**不含自动更新**，菜单里也不会出现“检查更新…”。这样的版本可以正常使用，只是要靠手动下载新版。

## 1. 构建

```bash
cd next && scripts/release.sh
```

依次执行：
1. 全部测试闸门：Rust 测试、clippy、JS 测试、类型检查、Markdown 保真测试、界面自测。
2. 构建 Universal（Apple 芯片 + Intel）的 `Vellumi.app` 和 DMG，并做 ad-hoc 签名。
3. 校验 bundle id、版本号和架构。
4. 冒烟测试：用**发布版二进制**完整跑一遍界面自测。

产物放在 `next/dist/`：
- `Vellumi-<版本>.dmg`
- `Vellumi-<版本>.app.zip`

脚本会打印两者的 sha256。

## 2. 把 0.5.x 用户接过来（Sparkle → Tauri）

0.5.x 用 Sparkle 检查更新，feed 是 `appcast.xml`。最后一次通过 Sparkle 发布时，让它把 0.6.0 送到用户手里，之后的更新就交给 Tauri updater。

1. 用**现有的 Sparkle EdDSA 私钥**给 `Vellumi-0.6.0.app.zip` 签名，签名工具是 Sparkle 自带的 `sign_update`。
2. 在 `appcast.xml` 里加一个 0.6.0 条目，`enclosure` 指向 GitHub Release 上的 zip。
3. **发布前必须先实测**（这就是 P0.7，至今还没有验证过）：在一台装着 0.5.3 的 Mac 上点“检查更新”，确认能装上并启动 0.6.0。

   两个版本都是 ad-hoc 签名、bundle id 相同，按理 Sparkle 只校验 EdDSA 签名就会放行，但这一点必须实机确认。如果装不上，就退回第二种方案：在 0.5.x 的 appcast 里发一条说明，引导用户手动下载 DMG。

## 3. GitHub Release

上传以下文件：
- `Vellumi-<版本>.dmg`
- `Vellumi-<版本>.app.zip`
- 启用了 updater 时：再加上 Tauri 生成的 `.app.tar.gz`、`.sig` 和 `latest.json`

`tauri.updater.conf.json` 里的更新地址是 `releases/latest/download/latest.json`，所以只要这一版标记为 latest，已安装的用户就能收到更新。

## 4. Homebrew（`SUDAcyber/tap`）

在 `Casks/vellumi.rb` 里更新 `version` 和 `sha256`（取 DMG 的值）。App 名称仍然是 `Vellumi.app`。

## 首次启动时用户会看到什么

- **Gatekeeper**：版本仍是 ad-hoc 签名（D12），第一次打开需要右键选“打开”，或者在“系统设置 ▸ 隐私与安全性”里放行。
- **设置迁移**：新版第一次启动、还没有自己的设置文件时，会**只读地**导入 0.5.x 的偏好设置，包括外观、主题、缩放、排印、项目、草稿文件夹，并显示一条“已从旧版 Vellumi 导入”的提示。旧的偏好文件不会被改动。
- **可能出现的系统授权弹窗**：macOS 14 及以上，读取 0.5.x 沙盒容器时可能弹出“想要访问其他 App 的数据”。新旧版本的 bundle id 相同，按理不会弹，但需要在干净的系统上确认。

## 回滚

- 0.5.x 的偏好文件始终没有被改动，重新安装 0.5.3 就能完全恢复旧状态。
- 新版的设置在 `~/Library/Application Support/com.aaron.vellumi/settings.json`，删掉它会在下次启动时重新迁移一次。
- 搜索索引在 `~/Library/Caches/com.aaron.vellumi/`，可以随时删除，删掉后会自动重建。
