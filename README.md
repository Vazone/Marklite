# MarkLite

[中文说明](README.zh-CN.md) | [English README](README.en.md)

MarkLite is a lightweight cross-platform Markdown editor by **Vazone**, built with Rust, Tauri 2, Svelte, TypeScript, CodeMirror 6, and pulldown-cmark.

MarkLite 是由 **Vazone** 发起的轻量级跨平台 Markdown 编辑器，适合在 Windows、macOS 和 Linux 上进行本地写作、预览、笔记整理和文档管理。

![MarkLite interface screenshot](assets/marklite-screenshot.png)

## Download

Download Windows x64, macOS Intel/Apple Silicon, or Linux x64 packages from [MarkLite Releases](https://github.com/Vazone/Marklite/releases). Check each release's signing notice and `SHA256SUMS.txt` before installation.

## Highlights

- Cross-platform Markdown editor with a small Tauri/Rust desktop footprint
- Live Markdown preview, split view, edit-only mode, and preview-only mode
- CodeMirror 6 editing experience with toolbar, search, line numbers, word wrap, and multi-tab editing
- Save, discard, or cancel before app exit when documents still contain unsaved changes
- Recent files, outline, document stats, settings, theme options, and HTML export
- GitHub Actions packages Windows x64, Linux x64, macOS Intel, and macOS Apple Silicon releases
- Windows NSIS installer includes optional “Open with MarkLite” and default Markdown opener integration

## Quick Start

```bash
npm ci
npm run tauri dev
```

Build the verified Windows installer locally:

```bash
npm run package:windows
```

Output:

```text
src-tauri/target/release/bundle/nsis/MarkLite_<version>_x64-setup.exe
```

Linux and macOS packages are built natively by the tag-driven [GitHub Actions release workflow](.github/workflows/release.yml).

## License

MarkLite is open source under the [MIT License](LICENSE).

## Notice

This is a vibe coding / AI assisted coding project. If any material unintentionally infringes your rights, please contact Vazone through the GitHub repository so it can be reviewed and removed or replaced.

本项目是 vibe coding / AI assisted coding 方式完成的项目。如无意包含任何侵权内容，请通过 GitHub 仓库联系作者 Vazone，我会尽快核实并删除或替换。
