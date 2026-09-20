import { describe, expect, test } from 'vitest';
import type { OutlineItem } from './tauriApi';
import {
  approximateMindMapTextWidth,
  buildMindMap,
  collectCollapsibleNodeIds,
  createMindMapViewportIndex,
  estimateMindMapNodeSize,
  layoutMindMap,
  MindMapLayoutError,
  MIND_MAP_NODE_MAX_WIDTH,
  MIND_MAP_NODE_TEXT_CHROME,
  projectMindMapViewport,
  wrapMindMapLabel
} from './mindMap';

function heading(level: number, title: string, line: number): OutlineItem {
  return { level, title, line, slug: title.toLowerCase() };
}

describe('Markdown mind map', () => {
  test('uses the nearest lower heading as parent across skipped levels', () => {
    const tree = buildMindMap('Project.md', [
      heading(1, 'Plan', 1),
      heading(3, 'Research', 4),
      heading(2, 'Build', 8),
      heading(1, 'Review', 12)
    ]);

    expect(tree.label).toBe('Project.md');
    expect(tree.children.map((node) => node.label)).toEqual(['Plan', 'Review']);
    expect(tree.children[0].children.map((node) => node.label)).toEqual([
      'Research',
      'Build'
    ]);
  });

  test('keeps duplicate and Chinese headings as distinct ordered nodes', () => {
    const tree = buildMindMap('中文.md', [
      heading(1, '概览', 1),
      heading(2, '相同', 2),
      heading(2, '相同', 3)
    ]);

    const children = tree.children[0].children;
    expect(children.map((node) => node.label)).toEqual(['相同', '相同']);
    expect(new Set(children.map((node) => node.id)).size).toBe(2);
    expect(children.map((node) => node.line)).toEqual([2, 3]);
  });

  test('removes collapsed descendants from layout and keeps every connector owned', () => {
    const tree = buildMindMap('Map.md', [
      heading(1, 'A', 1),
      heading(2, 'A1', 2),
      heading(1, 'B', 3)
    ]);
    const branch = tree.children[0];
    const expanded = layoutMindMap(tree, new Set());
    const collapsed = layoutMindMap(tree, new Set([branch.id]));

    expect(expanded.nodes.map((node) => node.label)).toContain('A1');
    expect(collapsed.nodes.map((node) => node.label)).not.toContain('A1');
    expect(collapsed.connectors).toHaveLength(collapsed.nodes.length - 1);
    expect(collectCollapsibleNodeIds(tree)).toEqual([tree.id, branch.id]);
  });

  test('uses measured node dimensions for spacing, connectors and canvas bounds', () => {
    const tree = buildMindMap('Map.md', [
      heading(1, 'A long heading that needs two visible lines', 1),
      heading(1, 'B', 2)
    ]);
    const first = tree.children[0];
    const layout = layoutMindMap(tree, new Set(), new Map([
      [tree.id, { width: 220, height: 60 }],
      [first.id, { width: 310, height: 92 }]
    ]));
    const firstLayout = layout.nodes.find((node) => node.id === first.id)!;
    const secondLayout = layout.nodes.find((node) => node.id === tree.children[1].id)!;
    const connector = layout.connectors.find((item) => item.id.endsWith(`->${first.id}`))!;

    expect(firstLayout.width).toBe(310);
    expect(firstLayout.height).toBe(92);
    expect(secondLayout.y).toBeGreaterThanOrEqual(firstLayout.y + firstLayout.height + 30);
    expect(connector.path).toContain(`${firstLayout.x} ${firstLayout.y + firstLayout.height / 2}`);
    expect(layout.height).toBeGreaterThanOrEqual(secondLayout.y + secondLayout.height + 32);
  });

  test('keeps complete labels inside a bounded pixel width and grows height after reaching max width', () => {
    const label = '7.1 工程类别和安全员触发判断与项目管理机构人员资格材料完整说明';
    const size = estimateMindMapNodeSize({ label, line: 106 });
    const contentWidth = size.width - MIND_MAP_NODE_TEXT_CHROME;
    const lines = wrapMindMapLabel(label, contentWidth);

    expect(size.width).toBe(MIND_MAP_NODE_MAX_WIDTH);
    expect(lines.length).toBeGreaterThan(1);
    expect(lines.join('')).toBe(label);
    expect(lines.every((line) => approximateMindMapTextWidth(line) <= contentWidth)).toBe(true);
    expect(size.height).toBeGreaterThan(60);
  });

  test('uses the widest node in a depth as the next column boundary', () => {
    const tree = buildMindMap('Map.md', [
      heading(1, 'A', 1),
      heading(2, 'A1', 2),
      heading(1, 'B', 3)
    ]);
    const first = tree.children[0];
    const second = tree.children[1];
    const grandchild = first.children[0];
    const layout = layoutMindMap(tree, new Set(), new Map([
      [tree.id, { width: 210, height: 60 }],
      [first.id, { width: 220, height: 60 }],
      [second.id, { width: 340, height: 60 }],
      [grandchild.id, { width: 196, height: 60 }]
    ]));
    const firstLayout = layout.nodes.find((node) => node.id === first.id)!;
    const grandchildLayout = layout.nodes.find((node) => node.id === grandchild.id)!;

    expect(grandchildLayout.x).toBe(firstLayout.x + 340 + 72);
  });

  test('lays out 150,000 broad headings without argument or call stack overflow', () => {
    const outline = Array.from({ length: 150_000 }, (_, index) =>
      heading(1, `Heading ${index + 1}`, index + 1)
    );
    const tree = buildMindMap('Large.md', outline);

    const layout = layoutMindMap(tree, new Set());

    expect(layout.nodes).toHaveLength(150_001);
    expect(layout.connectors).toHaveLength(150_000);
    expect(layout.height).toBeGreaterThan(150_000 * 60);
  }, 30_000);

  test('projects a bounded viewport while keeping every node in the full layout', () => {
    const outline = Array.from({ length: 10_000 }, (_, index) =>
      heading(1, `Heading ${index + 1}`, index + 1)
    );
    const layout = layoutMindMap(buildMindMap('Large.md', outline), new Set());
    const index = createMindMapViewportIndex(layout);
    const first = projectMindMapViewport(index, { left: 0, top: 0, width: 900, height: 600 });
    const last = projectMindMapViewport(index, {
      left: 0,
      top: layout.height - 600,
      width: 900,
      height: 600
    });

    expect(layout.nodes).toHaveLength(10_001);
    expect(first.nodes.length).toBeLessThan(30);
    expect(last.nodes.length).toBeLessThan(30);
    expect(last.nodes.at(-1)?.label).toBe('Heading 10000');
    expect(last.connectors.every((connector) =>
      last.nodes.some((node) => node.id === connector.targetId)
    )).toBe(true);
  });

  test('fails explicitly at the configured scale boundary and cancellation checkpoint', () => {
    const tree = buildMindMap('Map.md', [heading(1, 'A', 1)]);

    expect(() => layoutMindMap(tree, new Set(), new Map(), { maxNodes: 1 }))
      .toThrow(new MindMapLayoutError('MIND_MAP_LAYOUT_TOO_LARGE'));
    expect(() => layoutMindMap(tree, new Set(), new Map(), { shouldCancel: () => true }))
      .toThrow(new MindMapLayoutError('MIND_MAP_LAYOUT_CANCELLED'));
  });
});
