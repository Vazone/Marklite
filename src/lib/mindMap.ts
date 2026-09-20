import type { OutlineItem } from './tauriApi';
import { t } from './i18n';

export type MindMapNode = {
  id: string;
  label: string;
  level: number;
  line: number | null;
  children: MindMapNode[];
};

export type MindMapNodeSize = {
  width: number;
  height: number;
};

export type MindMapLayoutNode = MindMapNode & MindMapNodeSize & {
  x: number;
  y: number;
  depth: number;
  hasChildren: boolean;
  collapsed: boolean;
};

export type MindMapConnector = {
  id: string;
  sourceId: string;
  targetId: string;
  path: string;
  level: number;
};

export type MindMapLayout = {
  nodes: MindMapLayoutNode[];
  connectors: MindMapConnector[];
  width: number;
  height: number;
};

export type MindMapViewport = {
  left: number;
  top: number;
  width: number;
  height: number;
};

export type MindMapViewportIndex = {
  layout: MindMapLayout;
  columns: Array<{ nodes: MindMapLayoutNode[]; maxHeight: number }>;
  connectorByTargetId: ReadonlyMap<string, MindMapConnector>;
};

export type MindMapViewportProjection = {
  nodes: MindMapLayoutNode[];
  connectors: MindMapConnector[];
};

export type MindMapLayoutOptions = {
  maxNodes?: number;
  shouldCancel?: () => boolean;
};

export type MindMapLayoutErrorCode =
  | 'MIND_MAP_LAYOUT_TOO_LARGE'
  | 'MIND_MAP_LAYOUT_CANCELLED';

export class MindMapLayoutError extends Error {
  readonly code: MindMapLayoutErrorCode;

  constructor(code: MindMapLayoutErrorCode) {
    super(code);
    this.name = 'MindMapLayoutError';
    this.code = code;
  }
}

export const MIND_MAP_NODE_MIN_WIDTH = 196;
export const MIND_MAP_NODE_MAX_WIDTH = 360;
export const MIND_MAP_NODE_MIN_HEIGHT = 60;
export const MIND_MAP_NODE_TEXT_CHROME = 50;
export const MIND_MAP_LABEL_FONT_SIZE = 16;
export const MIND_MAP_LABEL_LINE_HEIGHT = 19;
export const MIND_MAP_META_FONT_SIZE = 11;
export const MIND_MAP_META_LINE_HEIGHT = 14;
export const MIND_MAP_META_GAP = 3;
export const MIND_MAP_NODE_VERTICAL_PADDING = 18;
export const MIND_MAP_MAX_HEADINGS = 200_000;
export const MIND_MAP_MAX_LAYOUT_NODES = MIND_MAP_MAX_HEADINGS + 1;
export const MIND_MAP_VIEWPORT_OVERSCAN = 320;

const COLUMN_GAP = 72;
const ROW_GAP = 30;
const CANVAS_PADDING = 32;

export function buildMindMap(documentTitle: string, outline: readonly OutlineItem[]): MindMapNode {
  const root: MindMapNode = {
    id: 'document-root',
    label: documentTitle.trim() || t('mindMap.untitled'),
    level: 0,
    line: null,
    children: []
  };
  const ancestors: Array<{ headingLevel: number; node: MindMapNode }> = [
    { headingLevel: 0, node: root }
  ];

  outline.forEach((item, index) => {
    const level = Math.min(6, Math.max(1, Math.trunc(item.level)));
    while (ancestors.length > 1 && ancestors.at(-1)!.headingLevel >= level) {
      ancestors.pop();
    }
    const node: MindMapNode = {
      id: `heading-${item.line}-${item.slug || 'section'}-${index}`,
      label: item.title.trim() || t('mindMap.untitledSection'),
      level,
      line: item.line,
      children: []
    };
    ancestors.at(-1)!.node.children.push(node);
    ancestors.push({ headingLevel: level, node });
  });

  return root;
}

export function collectCollapsibleNodeIds(root: MindMapNode): string[] {
  const ids: string[] = [];
  const stack = [root];
  while (stack.length) {
    const node = stack.pop()!;
    if (node.children.length) ids.push(node.id);
    for (let index = node.children.length - 1; index >= 0; index -= 1) {
      stack.push(node.children[index]);
    }
  }
  return ids;
}

export function approximateMindMapTextWidth(value: string): number {
  let width = 0;
  for (const character of value) width += approximateGlyphWidth(character);
  return width;
}

export function wrapMindMapLabel(label: string, maxWidth: number): string[] {
  const normalized = label.trim() || t('mindMap.untitledSection');
  const availableWidth = Math.max(1, maxWidth);
  const lines: string[] = [];
  let current = '';
  let currentWidth = 0;

  for (const character of normalized) {
    const glyphWidth = approximateGlyphWidth(character);
    if (current && currentWidth + glyphWidth > availableWidth) {
      lines.push(current);
      current = character;
      currentWidth = glyphWidth;
      continue;
    }
    current += character;
    currentWidth += glyphWidth;
  }
  if (current) lines.push(current);
  return lines.length ? lines : [t('mindMap.untitledSection')];
}

export function estimateMindMapNodeSize(node: Pick<MindMapNode, 'label' | 'line'>): MindMapNodeSize {
  const naturalWidth = Math.ceil(
    approximateMindMapTextWidth(node.label.trim() || t('mindMap.untitledSection')) +
      MIND_MAP_NODE_TEXT_CHROME
  );
  const width = Math.min(
    MIND_MAP_NODE_MAX_WIDTH,
    Math.max(MIND_MAP_NODE_MIN_WIDTH, naturalWidth)
  );
  const lines = wrapMindMapLabel(node.label, width - MIND_MAP_NODE_TEXT_CHROME);
  const contentHeight = lines.length * MIND_MAP_LABEL_LINE_HEIGHT +
    (node.line === null ? 0 : MIND_MAP_META_GAP + MIND_MAP_META_LINE_HEIGHT);
  return {
    width,
    height: Math.max(MIND_MAP_NODE_MIN_HEIGHT, MIND_MAP_NODE_VERTICAL_PADDING + contentHeight)
  };
}

export function layoutMindMap(
  root: MindMapNode,
  collapsedIds: ReadonlySet<string>,
  measuredSizes: ReadonlyMap<string, MindMapNodeSize> = new Map(),
  options: MindMapLayoutOptions = {}
): MindMapLayout {
  const positioned = new Map<string, MindMapLayoutNode>();
  const sizes = new Map<string, MindMapNodeSize>();
  const columnWidths: number[] = [];
  const visibleNodes: Array<{ node: MindMapNode; depth: number }> = [];
  const maxNodes = options.maxNodes ?? MIND_MAP_MAX_LAYOUT_NODES;
  let operationCount = 0;
  const checkpoint = () => {
    operationCount += 1;
    if ((operationCount & 2047) === 0 && options.shouldCancel?.()) {
      throw new MindMapLayoutError('MIND_MAP_LAYOUT_CANCELLED');
    }
  };
  const collectStack: Array<{ node: MindMapNode; depth: number }> = [{ node: root, depth: 0 }];
  while (collectStack.length) {
    checkpoint();
    const { node, depth } = collectStack.pop()!;
    if (visibleNodes.length >= maxNodes) {
      throw new MindMapLayoutError('MIND_MAP_LAYOUT_TOO_LARGE');
    }
    const size = normalizeNodeSize(measuredSizes.get(node.id), estimateMindMapNodeSize(node));
    sizes.set(node.id, size);
    columnWidths[depth] = Math.max(columnWidths[depth] ?? 0, size.width);
    visibleNodes.push({ node, depth });
    if (collapsedIds.has(node.id)) continue;
    for (let index = node.children.length - 1; index >= 0; index -= 1) {
      collectStack.push({ node: node.children[index], depth: depth + 1 });
    }
  }
  if (options.shouldCancel?.()) throw new MindMapLayoutError('MIND_MAP_LAYOUT_CANCELLED');

  const columnX = [CANVAS_PADDING];
  for (let depth = 1; depth < columnWidths.length; depth += 1) {
    columnX[depth] = columnX[depth - 1] + columnWidths[depth - 1] + COLUMN_GAP;
  }

  const subtreeHeights = new Map<string, number>();
  for (let index = visibleNodes.length - 1; index >= 0; index -= 1) {
    checkpoint();
    const node = visibleNodes[index].node;
    const size = sizes.get(node.id)!;
    let childrenHeight = 0;
    if (!collapsedIds.has(node.id)) {
      for (const child of node.children) {
        const childHeight = subtreeHeights.get(child.id);
        if (childHeight === undefined) continue;
        if (childrenHeight) childrenHeight += ROW_GAP;
        childrenHeight += childHeight;
      }
    }
    const height = Math.max(size.height, childrenHeight);
    subtreeHeights.set(node.id, height);
  }

  const placeStack: Array<{ node: MindMapNode; depth: number; subtreeTop: number }> = [
    { node: root, depth: 0, subtreeTop: CANVAS_PADDING }
  ];
  while (placeStack.length) {
    checkpoint();
    const { node, depth, subtreeTop } = placeStack.pop()!;
    const size = sizes.get(node.id)!;
    const subtreeHeight = subtreeHeights.get(node.id)!;
    const visibleChildren = collapsedIds.has(node.id) ? [] : node.children;
    positioned.set(node.id, {
      ...node,
      x: columnX[depth],
      y: subtreeTop + (subtreeHeight - size.height) / 2,
      ...size,
      depth,
      hasChildren: node.children.length > 0,
      collapsed: collapsedIds.has(node.id)
    });

    let childrenHeight = 0;
    for (const child of visibleChildren) {
      if (childrenHeight) childrenHeight += ROW_GAP;
      childrenHeight += subtreeHeights.get(child.id)!;
    }
    let childBottom = subtreeTop + (subtreeHeight + childrenHeight) / 2;
    for (let index = visibleChildren.length - 1; index >= 0; index -= 1) {
      const child = visibleChildren[index];
      const childHeight = subtreeHeights.get(child.id)!;
      childBottom -= childHeight;
      placeStack.push({ node: child, depth: depth + 1, subtreeTop: childBottom });
      if (index) childBottom -= ROW_GAP;
    }
  }

  const nodes = [...positioned.values()].sort((left, right) =>
    left.depth === right.depth ? left.y - right.y : left.depth - right.depth
  );
  const connectors: MindMapConnector[] = [];
  for (const parent of nodes) {
    if (parent.collapsed) continue;
    for (const child of parent.children) {
      const target = positioned.get(child.id);
      if (!target) continue;
      const startX = parent.x + parent.width;
      const startY = parent.y + parent.height / 2;
      const endX = target.x;
      const endY = target.y + target.height / 2;
      const controlX = startX + (endX - startX) / 2;
      connectors.push({
        id: `${parent.id}->${target.id}`,
        sourceId: parent.id,
        targetId: target.id,
        path: `M ${startX} ${startY} C ${controlX} ${startY}, ${controlX} ${endY}, ${endX} ${endY}`,
        level: target.level
      });
    }
  }

  let width = 480;
  let height = 240;
  for (const node of nodes) {
    width = Math.max(width, node.x + node.width + CANVAS_PADDING);
    height = Math.max(height, node.y + node.height + CANVAS_PADDING);
  }

  return {
    nodes,
    connectors,
    width,
    height
  };
}

export function createMindMapViewportIndex(layout: MindMapLayout): MindMapViewportIndex {
  const columns: Array<{ nodes: MindMapLayoutNode[]; maxHeight: number }> = [];
  const connectorByTargetId = new Map<string, MindMapConnector>();
  for (const node of layout.nodes) {
    const column = columns[node.depth] ??= { nodes: [], maxHeight: 0 };
    column.nodes.push(node);
    column.maxHeight = Math.max(column.maxHeight, node.height);
  }
  for (const column of columns) column.nodes.sort((left, right) => left.y - right.y);
  for (const connector of layout.connectors) {
    connectorByTargetId.set(connector.targetId, connector);
  }
  return {
    layout,
    columns,
    connectorByTargetId
  };
}

export function projectMindMapViewport(
  index: MindMapViewportIndex,
  viewport: MindMapViewport,
  overscan = MIND_MAP_VIEWPORT_OVERSCAN
): MindMapViewportProjection {
  const left = viewport.left - overscan;
  const top = viewport.top - overscan;
  const right = viewport.left + viewport.width + overscan;
  const bottom = viewport.top + viewport.height + overscan;
  const nodes: MindMapLayoutNode[] = [];
  const connectors: MindMapConnector[] = [];

  for (const column of index.columns) {
    if (!column?.nodes.length) continue;
    const columnNodes = column.nodes;
    let low = 0;
    let high = columnNodes.length;
    const earliestY = top - column.maxHeight;
    while (low < high) {
      const middle = (low + high) >>> 1;
      if (columnNodes[middle].y < earliestY) low = middle + 1;
      else high = middle;
    }
    for (let position = low; position < columnNodes.length; position += 1) {
      const node = columnNodes[position];
      if (node.y > bottom) break;
      if (node.x > right || node.x + node.width < left || node.y + node.height < top) continue;
      nodes.push(node);
      const connector = index.connectorByTargetId.get(node.id);
      if (connector) connectors.push(connector);
    }
  }
  nodes.sort((leftNode, rightNode) =>
    leftNode.depth === rightNode.depth
      ? leftNode.y - rightNode.y
      : leftNode.depth - rightNode.depth
  );
  return { nodes, connectors };
}

function normalizeNodeSize(
  measured: MindMapNodeSize | undefined,
  fallback: MindMapNodeSize
): MindMapNodeSize {
  if (
    !measured ||
    !Number.isFinite(measured.width) ||
    !Number.isFinite(measured.height) ||
    measured.width <= 0 ||
    measured.height <= 0
  ) {
    return fallback;
  }
  return {
    width: Math.min(
      MIND_MAP_NODE_MAX_WIDTH,
      Math.max(MIND_MAP_NODE_MIN_WIDTH, Math.ceil(measured.width))
    ),
    height: Math.max(MIND_MAP_NODE_MIN_HEIGHT, Math.ceil(measured.height))
  };
}

function approximateGlyphWidth(character: string): number {
  if (/\s/u.test(character)) return 4.5;
  if (character.codePointAt(0)! > 0xff) return 16.5;
  if (/[MW@#%&]/u.test(character)) return 14;
  if (/[A-Z]/u.test(character)) return 10.5;
  if (/[ilI1'`.,:;|!]/u.test(character)) return 5.5;
  return 9;
}
