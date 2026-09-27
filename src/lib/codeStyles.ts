import palette from '../shared/code-palette.json';

export function codeHighlightCss(): string {
  return palette.map(({ classes, light, dark }) => {
    const selectors = classes.map(name => `:is(pre, .cm-content) .${name}`);
    return `${selectors.join(',')} { color: #${light}; }\n` +
      `${selectors.map(selector => `[data-theme="dark"] ${selector}`).join(',')} { color: #${dark}; }`;
  }).join('\n');
}

export function installCodeStyles() {
  const style = document.createElement('style');
  style.textContent = codeHighlightCss();
  document.head.append(style);
}
