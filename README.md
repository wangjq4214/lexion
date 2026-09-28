# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup


## macOS 打包

在安装了 Bun、Rust 和 Xcode Command Line Tools 的 macOS 机器上运行：

```sh
bun install --frozen-lockfile
bun run build:mac
```

生成的 `.app` 和 `.dmg` 位于 `src-tauri/target/release/bundle/`。

macOS 构建从 `src-tauri/Info.macos.plist` 合并本地网络用途说明与 `_lexion-pair._tcp` Bonjour 声明。重装后在 Mac 上核对实际产物并观察广播：

```sh
plutil -p /Applications/lexion.app/Contents/Info.plist | grep -E 'NSLocalNetworkUsageDescription|NSBonjourServices|_lexion-pair'
dns-sd -B _lexion-pair._tcp local.
```

启动后的 mDNS 注册、浏览、解析事件与错误会输出到应用进程的标准错误；在终端运行 `/Applications/lexion.app/Contents/MacOS/lexion` 可收集日志，异步 daemon 错误也会出现在应用局域网状态中。API 返回注册请求成功不等于已经广播，需在另一台设备或终端用 `dns-sd` 验证。Apple Developer 签名仍需配置有效证书/签名身份并在 Mac 上验证 `codesign -dv --verbose=4 /Applications/lexion.app`；现有 CI 无签名凭据，不能把临时签名当作已解决。

## GitHub Release

将与 `src-tauri/tauri.conf.json` 中版本一致的 `vX.Y.Z` tag 推送到 GitHub（例如 `v0.1.0`），**Build and release desktop apps** 工作流会构建 Windows NSIS 安装包和适用于 Apple Silicon / Intel 的 macOS DMG。两个平台都成功后，工作流自动生成发布说明并将安装包上传到已发布的 GitHub Release；失败时不会发布不完整的版本。重新运行工作流会更新同一 Release 的安装包。

Release job 使用内置 `GITHUB_TOKEN` 的 `contents: write` 权限；仓库的 Actions 工作流权限设置必须允许该权限。macOS 安装包当前未经过 Apple Developer 签名或公证，首次打开可能受到 Gatekeeper 限制；公开分发前需配置相应证书和公证凭据。
- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
