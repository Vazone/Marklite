# MarkLite

[中文说明](README.zh-CN.md) | [English README](README.en.md)

MarkLite is a lightweight Windows Markdown editor by **Vazone**, built with Rust, Tauri 2, Svelte, TypeScript, CodeMirror 6, and pulldown-cmark.

MarkLite 是由 **Vazone** 发起的轻量级 Windows Markdown 编辑器，适合本地 Markdown 写作、预览、笔记整理和文档管理。

## Highlights

- Windows Markdown editor with a small Tauri/Rust desktop footprint
- Live Markdown preview, split view, edit-only mode, and preview-only mode
- CodeMirror 6 editing experience with toolbar, search, line numbers, word wrap, and multi-tab editing
- Recent files, outline, document stats, settings, theme options, and HTML export
- NSIS installer with optional right-click “Open with MarkLite” registration
- Optional installer checkbox to register MarkLite as the default Markdown opener
- Release builds use the Windows GUI subsystem, so launching MarkLite does not open a terminal window

## Quick Start

```bash
npm install
npm run tauri dev
```

Build the Windows installer:

```bash
npm run package:windows
```

Output:

```text
src-tauri/target/release/bundle/nsis/MarkLite_0.1.0_x64-setup.exe
```

## License

MarkLite is open source under the [MIT License](LICENSE).

## Notice

This is a vibe coding / AI assisted coding project. If any material unintentionally infringes your rights, please contact Vazone through the GitHub repository so it can be reviewed and removed or replaced.

本项目是 vibe coding / AI assisted coding 方式完成的项目。如无意包含任何侵权内容，请通过 GitHub 仓库联系作者 Vazone，我会尽快核实并删除或替换。
