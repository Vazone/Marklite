import type { OutlineItem } from './tauriApi';
import { t } from './i18n';
import {
  buildMindMap,
  estimateMindMapNodeSize,
  layoutMindMap,
  MIND_MAP_LABEL_FONT_SIZE,
  MIND_MAP_LABEL_LINE_HEIGHT,
  MIND_MAP_META_FONT_SIZE,
  MIND_MAP_META_GAP,
  MIND_MAP_META_LINE_HEIGHT,
  MIND_MAP_NODE_TEXT_CHROME,
  wrapMindMapLabel,
  type MindMapNode,
  type MindMapNodeSize
} from './mindMap';

const NODE_TEXT_LEFT = 16;
export const MIND_MAP_SVG_MAX_HEADINGS = 20_000;
export const MIND_MAP_SVG_MAX_BYTES = 16 * 1024 * 1024;

export class MindMapSvgExportError extends Error {
  readonly code = 'MIND_MAP_SVG_TOO_LARGE';

  constructor() {
    super('MIND_MAP_SVG_TOO_LARGE');
    this.name = 'MindMapSvgExportError';
  }
}

const LEVEL_COLORS = [
  { stroke: '#159b9b', fill: '#eefafa' },
  { stroke: '#2276cf', fill: '#eef6ff' },
  { stroke: '#8655c5', fill: '#f7f1ff' },
  { stroke: '#c05991', fill: '#fff1f7' },
  { stroke: '#c26827', fill: '#fff6ec' },
  { stroke: '#488b37', fill: '#f2faef' },
  { stroke: '#177f87', fill: '#edfafa' }
] as const;

export function serializeMindMapSvg(
  documentTitle: string,
  outline: readonly OutlineItem[]
): string {
  if (outline.length > MIND_MAP_SVG_MAX_HEADINGS) throw new MindMapSvgExportError();
  const tree = buildMindMap(documentTitle, outline);
  const labels = new Map<string, string[]>();
  const nodeSizes = new Map<string, MindMapNodeSize>();
  for (const node of flattenTree(tree)) {
    const size = estimateMindMapNodeSize(node);
    nodeSizes.set(node.id, size);
    labels.set(node.id, wrapMindMapLabel(node.label, size.width - MIND_MAP_NODE_TEXT_CHROME));
  }

  const layout = layoutMindMap(tree, new Set(), nodeSizes);
  const connectorMarkup = layout.connectors.map((connector) => {
    const color = levelColor(connector.level).stroke;
    return `    <path d="${connector.path}" fill="none" stroke="${color}" stroke-width="2" stroke-linecap="round" opacity="0.76"/>`;
  });
  const nodeMarkup = layout.nodes.flatMap((node) => {
    const colors = levelColor(node.level);
    const textX = node.x + NODE_TEXT_LEFT;
    const nodeLines = labels.get(node.id) ?? [node.label];
    const contentHeight = nodeLines.length * MIND_MAP_LABEL_LINE_HEIGHT +
      (node.line === null ? 0 : MIND_MAP_META_GAP + MIND_MAP_META_LINE_HEIGHT);
    const firstBaseline = node.y + (node.height - contentHeight) / 2 + MIND_MAP_LABEL_FONT_SIZE;
    const labelMarkup = nodeLines.map((line, index) =>
      `      <tspan x="${textX}" y="${formatNumber(firstBaseline + index * MIND_MAP_LABEL_LINE_HEIGHT)}">${escapeXml(line)}</tspan>`
    );
    const metadataMarkup = node.line === null ? [] : [
      `    <text x="${textX}" y="${formatNumber(firstBaseline + nodeLines.length * MIND_MAP_LABEL_LINE_HEIGHT + MIND_MAP_META_GAP)}" fill="#687386" font-size="${MIND_MAP_META_FONT_SIZE}">${escapeXml(t('common.line', { line: node.line }))}</text>`
    ];
    return [
      `    <g data-node-level="${node.level}">`,
      `      <rect x="${node.x}" y="${formatNumber(node.y)}" width="${node.width}" height="${node.height}" rx="9" fill="${colors.fill}" stroke="${colors.stroke}"/>`,
      `      <rect x="${node.x}" y="${formatNumber(node.y)}" width="6" height="${node.height}" rx="3" fill="${colors.stroke}"/>`,
      `      <text xml:space="preserve" fill="#172033" font-size="${MIND_MAP_LABEL_FONT_SIZE}" font-weight="${node.level === 0 ? 700 : 500}">`,
      ...labelMarkup,
      '      </text>',
      ...metadataMarkup,
      '    </g>'
    ];
  });

  const svg = [
    '<?xml version="1.0" encoding="UTF-8"?>',
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${layout.width} ${layout.height}" width="${layout.width}" height="${layout.height}" data-marklite-mind-map="1">`,
    `  <title>${escapeXml(t('mindMap.svgTitle', { title: documentTitle.trim() || t('mindMap.untitled') }))}</title>`,
    `  <rect width="${layout.width}" height="${layout.height}" fill="#ffffff"/>`,
    `  <g aria-label="${escapeXml(t('mindMap.connectors'))}">`,
    ...connectorMarkup,
    '  </g>',
    `  <g aria-label="${escapeXml(t('mindMap.nodeCount', { count: outline.length }))}">`,
    ...nodeMarkup,
    '  </g>',
    '</svg>',
    ''
  ].join('\n');
  if (new TextEncoder().encode(svg).byteLength > MIND_MAP_SVG_MAX_BYTES) {
    throw new MindMapSvgExportError();
  }
  return svg;
}

function flattenTree(root: MindMapNode): MindMapNode[] {
  const nodes: MindMapNode[] = [];
  const stack = [root];
  while (stack.length) {
    const node = stack.pop()!;
    nodes.push(node);
    for (let index = node.children.length - 1; index >= 0; index -= 1) {
      stack.push(node.children[index]);
    }
  }
  return nodes;
}

function levelColor(level: number) {
  return LEVEL_COLORS[Math.min(LEVEL_COLORS.length - 1, Math.max(0, level))];
}

function escapeXml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
}

function formatNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(2).replace(/\.0+$/, '');
}
