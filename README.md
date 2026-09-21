# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup


## macOS 打包

在安装了 Bun、Rust 和 Xcode Command Line Tools 的 macOS 机器上运行：

```sh
bun install --frozen-lockfile
bun run build:mac
```

生成的 `.app` 和 `.dmg` 位于 `src-tauri/target/release/bundle/`。GitHub Actions 中的 **Build macOS app** 工作流会生成通用二进制（Apple Silicon 和 Intel），可从工作流 artifact 下载。

当前构建不包含 Apple Developer 签名或公证；公开分发前需配置相应证书与 Apple 公证凭据。
- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
