import { getLanguage, translate, type AppLanguage } from './i18n';
import capabilityContract from '../shared/markdown-capabilities.json';

export type MarkdownGuideGroup = 'basic' | 'extended' | 'mindMap';
export type MarkdownGuideSupport = 'supported' | 'limited' | 'unsupported';

export type MarkdownGuideTopic = {
  id: string;
  group: MarkdownGuideGroup;
  title: string;
  summary: string;
  example: string;
  support: MarkdownGuideSupport;
  fixture?: string;
  note?: string;
};

const zhCnMarkdownGuideGroups: ReadonlyArray<{
  id: MarkdownGuideGroup;
  label: string;
  description: string;
}> = [
  { id: 'basic', label: '基础语法', description: '标题、段落、排版、链接与图片' },
  { id: 'extended', label: '扩展语法', description: '表格、脚注、任务与兼容性' },
  { id: 'mindMap', label: '脑图', description: '用标题层级组织可交互脑图' }
];

const zhCnMarkdownGuideTopics = [
  {
    id: 'atx-headings', group: 'basic', title: '井号标题', support: 'supported',
    summary: '在行首使用 1–6 个 # 表示六级标题，井号后保留空格。',
    example: '# 一级标题\n\n## 二级标题\n\n###### 六级标题'
  },
  {
    id: 'setext-headings', group: 'basic', title: 'Setext 标题', support: 'supported',
    summary: '在文字下一行使用等号或短横线，可分别创建一级或二级标题。',
    example: '一级标题\n========\n\n二级标题\n--------'
  },
  {
    id: 'paragraphs', group: 'basic', title: '段落', support: 'supported',
    summary: '连续文字属于同一段；用空行开始新段落，不需要手动缩进。',
    example: '这是第一段，句子可以自然换行。\n仍然属于第一段。\n\n这是第二段。'
  },
  {
    id: 'line-breaks', group: 'basic', title: '强制换行', support: 'supported',
    summary: '行尾保留两个空格或写一个反斜杠，可在同一段内产生明确换行。',
    example: '第一行末尾有两个空格。  \n第二行。\n\n第三行末尾有反斜杠。\\\n第四行。'
  },
  {
    id: 'bold', group: 'basic', title: '粗体', support: 'supported',
    summary: '用两个星号或两个下划线包围需要强调的文字。',
    example: '**重要结论**\n\n__同样是粗体__'
  },
  {
    id: 'italic', group: 'basic', title: '斜体', support: 'supported',
    summary: '用一个星号或一个下划线包围文字；单词内部优先使用星号以减少歧义。',
    example: '*需要关注*\n\n_斜体说明_'
  },
  {
    id: 'bold-italic', group: 'basic', title: '粗斜体', support: 'supported',
    summary: '用三个星号或三个下划线组合粗体和斜体。',
    example: '***最高优先级***\n\n___同时粗体和斜体___'
  },
  {
    id: 'blockquotes', group: 'basic', title: '块引用', support: 'supported',
    summary: '在段落每行前使用 > 创建引用；引用中的空行也可保留 >。',
    example: '> 这是引用的第一段。\n>\n> 这是引用的第二段。'
  },
  {
    id: 'nested-blockquotes', group: 'basic', title: '嵌套引用', support: 'supported',
    summary: '连续增加 > 表示更深层引用，引用内也可使用强调、列表和链接。',
    example: '> 外层观点\n>\n> > 内层补充\n>\n> - 引用中的列表'
  },
  {
    id: 'ordered-lists', group: 'basic', title: '有序列表', support: 'supported',
    summary: '使用数字加点创建有序项；导出 DOCX 时会保留列表的起始数字。',
    example: '1. 准备资料\n2. 编写内容\n3. 完成验证'
  },
  {
    id: 'unordered-lists', group: 'basic', title: '无序列表', support: 'supported',
    summary: '使用短横线、星号或加号创建无序项；同一层建议保持一种标记。',
    example: '- 编辑\n- 预览\n- 导出'
  },
  {
    id: 'nested-list-content', group: 'basic', title: '列表嵌套与块内容', support: 'supported',
    summary: '缩进可以放置子列表；继续缩进还能在列表项中加入段落、引用、代码块或图片。',
    example: '1. 主任务\n   - 子任务 A\n   - 子任务 B\n\n     > 子任务说明'
  },
  {
    id: 'inline-code', group: 'basic', title: '行内代码', support: 'supported',
    summary: '用一个反引号包围命令、路径或代码标识；其中的 Markdown 标记不会继续解析。',
    example: '运行 `npm run check`，然后查看 `src/main.ts`。'
  },
  {
    id: 'escaped-backticks', group: 'basic', title: '代码中的反引号', support: 'supported',
    summary: '内容本身含反引号时，用两个反引号作为外层定界符。',
    example: '``在 Markdown 中写 `code` ``'
  },
  {
    id: 'indented-code-blocks', group: 'basic', title: '缩进代码块', support: 'supported',
    summary: '每行缩进四个空格可创建代码块；复杂代码通常更适合围栏代码块。',
    example: '普通段落\n\n    const ready = true;\n    console.log(ready);'
  },
  {
    id: 'horizontal-rules', group: 'basic', title: '分隔线', support: 'supported',
    summary: '单独一行使用三个或更多短横线、星号或下划线创建分隔线。',
    example: '上半部分\n\n---\n\n下半部分'
  },
  {
    id: 'inline-links', group: 'basic', title: '行内链接', support: 'supported',
    summary: '方括号写显示文字，圆括号写目标；本地文档目标相对当前已保存文档解析。',
    example: '[项目说明](./README.md)'
  },
  {
    id: 'link-titles', group: 'basic', title: '链接标题', support: 'supported',
    summary: '在链接目标后增加引号包围的标题，悬停时可显示补充说明。',
    example: '[项目说明](./README.md "打开项目说明")'
  },
  {
    id: 'angle-autolinks', group: 'basic', title: '网址与邮箱尖括号链接', support: 'supported',
    summary: '尖括号可以把完整网址或邮箱地址明确转换为链接。',
    example: '<https://docs.example.invalid/guide>\n\n<team@example.invalid>',
    note: '示例使用保留域名，不是 MarkLite 的外部帮助入口。'
  },
  {
    id: 'formatted-links', group: 'basic', title: '带格式的链接文字', support: 'supported',
    summary: '链接显示文字中可以使用粗体或斜体；代码样式也可以作为链接文字。',
    example: '[**重要说明**](./IMPORTANT.md)\n\n[`配置文件`](./settings.md)'
  },
  {
    id: 'reference-links', group: 'basic', title: '引用式链接', support: 'supported',
    summary: '正文只保留引用标签，目标集中定义在其他位置，适合多次复用或保持段落整洁。',
    example: '阅读[安装说明][install]和[安装说明][]。\n\n[install]: ./INSTALL.md "安装"\n[安装说明]: ./INSTALL.md'
  },
  {
    id: 'images', group: 'basic', title: '图片', support: 'supported',
    summary: '在链接语法前加 ! 插入图片；相对图片以当前已保存文档所在目录为基准。',
    example: '![界面截图](./images/screenshot.png "MarkLite 界面")',
    note: '本地图片需要在设置中启用；预览不会自动下载远程图片。'
  },
  {
    id: 'linked-images', group: 'basic', title: '可点击图片', support: 'supported',
    summary: '把图片语法放进链接文字位置，可以让图片成为链接入口。',
    example: '[![项目图标](./images/logo.png)](./README.md)'
  },
  {
    id: 'escaping-characters', group: 'basic', title: '转义字符', support: 'supported',
    summary: '在 Markdown 标点前加反斜杠，可按普通字符显示星号、井号、括号等符号。',
    example: '\\*这不是斜体\\*\n\n\\# 这不是标题\n\n\\[这不是链接\\]'
  },
  {
    id: 'inline-html', group: 'basic', title: '行内 HTML', support: 'limited',
    summary: 'Markdown 可以包含行内 HTML，但 MarkLite 会在显示前执行安全清洗。',
    example: '这是 <mark>标记文字</mark>，这是 <kbd>Ctrl</kbd>。',
    note: '只有清洗器允许的标签和属性会保留；脚本和事件属性不会执行。'
  },
  {
    id: 'block-html', group: 'basic', title: '块级 HTML', support: 'limited',
    summary: '块级 HTML 可被 parser 接收，但清洗结果和其中的 Markdown 再解析行为取决于安全边界。',
    example: '<div class="notice">\n  原始 HTML 内容\n</div>',
    note: '不要依赖任意 HTML、样式、iframe 或脚本构建文档功能。'
  },
  {
    id: 'extension-profile', group: 'extended', title: 'MarkLite 扩展能力', support: 'supported',
    summary: 'MarkLite 以 GFM 为基础，并显式扩展脚注、标题属性、GitHub alerts、定义列表和受限数学公式；普通标点保持原文。',
    example: '| 能力 | 状态 |\n| --- | --- |\n| 表格 | 启用 |\n| 脚注 | 启用 |'
  },
  {
    id: 'math', group: 'extended', title: '数学公式', support: 'limited',
    summary: '使用 $…$ 写行内公式、$$…$$ 或 math 围栏写块公式；预览、HTML/PDF 和 DOCX 共用受限子集。',
    example: '勾股定理：$a^2+b^2=c^2$\n\n$$\\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}$$',
    note: '稳定子集包含字母与数字、上下标、基本运算、\\frac、\\sqrt 和希腊字母。不支持自定义宏、URL/文件命令或完整 TeX；失败时保留可见源码并返回导出警告。价格中的美元符号可写成 \\$。'
  },
  {
    id: 'mermaid', group: 'extended', title: 'Mermaid 图表', support: 'limited',
    summary: '用 mermaid 围栏写离线图表；安装兼容的离线包后可在预览及 HTML、PDF、DOCX 导出中渲染。',
    example: '```mermaid\nflowchart TD\nA[开始]-->B[完成]\n```',
    note: '缺少离线包或图表语法错误时保留可见源码；DOCX 中嵌入图片，不能编辑图表源码。'
  },
  {
    id: 'tables', group: 'extended', title: '表格', support: 'supported',
    summary: '表头下一行用短横线定义列，竖线分隔单元格。',
    example: '| 功能 | 状态 |\n| --- | --- |\n| 预览 | 完成 |\n| 脑图 | 完成 |'
  },
  {
    id: 'table-alignment', group: 'extended', title: '表格对齐', support: 'supported',
    summary: '分隔行左侧、右侧或两侧的冒号分别表示左对齐、右对齐或居中。',
    example: '| 左对齐 | 居中 | 右对齐 |\n| :--- | :---: | ---: |\n| A | B | 100 |'
  },
  {
    id: 'table-formatting-pipes', group: 'extended', title: '表格格式与竖线转义', support: 'supported',
    summary: '单元格可使用行内强调、代码和链接；需要显示竖线时在其前面加反斜杠。',
    example: '| 名称 | 示例 |\n| --- | --- |\n| **强调** | `A \\| B` |'
  },
  {
    id: 'fenced-code', group: 'extended', title: '围栏代码块', support: 'supported',
    summary: '三个或更多反引号或波浪线包围多行代码，不需要逐行缩进。',
    example: '~~~rust\nfn main() {\n    println!("MarkLite");\n}\n~~~'
  },
  {
    id: 'code-info-string', group: 'extended', title: '代码语言标记', support: 'limited',
    summary: '围栏开始处可写语言名，MarkLite 会保留语言 class。',
    example: '```typescript\nconst ready: boolean = true;\n```',
    note: '当前预览没有内置按语言着色的高亮引擎。'
  },
  {
    id: 'footnotes', group: 'extended', title: '脚注', support: 'supported',
    summary: '正文用 [^id] 引用脚注，在其他位置用 [^id]: 定义说明。',
    example: 'MarkLite 是轻量编辑器。[^about]\n\n[^about]: 支持实时预览和多格式导出。'
  },
  {
    id: 'heading-ids', group: 'extended', title: '标题 ID', support: 'supported',
    summary: '标题末尾可指定 {#id}；未指定时 MarkLite 会生成稳定且不重复的预览锚点。',
    example: '## 安装说明 {#install}'
  },
  {
    id: 'heading-id-links', group: 'extended', title: '链接到标题', support: 'supported',
    summary: '使用 # 加标题 ID 跳转到当前预览内的对应标题。',
    example: '[跳到安装说明](#install)\n\n## 安装说明 {#install}'
  },
  {
    id: 'definition-lists', group: 'extended', title: '定义列表', support: 'supported',
    summary: '用“术语 + 冒号定义”创建定义列表；这是 MarkLite 扩展，不属于 CommonMark 或基础 GFM。',
    example: 'MarkLite\n: 轻量 Markdown 编辑器'
  },
  {
    id: 'github-alerts', group: 'extended', title: 'GitHub Alerts', support: 'supported',
    summary: '用 [!NOTE]、[!TIP]、[!IMPORTANT]、[!WARNING] 或 [!CAUTION] 标记提示块。',
    example: '> [!NOTE]\n> 这是需要留意的信息。',
    note: '这是显式启用的 GitHub 扩展，不属于 CommonMark 或基础 GFM。'
  },
  {
    id: 'strikethrough', group: 'extended', title: '删除线', support: 'supported',
    summary: '用两个波浪线包围不再适用的内容。',
    example: '~~旧方案~~ 新方案'
  },
  {
    id: 'task-lists', group: 'extended', title: '任务列表', support: 'supported',
    summary: '列表标记后使用 [ ] 或 [x] 表示未完成和已完成任务。',
    example: '- [x] 明确需求\n- [ ] 完成验收'
  },
  {
    id: 'unicode-emoji', group: 'extended', title: '直接使用 Emoji', support: 'supported',
    summary: '可以直接输入和保存 Unicode Emoji，显示效果由系统字体决定。',
    example: '完成 ✅  注意 ⚠️  灵感 💡'
  },
  {
    id: 'emoji-shortcodes', group: 'extended', title: 'Emoji 短码', support: 'unsupported',
    summary: ':smile:、:warning: 一类短码不会被 MarkLite 自动替换。',
    example: ':smile: 仍按普通文字显示。'
  },
  {
    id: 'bare-url-autolinks', group: 'extended', title: '裸网址自动链接', support: 'supported',
    summary: 'GFM 裸 URL、www 地址和 email 会自动转换为受控链接。',
    example: 'https://docs.example.invalid/guide',
    note: '代码片段和已有链接内部不会再次自动链接；email 只交给受控 mailto opener。'
  },
  {
    id: 'smart-punctuation', group: 'extended', title: '智能标点', support: 'unsupported',
    summary: 'MarkLite 不自动改写省略号、连续短横线或直引号，以保持 CommonMark/GFM 文本语义。',
    example: '等待...  范围 1--5  "引用文字"'
  },
  {
    id: 'highlight-syntax', group: 'extended', title: '双等号高亮标记', support: 'unsupported',
    summary: '==文字== 会按原文显示，不会生成高亮。', example: '==标记=='
  },
  {
    id: 'subscript-superscript', group: 'extended', title: '上下标简写', support: 'unsupported',
    summary: 'H~2~O 与 x^2^ 不会作为上下标解析；公式请使用受限数学语法。', example: 'H~2~O 与 x^2^'
  },
  {
    id: 'front-matter', group: 'extended', title: 'YAML 头部', support: 'unsupported',
    summary: '文件开头的 --- 不会成为元数据，仍按普通 Markdown 解析。', example: '---\ntitle: 示例\n---\n正文'
  },
  {
    id: 'document-toc', group: 'extended', title: '文内目录标记', support: 'unsupported',
    summary: '[TOC] 不会插入文内目录；侧栏可查看标题大纲。', example: '[TOC]\n\n# 标题'
  },
  {
    id: 'mind-map-headings', group: 'mindMap', title: '用标题生成脑图', support: 'supported',
    summary: '脑图把文档名作为根节点，把 #–###### 标题按最近的上级标题组织成分支。',
    example: '# 产品规划\n\n## 用户价值\n\n### 快速打开\n\n## 发布计划'
  },
  {
    id: 'mind-map-hierarchy', group: 'mindMap', title: '层级与跳级标题', support: 'supported',
    summary: '标题跳级时会挂到最近可用的上级节点；同级标题保持文档中的先后顺序。',
    example: '# 根主题\n\n### 跳级节点\n\n## 新分支\n\n### 子节点'
  },
  {
    id: 'mind-map-editing', group: 'mindMap', title: '编辑与节点交互', support: 'supported',
    summary: '节点只用于浏览、展开、折叠和跳转；增删改标题始终在编辑栏完成。',
    example: '把 `## 发布计划` 改为 `## 版本路线`，脑图会随下一次预览同步。'
  }
] as const satisfies readonly MarkdownGuideTopic[];

type MarkdownGuideTopicId = (typeof zhCnMarkdownGuideTopics)[number]['id'];
type LocalizedTopicText = Pick<MarkdownGuideTopic, 'title' | 'summary' | 'example' | 'note'>;

const enTopicText: Record<MarkdownGuideTopicId, LocalizedTopicText> = {
  'atx-headings': { title: 'ATX headings', summary: 'Start a line with one to six # characters followed by a space to create heading levels 1–6.', example: '# Heading 1\n\n## Heading 2\n\n###### Heading 6' },
  'setext-headings': { title: 'Setext headings', summary: 'Underline text with equals signs or hyphens to create a level-one or level-two heading.', example: 'Heading 1\n=========\n\nHeading 2\n---------' },
  paragraphs: { title: 'Paragraphs', summary: 'Continuous text belongs to one paragraph. Use a blank line to start a new paragraph; indentation is unnecessary.', example: 'This is the first paragraph. A sentence may wrap naturally.\nIt still belongs to the first paragraph.\n\nThis is the second paragraph.' },
  'line-breaks': { title: 'Hard line breaks', summary: 'End a line with two spaces or a backslash to create an explicit break inside one paragraph.', example: 'The first line ends with two spaces.  \nSecond line.\n\nThe third line ends with a backslash.\\\nFourth line.' },
  bold: { title: 'Bold', summary: 'Wrap text with two asterisks or two underscores.', example: '**Important conclusion**\n\n__Also bold__' },
  italic: { title: 'Italic', summary: 'Wrap text with one asterisk or underscore. Prefer asterisks inside words to reduce ambiguity.', example: '*Needs attention*\n\n_Italic note_' },
  'bold-italic': { title: 'Bold and italic', summary: 'Use three asterisks or underscores to combine bold and italic.', example: '***Highest priority***\n\n___Bold and italic___' },
  blockquotes: { title: 'Block quotes', summary: 'Prefix each paragraph line with >. Prefix blank lines as well when keeping multiple paragraphs in one quote.', example: '> First quoted paragraph.\n>\n> Second quoted paragraph.' },
  'nested-blockquotes': { title: 'Nested block quotes', summary: 'Add more > characters for deeper levels. Quotes may also contain emphasis, lists, and links.', example: '> Outer idea\n>\n> > Nested detail\n>\n> - A list inside the quote' },
  'ordered-lists': { title: 'Ordered lists', summary: 'Use a number followed by a period. DOCX export preserves the starting number.', example: '1. Prepare material\n2. Write content\n3. Validate the result' },
  'unordered-lists': { title: 'Unordered lists', summary: 'Use a hyphen, asterisk, or plus sign. Keep one marker style at the same level for clarity.', example: '- Edit\n- Preview\n- Export' },
  'nested-list-content': { title: 'Nested lists and block content', summary: 'Indent child lists. Further indentation can add paragraphs, quotes, code blocks, or images to a list item.', example: '1. Main task\n   - Subtask A\n   - Subtask B\n\n     > Subtask note' },
  'inline-code': { title: 'Inline code', summary: 'Wrap commands, paths, or identifiers in one backtick. Markdown inside is not parsed again.', example: 'Run `npm run check`, then inspect `src/main.ts`.' },
  'escaped-backticks': { title: 'Backticks inside code', summary: 'When the content includes a backtick, use two backticks as the outer delimiter.', example: '``Write `code` in Markdown``' },
  'indented-code-blocks': { title: 'Indented code blocks', summary: 'Indent every line by four spaces. Fenced code blocks are usually clearer for complex code.', example: 'Normal paragraph\n\n    const ready = true;\n    console.log(ready);' },
  'horizontal-rules': { title: 'Horizontal rules', summary: 'Use three or more hyphens, asterisks, or underscores on their own line.', example: 'Upper section\n\n---\n\nLower section' },
  'inline-links': { title: 'Inline links', summary: 'Put link text in brackets and the target in parentheses. Local documents resolve relative to the saved source document.', example: '[Project overview](./README.md)' },
  'link-titles': { title: 'Link titles', summary: 'Add a quoted title after the target to show extra information on hover.', example: '[Project overview](./README.md "Open the project overview")' },
  'angle-autolinks': { title: 'Angle-bracket links', summary: 'Angle brackets explicitly turn a complete URL or email address into a link.', example: '<https://docs.example.invalid/guide>\n\n<team@example.invalid>', note: 'The example uses reserved domains and is not an external MarkLite help link.' },
  'formatted-links': { title: 'Formatted link text', summary: 'Link text may contain bold, italic, or code formatting.', example: '[**Important**](./IMPORTANT.md)\n\n[`Configuration`](./settings.md)' },
  'reference-links': { title: 'Reference links', summary: 'Keep compact labels in the body and define destinations elsewhere for reuse and cleaner paragraphs.', example: 'Read the [installation guide][install] and [installation guide][].\n\n[install]: ./INSTALL.md "Install"\n[installation guide]: ./INSTALL.md' },
  images: { title: 'Images', summary: 'Prefix link syntax with !. Relative images resolve from the saved document directory.', example: '![Interface screenshot](./images/screenshot.png "MarkLite interface")', note: 'Enable local images in Settings. Preview does not download remote images automatically.' },
  'linked-images': { title: 'Linked images', summary: 'Place image syntax where link text normally goes to make the image clickable.', example: '[![Project icon](./images/logo.png)](./README.md)' },
  'escaping-characters': { title: 'Escaping characters', summary: 'Prefix Markdown punctuation with a backslash to display it as a normal character.', example: '\\*This is not italic\\*\n\n\\# This is not a heading\n\n\\[This is not a link\\]' },
  'inline-html': { title: 'Inline HTML', summary: 'Markdown may contain inline HTML, but MarkLite sanitizes it before display.', example: 'This is <mark>highlighted</mark> and this is <kbd>Ctrl</kbd>.', note: 'Only allowed tags and attributes remain. Scripts and event attributes never execute.' },
  'block-html': { title: 'Block HTML', summary: 'The parser accepts block HTML, but sanitization and nested Markdown behavior follow the security boundary.', example: '<div class="notice">\n  Raw HTML content\n</div>', note: 'Do not depend on arbitrary HTML, styles, iframes, or scripts for document behavior.' },
  'extension-profile': { title: 'MarkLite extension profile', summary: 'MarkLite uses GFM as its base and explicitly adds footnotes, heading attributes, GitHub alerts, definition lists, and bounded math. Ordinary punctuation remains unchanged.', example: '| Feature | Status |\n| --- | --- |\n| Tables | Enabled |\n| Footnotes | Enabled |' },
  math: { title: 'Mathematical expressions', summary: 'Use $…$ for inline math and $$…$$ or a math fence for display math. Preview, HTML/PDF, and DOCX share a bounded subset.', example: 'Pythagoras: $a^2+b^2=c^2$\n\n$$\\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}$$', note: 'The stable subset covers letters and numbers, scripts, basic operators, \\frac, \\sqrt, and Greek letters. User macros, URL/file commands, and complete TeX are unsupported. Failures preserve visible source and return export warnings. Escape currency dollars as \\$.' },
  mermaid: { title: 'Mermaid diagrams', summary: 'Write an offline diagram in a mermaid fence. Preview and HTML, PDF, DOCX export render it after a compatible offline pack is installed.', example: '```mermaid\nflowchart TD\nA[Start]-->B[Done]\n```', note: 'Missing pack or invalid syntax preserves visible source. DOCX embeds an image, not editable diagram source.' },
  tables: { title: 'Tables', summary: 'Use a hyphen separator below the header row and vertical bars between cells.', example: '| Feature | Status |\n| --- | --- |\n| Preview | Complete |\n| Mind map | Complete |' },
  'table-alignment': { title: 'Table alignment', summary: 'Colons on the left, right, or both sides of the separator select left, right, or centered alignment.', example: '| Left | Center | Right |\n| :--- | :---: | ---: |\n| A | B | 100 |' },
  'table-formatting-pipes': { title: 'Table formatting and escaped pipes', summary: 'Cells support inline emphasis, code, and links. Escape a literal vertical bar with a backslash.', example: '| Name | Example |\n| --- | --- |\n| **Emphasis** | `A \\| B` |' },
  'fenced-code': { title: 'Fenced code blocks', summary: 'Surround multiple lines with three or more backticks or tildes without indenting every line.', example: '~~~rust\nfn main() {\n    println!("MarkLite");\n}\n~~~' },
  'code-info-string': { title: 'Code language labels', summary: 'Add a language after the opening fence. MarkLite preserves it as a language class.', example: '```typescript\nconst ready: boolean = true;\n```', note: 'The preview currently has no built-in language-aware syntax highlighter.' },
  footnotes: { title: 'Footnotes', summary: 'Reference a footnote with [^id] and define it elsewhere with [^id]:.', example: 'MarkLite is a lightweight editor.[^about]\n\n[^about]: It supports live preview and multi-format export.' },
  'heading-ids': { title: 'Heading IDs', summary: 'Add {#id} at the end of a heading. MarkLite generates stable unique anchors when no ID is supplied.', example: '## Installation {#install}' },
  'heading-id-links': { title: 'Links to headings', summary: 'Use # plus a heading ID to jump within the current preview.', example: '[Jump to installation](#install)\n\n## Installation {#install}' },
  'definition-lists': { title: 'Definition lists', summary: 'A term followed by a colon definition creates a definition list. This is a MarkLite extension, not CommonMark or base GFM.', example: 'MarkLite\n: Lightweight Markdown editor' },
  'github-alerts': { title: 'GitHub Alerts', summary: 'Use [!NOTE], [!TIP], [!IMPORTANT], [!WARNING], or [!CAUTION] to create an alert block.', example: '> [!NOTE]\n> Information worth noticing.', note: 'This explicitly enabled GitHub extension is not part of CommonMark or base GFM.' },
  strikethrough: { title: 'Strikethrough', summary: 'Wrap obsolete content with two tildes.', example: '~~Old plan~~ New plan' },
  'task-lists': { title: 'Task lists', summary: 'Use [ ] or [x] after a list marker for incomplete and complete tasks.', example: '- [x] Confirm requirements\n- [ ] Complete validation' },
  'unicode-emoji': { title: 'Unicode Emoji', summary: 'Enter and save Unicode Emoji directly. Appearance depends on the system font.', example: 'Complete ✅  Caution ⚠️  Idea 💡' },
  'emoji-shortcodes': { title: 'Emoji shortcodes', summary: 'MarkLite does not automatically replace shortcodes such as :smile: or :warning:.', example: ':smile: remains plain text.' },
  'bare-url-autolinks': { title: 'Bare URL autolinks', summary: 'GFM bare URLs, www addresses, and email addresses are converted to controlled links.', example: 'https://docs.example.invalid/guide', note: 'Code spans and existing links are not linked again. Email uses the controlled mailto opener.' },
  'smart-punctuation': { title: 'Smart punctuation', summary: 'MarkLite does not rewrite ellipses, repeated hyphens, or straight quotes, preserving CommonMark/GFM text semantics.', example: 'Wait...  Range 1--5  "Quoted text"' },
  'highlight-syntax': { title: 'Double equals markers', summary: '==text== stays visible source rather than becoming highlighted text.', example: '==Marked==' },
  'subscript-superscript': { title: 'Subscript and superscript shorthand', summary: 'H~2~O and x^2^ stay source text. Use the bounded math syntax for formulas.', example: 'H~2~O and x^2^' },
  'front-matter': { title: 'YAML front matter', summary: 'Leading --- does not create metadata. It follows ordinary Markdown rules.', example: '---\ntitle: Example\n---\nBody' },
  'document-toc': { title: 'In-document table of contents', summary: '[TOC] does not insert a document block. The sidebar shows the heading outline.', example: '[TOC]\n\n# Heading' },
  'mind-map-headings': { title: 'Build a mind map from headings', summary: 'The document name becomes the root. Headings from # through ###### form branches under the nearest parent heading.', example: '# Product plan\n\n## User value\n\n### Fast opening\n\n## Release plan' },
  'mind-map-hierarchy': { title: 'Hierarchy and skipped levels', summary: 'A skipped heading level attaches to the nearest available parent. Siblings keep document order.', example: '# Root topic\n\n### Skipped-level node\n\n## New branch\n\n### Child node' },
  'mind-map-editing': { title: 'Editing and node interaction', summary: 'Nodes are for browsing, expanding, collapsing, and jumping. Add, remove, or edit headings only in the editor.', example: 'Change `## Release plan` to `## Version roadmap`; the mind map updates with the next preview.' }
};

const enMarkdownGuideGroups = [
  { id: 'basic', label: 'Basic syntax', description: 'Headings, paragraphs, formatting, links, and images' },
  { id: 'extended', label: 'Extended syntax', description: 'Tables, footnotes, tasks, and compatibility' },
  { id: 'mindMap', label: 'Mind map', description: 'Build an interactive mind map from heading levels' }
] satisfies typeof zhCnMarkdownGuideGroups;

const enMarkdownGuideTopics: readonly MarkdownGuideTopic[] = zhCnMarkdownGuideTopics.map((topic) => ({
  id: topic.id,
  group: topic.group,
  support: topic.support,
  ...enTopicText[topic.id]
}));

export const namedExtensionGuideTopics = {
  footnotes: 'footnotes',
  'definition-lists': 'definition-lists',
  alerts: 'github-alerts',
  'heading-attributes': 'heading-ids',
  'bounded-math': 'math',
  mermaid: 'mermaid',
  'unicode-emoji': 'unicode-emoji',
  'emoji-shortcodes': 'emoji-shortcodes',
  'highlight-syntax': 'highlight-syntax',
  'subscript-superscript': 'subscript-superscript',
  'front-matter': 'front-matter',
  'document-toc': 'document-toc'
} as const;

const namedByTopic = new Map<string, (typeof capabilityContract.namedExtensions)[number]>(capabilityContract.namedExtensions.map((extension) => [
  namedExtensionGuideTopics[extension.id as keyof typeof namedExtensionGuideTopics], extension
]));

function withCapability(topic: MarkdownGuideTopic): MarkdownGuideTopic {
  const extension = namedByTopic.get(topic.id);
  if (!extension) return { ...topic, fixture: topic.example };
  const support: MarkdownGuideSupport = extension.statuses.preview === 'source' ? 'unsupported'
    : extension.statuses.preview === 'conditional' ? 'limited' : 'supported';
  return { ...topic, support, fixture: extension.fixture };
}

export function markdownGuideGroupsFor(language: AppLanguage = getLanguage()) {
  return language === 'zh-CN' ? zhCnMarkdownGuideGroups : enMarkdownGuideGroups;
}

export function markdownGuideTopicsFor(language: AppLanguage = getLanguage()): readonly MarkdownGuideTopic[] {
  return (language === 'zh-CN' ? zhCnMarkdownGuideTopics : enMarkdownGuideTopics).map(withCapability);
}

export const markdownGuideGroups = enMarkdownGuideGroups;
export const markdownGuideTopics = enMarkdownGuideTopics.map(withCapability);

export function guideSupportLabel(
  support: MarkdownGuideSupport,
  language: AppLanguage = getLanguage()
): string {
  return translate(language, `guide.support.${support}`);
}

export function filterMarkdownGuide(
  group: MarkdownGuideGroup,
  query: string,
  language: AppLanguage = getLanguage()
): MarkdownGuideTopic[] {
  const normalized = query.trim().toLocaleLowerCase();
  return markdownGuideTopicsFor(language).filter((topic) => {
    if (topic.group !== group) return false;
    if (!normalized) return true;
    return `${topic.title} ${topic.summary} ${topic.example} ${topic.note ?? ''}`
      .toLocaleLowerCase()
      .includes(normalized);
  });
}
