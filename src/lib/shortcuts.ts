export type Shortcut = {
  keys: string;
  action: string;
};

export const shortcuts: Shortcut[] = [
  { keys: 'Ctrl + N', action: '新建文件' },
  { keys: 'Ctrl + O', action: '打开文件' },
  { keys: 'Ctrl + S', action: '保存文件' },
  { keys: 'Ctrl + Shift + S', action: '另存为' },
  { keys: 'Ctrl + F', action: '查找' },
  { keys: 'Ctrl + H', action: '替换' },
  { keys: 'Ctrl + B', action: '加粗' },
  { keys: 'Ctrl + I', action: '斜体' },
  { keys: 'Ctrl + K', action: '插入链接' },
  { keys: 'Ctrl + E', action: '切换预览' },
  { keys: 'Ctrl + ,', action: '设置' },
  { keys: 'Ctrl + P', action: '命令面板' },
  { keys: 'Ctrl + W', action: '关闭当前文件' }
];
