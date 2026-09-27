<div align="center">
  <img src="assets/marklite-icon.png" alt="MarkLite Logo" width="112" />
  <h1>MarkLite</h1>
  <p><strong>轻快、专注的 Markdown 编辑体验，不加载沉重的工作区。</strong></p>
  <p>面向 Windows、macOS、Linux 和 Android 的本地优先 Markdown 编辑器，用于写作、预览、整理与导出 Markdown。</p>
  <p>
    <a href="README.md">English</a> ·
    <a href="README.zh-CN.md">简体中文</a> ·
    <a href="https://github.com/Vazone/Marklite/releases">下载</a> ·
    <a href="https://github.com/Vazone/Marklite/issues">问题反馈</a>
  </p>
  <p>
    <a href="https://github.com/Vazone/Marklite/releases">发布版本</a> ·
    <a href="https://github.com/Vazone/Marklite/actions/workflows/ci.yml">构建状态</a> ·
    <a href="https://github.com/Vazone/Marklite/blob/branch/LICENSE">MIT 许可证</a> ·
    <a href="https://github.com/Vazone/Marklite/stargazers">收藏</a> ·
    Windows · macOS · Linux · Android
  </p>
</div>

<p align="center">
  <img src="assets/marklite-showcase.gif" alt="MarkLite 展示 mixed-500-kib.md 中的公式、分栏滚动与章节 PNG 导出" width="100%" />
</p>
<p align="center"><sub>mixed-500-kib.md（500 KiB）：公式预览、分栏滚动与章节 PNG 导出</sub></p>

## 0.1.7 有哪些更新

- **目录工作区：**恢复上次打开的目录，通过可折叠目录树浏览、筛选 Markdown 文件；桌面端可右键在文件管理器中定位。
- **Android 加入项目：**通过系统文件选择器或文件管理器打开文档，授权访问目录，保存修改，并通过 Android 文档提供程序导出。
- **更完整的 Markdown：**Front Matter、文档目录、脚注、行内扩展、代码高亮、公式和 Mermaid 渲染改进。
- **写作保护：**恢复副本、覆盖前的外部变更检查，以及最近文件和恢复标签页的处理改进。
- **可选桌面更新：**后台检查新版，由你确认后再安装。
- **DOCX 导出改进：**修复重复图表、脚注图片、可编辑公式和链接等问题。

## 从目录到成稿

1. **选择目录**，在侧边栏浏览其中的 Markdown 文档；下次启动时恢复目录选择。
2. **选择编辑、预览或分栏布局**，打开多个文档，通过大纲导航标题。
3. **选择导出格式**：HTML 分享、PDF 阅读、DOCX 继续编辑、SVG 脑图，或按章节输出 PNG 图片。

| 格式 | 导出内容 |
| --- | --- |
| HTML | 包含格式化内容的独立文档 |
| PDF | 分批渲染并合并的分页文档 |
| DOCX | 保留受支持公式和图表的可编辑文档 |
| PNG | 按章节生成图片，归入与文档同名的文件夹 |
| SVG | 根据文档标题生成的矢量脑图 |

### Android

ARM64 APK 面向 Android 7.0 及以上（API 24）。文件和目录通过系统选择器授权访问，可用位置由 Android 权限和存储提供程序决定。竖屏布局避让系统栏，横屏提供更大的写作空间。最近文件、目录恢复、触控交互和导出复用编辑器核心能力。

桌面自动更新和 Android APK 安装是不同机制：Android 下载 APK 后由系统安装。旧 debug 签名测试版可能需要先卸载，再安装正式签名包；请提前保存文档。


## 为什么选择 MarkLite？

| | |
| --- | --- |
| **⚡ 打开就能写**<br>快速启动，在多个文档间流畅切换，Markdown 内容变长时依然保持顺畅。 | **🪶 安静的写作空间**<br>没有拥挤的项目面板，只保留文档、写作工具、预览和真正需要的信息。 |
| **✍️ 完整的写作空间**<br>CodeMirror 6、多标签、查找替换、快捷键、格式工具和可恢复的编辑状态。 | **📦 统一导出入口**<br>把当前 Markdown 快照导出为独立 HTML、PDF、可继续编辑的 DOCX，或把标题脑图导出为 SVG。 |
| **👀 理解本地文件的预览**<br>可拖动分栏、文内锚点、本地 Markdown 跳转、受控本地图片和外链确认。 | **🔒 默认本地优先**<br>文档始终是由你控制的普通文件；预览 HTML 进入界面前会经过清洗。 |

## 功能

### 写作与导航

- 新建、打开、保存和另存为 `.md`、`.markdown`、`.txt` 文件。
- 多文档标签、未保存标识、横向标签浏览，以及类似浏览器的**关闭**、**关闭其他标签**、**关闭右侧标签**操作。
- Markdown 高亮、行号、自动换行、当前行高亮、折叠、查找替换、撤销重做和可配置缩进。
- 通过工具栏或键盘插入标题、强调、引用、代码、列表、任务列表、链接、图片、表格和分隔线。
- 应用运行期间可恢复每个标签的选区、撤销历史和滚动位置。

### 预览与整理

- 在编辑、预览和分栏三种布局之间切换；分栏双向滚动同步，快速连续滚轮保持预览位置稳定。
- 拖动分栏线调整编辑区和预览区宽度，也可在边缘收纳为单栏。
- 调整最近文件侧栏宽度、隐藏侧栏，并且无需重启即可重新展开。
- 在不挤压编辑区的前提下浏览最近文件、文档大纲、文档统计和大量标签。
- 按明确类型处理文内标题、本地 Markdown/文本文件和 HTTP/HTTPS 外链。
- 开启本地图片后，可受控显示 PNG、JPEG、GIF 和 WebP 图片。
- 把文档标题层级转换为彩色脑图，支持分支展开折叠、拖动画布和点击节点跳回编辑行。
- 在完整离线指南中学习基础、扩展与脑图语法，无需打开外部教程。

### 导出与桌面工作流

- 通过统一的**导出为…**入口生成独立 HTML、PDF、可编辑 DOCX，完整展开的矢量脑图 SVG，或按章节输出 PNG 图片。
- 每章导出一张 PNG，存入源文档旁的同名文件夹；未保存文档可选择目标父目录。
- 导出显示真实阶段与章节/批次进度，支持图片导出取消；CLI 和 Windows 右键导出也显示处理进度。
- 大文档预览按可见区域加载，PDF 分批生成并合并，减少长文章渲染和导出的内存压力。
- 导出任务冻结文档快照；选择保存位置期间继续编辑或切换标签也不会改变已经开始的导出内容。
- 支持浅色、深色和跟随系统主题，并可调整强调色、编辑器字体、预览延迟、自动保存、侧栏和状态栏。
- 支持把文件拖入窗口，或通过操作系统参数与第二实例打开文件。
- 关闭脏文档或退出程序时，可选择保存、不保存或取消。
- Windows NSIS 安装器可选添加 **Open with MarkLite**、Markdown 文件关联及 PDF/Word/HTML 转换菜单。

## 下载

**MarkLite v0.1.7** · Windows、macOS、Linux 与 Android。安装包和更新说明统一通过 GitHub Releases 提供。

| 平台 | 安装包 |
| --- | --- |
| Windows x64 | [`MarkLite_0.1.7_x64-setup.exe`](https://github.com/Vazone/Marklite/releases/download/v0.1.7/MarkLite_0.1.7_x64-setup.exe) |
| macOS Apple Silicon | [`MarkLite_0.1.7_macos_aarch64.dmg`](https://github.com/Vazone/Marklite/releases/download/v0.1.7/MarkLite_0.1.7_macos_aarch64.dmg) |
| macOS Intel | [`MarkLite_0.1.7_macos_x86_64.dmg`](https://github.com/Vazone/Marklite/releases/download/v0.1.7/MarkLite_0.1.7_macos_x86_64.dmg) |
| Linux x64 AppImage | [`MarkLite_0.1.7_linux_x86_64.AppImage`](https://github.com/Vazone/Marklite/releases/download/v0.1.7/MarkLite_0.1.7_linux_x86_64.AppImage) |
| Linux amd64 Debian | [`MarkLite_0.1.7_linux_amd64.deb`](https://github.com/Vazone/Marklite/releases/download/v0.1.7/MarkLite_0.1.7_linux_amd64.deb) |
| Android ARM64 | [`MarkLite_0.1.7_android_arm64.apk`](https://github.com/Vazone/Marklite/releases/download/v0.1.7/MarkLite_0.1.7_android_arm64.apk) |

请使用 [`SHA256SUMS.txt`](https://github.com/Vazone/Marklite/releases/download/v0.1.7/SHA256SUMS.txt) 核验下载文件，完整更新内容见 [v0.1.7 Release](https://github.com/Vazone/Marklite/releases/tag/v0.1.7)。

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
npm test
npm run test:tooling
npm run build
npm run build:cli
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --locked --all-features
```

构建经过校验的 Windows NSIS 安装包：

```bash
npm run package:windows
```

CI 和 Release 仅手动触发。Actions 构建四个桌面目标及 Android ARM64 未签名 APK；APK 在维护者本机签名，全部产物核验后再发布。

### 构建 Android

安装 JDK 17，并按 [toolchain.json](scripts/android/toolchain.json) 安装 SDK、Build Tools 和 NDK，配置 `JAVA_HOME`、`ANDROID_HOME` 和 `NDK_HOME`。

```bash
rustup target add aarch64-linux-android
npm ci
npm run android:build -- --target aarch64 --debug
```

本地测试 APK 输出到 `release/android/arm64/`。手动 Release 工作流输出未签名的优化 APK，由维护者本机签名后发布；私钥不会上传 Actions。

## 参与贡献

欢迎提交 Issue 和 Pull Request。提出修改时请提供复现步骤和相关测试。如需报告敏感安全问题，请先通过[维护者主页](https://github.com/Vazone)联系，不要直接在公开 Issue 中披露细节。

## 许可证

MarkLite 使用 [MIT License](LICENSE) 开源。

## 作者与说明

项目由 [Vazone](https://github.com/Vazone) 创建并维护。

MarkLite 是一个 AI 辅助开发的开源项目。如果仓库中的内容无意侵犯了你的权利，请通过 GitHub 联系作者；确认后将酌情删除或替换相关内容。

<div align="center">
  <strong>如果 MarkLite 让 Markdown 写作变得更轻松，欢迎给项目一个 Star。</strong><br><br>
  <a href="https://github.com/Vazone/Marklite/stargazers">⭐ Star MarkLite</a>
</div>
