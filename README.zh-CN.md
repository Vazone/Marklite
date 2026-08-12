<div align="center">
  <img src="assets/marklite-icon.png" alt="MarkLite Logo" width="112" />
  <h1>MarkLite</h1>
  <p><strong>轻快、专注的 Markdown 编辑体验，不加载沉重的工作区。</strong></p>
  <p>面向 Windows、macOS 和 Linux 的本地优先桌面编辑器，用于写作、预览、整理与导出 Markdown。</p>
  <p>
    <a href="README.md">English</a> ·
    <a href="README.zh-CN.md">简体中文</a> ·
    <a href="https://github.com/Vazone/Marklite/releases">下载</a> ·
    <a href="https://github.com/Vazone/Marklite/issues">问题反馈</a>
  </p>
  <p>
    <a href="https://github.com/Vazone/Marklite/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/Vazone/Marklite?include_prereleases&sort=semver&style=flat-square&color=20b2aa" /></a>
    <a href="https://github.com/Vazone/Marklite/actions/workflows/ci.yml"><img alt="CI 状态" src="https://github.com/Vazone/Marklite/actions/workflows/ci.yml/badge.svg?branch=branch" /></a>
    <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/github/license/Vazone/Marklite?style=flat-square" /></a>
    <a href="https://github.com/Vazone/Marklite/stargazers"><img alt="GitHub Stars" src="https://img.shields.io/github/stars/Vazone/Marklite?style=flat-square&logo=github" /></a>
    <img alt="支持平台" src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-4c566a?style=flat-square" />
  </p>
</div>

<p align="center">
  <img src="assets/marklite-screenshot.png" alt="MarkLite 分栏编辑 Markdown 文档" width="100%" />
</p>

## 为什么选择 MarkLite？

| | |
| --- | --- |
| **⚡ 打开就能写**<br>快速启动，在多个文档间流畅切换，Markdown 内容变长时依然保持顺畅。 | **🪶 安静的写作空间**<br>没有拥挤的项目面板，只保留文档、写作工具、预览和真正需要的信息。 |
| **✍️ 完整的写作空间**<br>CodeMirror 6、多标签、查找替换、快捷键、格式工具和可恢复的编辑状态。 | **📦 统一导出入口**<br>把当前 Markdown 快照导出为独立 HTML、PDF 或可继续编辑的 DOCX。 |
| **👀 理解本地文件的预览**<br>可拖动分栏、文内锚点、本地 Markdown 跳转、受控本地图片和外链确认。 | **🔒 默认本地优先**<br>文档始终是由你控制的普通文件；预览 HTML 进入界面前会经过清洗。 |

## 功能

### 写作与导航

- 新建、打开、保存和另存为 `.md`、`.markdown`、`.txt` 文件。
- 多文档标签、未保存标识、横向标签浏览，以及类似浏览器的**关闭**、**关闭其他标签**、**关闭右侧标签**操作。
- Markdown 高亮、行号、自动换行、当前行高亮、折叠、查找替换、撤销重做和可配置缩进。
- 通过工具栏或键盘插入标题、强调、引用、代码、列表、任务列表、链接、图片、表格和分隔线。
- 应用运行期间可恢复每个标签的选区、撤销历史和滚动位置。

### 预览与整理

- 在编辑、预览和分栏三种布局之间切换。
- 拖动分栏线调整编辑区和预览区宽度，也可在边缘收纳为单栏。
- 调整最近文件侧栏宽度、隐藏侧栏，并且无需重启即可重新展开。
- 在不挤压编辑区的前提下浏览最近文件、文档大纲、文档统计和大量标签。
- 按明确类型处理文内标题、本地 Markdown/文本文件和 HTTP/HTTPS 外链。
- 开启本地图片后，可受控显示 PNG、JPEG、GIF 和 WebP 图片。

### 导出与桌面工作流

- 通过统一的**导出为…**入口生成独立 HTML、PDF 或可编辑 DOCX。
- 导出任务冻结文档快照；选择保存位置期间继续编辑或切换标签也不会改变已经开始的导出内容。
- 支持浅色、深色和跟随系统主题，并可调整强调色、编辑器字体、预览延迟、自动保存、侧栏和状态栏。
- 支持把文件拖入窗口，或通过操作系统参数与第二实例打开文件。
- 关闭脏文档或退出程序时，可选择保存、不保存或取消。
- Windows NSIS 安装器可选添加 **Open with MarkLite** 和 Markdown 文件关联。

## 下载

当前公开构建为 **MarkLite v0.1.5 Pre-release**。

| 平台 | 安装包 |
| --- | --- |
| Windows x64 | [`MarkLite_0.1.5_x64-setup.exe`](https://github.com/Vazone/Marklite/releases/download/v0.1.5/MarkLite_0.1.5_x64-setup.exe) |
| macOS Apple Silicon | [`MarkLite_0.1.5_macos_aarch64.dmg`](https://github.com/Vazone/Marklite/releases/download/v0.1.5/MarkLite_0.1.5_macos_aarch64.dmg) |
| macOS Intel | [`MarkLite_0.1.5_macos_x86_64.dmg`](https://github.com/Vazone/Marklite/releases/download/v0.1.5/MarkLite_0.1.5_macos_x86_64.dmg) |
| Linux x64 AppImage | [`MarkLite_0.1.5_linux_x86_64.AppImage`](https://github.com/Vazone/Marklite/releases/download/v0.1.5/MarkLite_0.1.5_linux_x86_64.AppImage) |
| Linux amd64 Debian | [`MarkLite_0.1.5_linux_amd64.deb`](https://github.com/Vazone/Marklite/releases/download/v0.1.5/MarkLite_0.1.5_linux_amd64.deb) |

请使用 [`SHA256SUMS.txt`](https://github.com/Vazone/Marklite/releases/download/v0.1.5/SHA256SUMS.txt) 核验下载文件，完整更新内容见 [v0.1.5 Release](https://github.com/Vazone/Marklite/releases/tag/v0.1.5)。

## 技术架构

| 层级 | 技术 | 职责 |
| --- | --- | --- |
| 桌面外壳 | Rust + Tauri 2 | 原生窗口、文件操作、对话框、生命周期和打包 |
| 界面 | Svelte 5 + TypeScript | 应用状态和响应式桌面界面 |
| 编辑器 | CodeMirror 6 | Markdown 编辑、视口渲染、历史、搜索和键盘操作 |
| Markdown | pulldown-cmark + ammonia | 原生解析、大纲/统计生成和安全预览 HTML |
| 构建 | Vite + GitHub Actions | 前端构建和 Windows/macOS/Linux 发布包 |

## 从源码运行

### 环境要求

- Node.js 24+
- npm 11+
- Rust 1.88，由 `rust-toolchain.toml` 固定
- [Tauri 前置要求](https://v2.tauri.app/zh-cn/start/prerequisites/)中对应平台的系统依赖

```bash
git clone https://github.com/Vazone/Marklite.git
cd Marklite
npm ci
npm run tauri dev
```

仅运行前端：

```bash
npm run dev
```

核心验证：

```bash
npm run check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked --all-features
```

构建经过校验的 Windows NSIS 安装包：

```bash
npm run package:windows
```

版本 tag 会触发 GitHub Actions，在原生 runner 上构建 Windows x64、Linux x64、macOS Apple Silicon 和 macOS Intel 安装包；所有平台任务成功后才发布 Release。

## 参与贡献

欢迎提交 Issue 和 Pull Request。提出修改前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。如需报告敏感安全问题，请先通过[维护者主页](https://github.com/Vazone)联系，不要直接在公开 Issue 中披露细节。

## 许可证

MarkLite 使用 [MIT License](LICENSE) 开源。

## 作者与说明

项目由 [Vazone](https://github.com/Vazone) 创建并维护。

MarkLite 是一个 AI 辅助开发的开源项目。如果仓库中的内容无意侵犯了你的权利，请通过 GitHub 联系作者；确认后将酌情删除或替换相关内容。

<div align="center">
  <strong>如果 MarkLite 让 Markdown 写作变得更轻松，欢迎给项目一个 Star。</strong><br><br>
  <a href="https://github.com/Vazone/Marklite/stargazers">⭐ Star MarkLite</a>
</div>
