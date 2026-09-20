import { describe, expect, test } from 'vitest';
import {
  approximateMindMapTextWidth,
  estimateMindMapNodeSize,
  MIND_MAP_NODE_TEXT_CHROME,
  wrapMindMapLabel
} from './mindMap';
import {
  MIND_MAP_SVG_MAX_HEADINGS,
  MindMapSvgExportError,
  serializeMindMapSvg
} from './mindMapSvg';

describe('mind map SVG export', () => {
  test('serializes a complete expanded vector map with colors, connectors and line metadata', () => {
    const svg = serializeMindMapSvg('项目 & 计划.md', [
      { level: 1, title: '一、总体安排', line: 1, slug: 'plan' },
      { level: 2, title: '安全 <质量> 与人员责任分工说明', line: 104, slug: 'safety' }
    ]);

    expect(svg).toContain('data-marklite-mind-map="1"');
    expect(svg).toContain('<path d="M ');
    expect(svg).toContain('stroke="#8655c5"');
    expect(svg).toContain('Line 104');
    expect(svg).toContain('项目 &amp; 计划.md mind map');
    expect(svg).toContain('安全 &lt;质量&gt;');
    expect(svg).not.toContain('<script');
  });

  test('wraps long Chinese and ASCII labels without losing any content', () => {
    const chinese = '项目管理机构人员配比与安全责任说明';
    const ascii = 'LongUnbrokenHeadingForMindMapExport';
    expect(wrapMindMapLabel(chinese, 100).join('')).toBe(chinese);
    expect(wrapMindMapLabel(ascii, 100).join('')).toBe(ascii);
    expect(wrapMindMapLabel(chinese, 100).length).toBeGreaterThan(1);
  });

  test('uses the shared pixel model so every exported line fits its node content box', () => {
    const title = '7.1 工程类别和安全员触发判断与项目管理机构人员资格材料';
    const size = estimateMindMapNodeSize({ label: title, line: 106 });
    const contentWidth = size.width - MIND_MAP_NODE_TEXT_CHROME;
    const lines = wrapMindMapLabel(title, contentWidth);
    const svg = serializeMindMapSvg('投标项目.md', [
      { level: 2, title, line: 106, slug: 'safety' }
    ]);

    expect(lines.join('')).toBe(title);
    expect(lines.every((line) => approximateMindMapTextWidth(line) <= contentWidth)).toBe(true);
    expect(svg).toContain(`width="${size.width}" height="${size.height}"`);
    for (const line of lines) expect(svg).toContain(`>${line}</tspan>`);
  });

  test('fails explicitly before generating a partial SVG beyond the complete-export budget', () => {
    const outline = Array.from({ length: MIND_MAP_SVG_MAX_HEADINGS + 1 }, (_, index) => ({
      level: 1,
      title: `Heading ${index + 1}`,
      line: index + 1,
      slug: `heading-${index + 1}`
    }));

    expect(() => serializeMindMapSvg('Too-large.md', outline))
      .toThrow(new MindMapSvgExportError());
  });
});
