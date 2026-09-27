/** @param {string} svgMarkup @returns {Element} */
export function parseMermaidSvgDocument(svgMarkup) {
  // Mermaid 11.17.2 serializes an HTML label's line break as <br> inside
  // foreignObject. XML parsing needs the void tag closed before SVG policy runs.
  const xml = svgMarkup.replaceAll('<br>', '<br/>');
  const root = new DOMParser().parseFromString(xml, 'image/svg+xml').documentElement;
  if (root.localName !== 'svg' || root.namespaceURI !== 'http://www.w3.org/2000/svg') {
    throw new Error('Mermaid returned invalid SVG XML');
  }
  return root;
}

/** @param {Element} root @returns {Element} */
export function normalizeMermaidSvgDocument(root) {
  const svgNamespace = 'http://www.w3.org/2000/svg';
  // The fixed 11.17.2 info renderer declares a 400 x 100 canvas internally,
  // but configureSvgSize omits both viewBox and height when useMaxWidth is true.
  if (!root.hasAttribute('viewBox') && root.getAttribute('aria-roledescription') === 'info') {
    root.setAttribute('viewBox', '0 0 400 100');
  }
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
    const labelNode = /** @type {Element} */ (foreignObject.cloneNode(true));
    for (const lineBreak of [...labelNode.querySelectorAll('br')]) {
      lineBreak.replaceWith(root.ownerDocument.createTextNode('\n'));
    }
    const label = labelNode.textContent?.replace(/[^\S\n]+/g, ' ').replace(/ *\n */g, '\n').trim() ?? '';
    const fontSize = 16;
    const lineHeight = fontSize * 1.5;
    const context = document.createElement('canvas').getContext('2d');
    if (context) context.font = `${fontSize}px Arial, system-ui, sans-serif`;
    const lines = [];
    let current = '';
    for (const character of label) {
      if (character === '\n') {
        lines.push(current.trim());
        current = '';
        continue;
      }
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
