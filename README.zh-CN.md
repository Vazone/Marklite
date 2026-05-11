# MarkLite

MarkLite 是由 **Vazone** 发起的轻量级 Windows Markdown 编辑器，面向日常写作、笔记整理、文档预览和本地 Markdown 文件管理。项目基于 Rust、Tauri 2、Svelte、TypeScript、CodeMirror 6 和 pulldown-cmark 构建，目标是在 Windows 上提供比 Electron 更轻、更快、更安静的 Markdown 编辑体验。

> 本项目是 vibe coding / AI assisted coding 方式完成的开源项目。如果项目中无意包含任何可能侵权的内容，请在 GitHub 仓库联系作者 Vazone，我会尽快核实并删除或替换相关内容。

## 关键词

Markdown editor, Windows Markdown editor, Rust Markdown editor, Tauri Markdown editor, CodeMirror editor, lightweight notes app, Markdown preview, Windows desktop Markdown, MarkLite, Windows 笔记软件, Markdown 编辑器, Tauri 桌面应用。

## 功能

- 新建、打开、保存、另存为 `.md`、`.markdown`、`.txt` 文件
- 多标签编辑，未保存状态提示，关闭未保存文件确认
- CodeMirror 6 编辑器，支持 Markdown 高亮、行号、自动换行、当前行高亮、查找
- Markdown 工具栏：加粗、斜体、删除线、标题、引用、代码块、行内代码、列表、任务列表、链接、图片、表格、分割线
- Rust 后端使用 pulldown-cmark 渲染 Markdown
- 使用 ammonia 清洗预览 HTML，避免脚本执行
- 编辑、预览、分栏三种布局
- 最近文件、文档大纲、文档信息侧边栏
- 设置窗口：主题、强调色、字体、字号、行高、行号、自动换行、自动保存、预览延迟、状态栏、侧边栏
- 导出 HTML
- 拖拽文件打开
- Windows 安装器支持可选注册右键菜单和 Markdown 默认打开方式

## 技术栈

- Rust
- Tauri 2
- Svelte 5
- TypeScript
- Vite
- CodeMirror 6
- pulldown-cmark
- ammonia

## 开发环境

需要安装：

- Node.js 24+
- npm 11+
- Rust stable
- Microsoft Visual Studio Build Tools
- Microsoft Edge WebView2 Runtime

检查 Tauri 环境：

```bash
npm run tauri -- info
```

## 安装依赖

```bash
npm install
```

## 开发运行

```bash
npm run tauri dev
```

如果只想查看前端界面：

```bash
npm run dev
```

## 构建安装程序

推荐使用项目封装的 Windows 打包命令：

```bash
npm run package:windows
```

该命令会生成 NSIS 安装程序，并加入 MarkLite 的 Windows 集成选项：

- 添加 “Open with MarkLite” 到右键菜单
- 可选设置 MarkLite 为 `.md` / `.markdown` 文件默认打开方式
- Release 版本不再打开 Windows 终端窗口

输出路径：

```text
src-tauri/target/release/bundle/nsis/MarkLite_0.1.0_x64-setup.exe
```

## 许可证

MarkLite 使用 [MIT License](LICENSE) 开源。

## 贡献

欢迎提交 Issue 和 Pull Request。仓库公开，但主分支会启用保护规则，外部贡献需要 Pull Request 并由作者 Vazone 审核后合并。

## 作者

作者：Vazone  
仓库：https://github.com/Vazone/Marklite
