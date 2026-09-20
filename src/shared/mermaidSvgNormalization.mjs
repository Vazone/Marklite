/** @param {Element} root @returns {Element} */
export function normalizeMermaidSvgDocument(root) {
  const svgNamespace = 'http://www.w3.org/2000/svg';
  for (const foreignObject of [...root.querySelectorAll('foreignObject')]) {
    if (foreignObject.querySelector('.katex, math')) continue;
    const parent = foreignObject.parentElement;
    const fallback = parent?.localName === 'switch'
      ? [...parent.children].find(
          (child) => child !== foreignObject && child.namespaceURI === svgNamespace && child.localName === 'text'
        )
      : null;
    if (fallback) {
      foreignObject.remove();
      continue;
    }
    const text = root.ownerDocument.createElementNS(svgNamespace, 'text');
    const x = Number(foreignObject.getAttribute('x') ?? 0);
    const y = Number(foreignObject.getAttribute('y') ?? 0);
    const width = Number(foreignObject.getAttribute('width') ?? 0);
    const height = Number(foreignObject.getAttribute('height') ?? 0);
    text.setAttribute('x', String(Number.isFinite(x + width / 2) ? x + width / 2 : 0));
    text.setAttribute('y', String(Number.isFinite(y + height / 2) ? y + height / 2 : 0));
    text.setAttribute('text-anchor', 'middle');
    text.setAttribute('dominant-baseline', 'middle');
    const label = foreignObject.textContent?.replace(/\s+/g, ' ').trim() ?? '';
    const fontSize = 16;
    const lineHeight = fontSize * 1.5;
    const context = document.createElement('canvas').getContext('2d');
    if (context) context.font = `${fontSize}px Arial, system-ui, sans-serif`;
    const lines = [];
    let current = '';
    for (const character of label) {
      const candidate = current + character;
      const measured = context?.measureText(candidate).width ?? candidate.length * fontSize;
      if (current && width > 0 && measured > width + 8) {
        lines.push(current.trim());
        current = character.trimStart();
      } else {
        current = candidate;
      }
    }
    if (current || lines.length === 0) lines.push(current.trim());
    const firstY = (Number.isFinite(y + height / 2) ? y + height / 2 : 0) - ((lines.length - 1) * lineHeight) / 2;
    text.setAttribute('y', String(firstY));
    text.setAttribute('font-size', String(fontSize));
    for (let index = 0; index < lines.length; index++) {
      const tspan = root.ownerDocument.createElementNS(svgNamespace, 'tspan');
      tspan.setAttribute('x', text.getAttribute('x') ?? '0');
      if (index > 0) tspan.setAttribute('dy', String(lineHeight));
      tspan.textContent = lines[index];
      text.append(tspan);
    }
    foreignObject.replaceWith(text);
  }
  for (const switchElement of [...root.querySelectorAll('switch')]) {
    if (switchElement.querySelector('.katex, math')) continue;
    const children = [...switchElement.children].filter((child) => child.namespaceURI === svgNamespace);
    switchElement.replaceWith(...children);
  }
  return root;
}
