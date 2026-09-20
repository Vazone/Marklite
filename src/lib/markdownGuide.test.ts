import { describe, expect, test } from 'vitest';
import {
  filterMarkdownGuide,
  markdownGuideGroups,
  markdownGuideTopics,
  markdownGuideTopicsFor,
  namedExtensionGuideTopics
} from './markdownGuide';
import capabilityContract from '../shared/markdown-capabilities.json';

describe('MarkLite Markdown guide catalog', () => {
  const requiredBasic = [
    'atx-headings', 'setext-headings', 'paragraphs', 'line-breaks', 'bold', 'italic',
    'bold-italic', 'blockquotes', 'nested-blockquotes', 'ordered-lists', 'unordered-lists',
    'nested-list-content', 'inline-code', 'escaped-backticks', 'indented-code-blocks',
    'horizontal-rules', 'inline-links', 'link-titles', 'angle-autolinks', 'formatted-links',
    'reference-links', 'images', 'linked-images', 'escaping-characters', 'inline-html', 'block-html'
  ];
  const requiredExtended = [
    'extension-profile', 'math', 'tables', 'table-alignment', 'table-formatting-pipes', 'fenced-code',
    'code-info-string', 'footnotes', 'heading-ids', 'heading-id-links', 'definition-lists', 'github-alerts',
    'strikethrough', 'task-lists', 'unicode-emoji', 'emoji-shortcodes',
    'bare-url-autolinks', 'smart-punctuation', 'mermaid', 'highlight-syntax',
    'subscript-superscript', 'front-matter', 'document-toc'
  ];

  test('has stable unique topics for every learning group', () => {
    expect(new Set(markdownGuideTopics.map((topic) => topic.id)).size).toBe(
      markdownGuideTopics.length
    );
    for (const group of markdownGuideGroups) {
      expect(markdownGuideTopics.some((topic) => topic.group === group.id)).toBe(true);
    }
    expect(markdownGuideTopicsFor('zh-CN').map((topic) => topic.id)).toEqual(
      markdownGuideTopics.map((topic) => topic.id)
    );
  });

  test('covers every basic and extended syntax form in the offline catalog', () => {
    const ids = new Set(markdownGuideTopics.map((topic) => topic.id));
    for (const id of [...requiredBasic, ...requiredExtended]) expect(ids.has(id)).toBe(true);
    expect(markdownGuideTopics.filter((topic) => topic.group === 'basic')).toHaveLength(requiredBasic.length);
    expect(markdownGuideTopics.filter((topic) => topic.group === 'extended')).toHaveLength(requiredExtended.length);
  });

  test('does not advertise unsupported parser extensions as complete support', () => {
    expect(markdownGuideTopics.find((topic) => topic.id === 'definition-lists')?.support).toBe('supported');
    expect(markdownGuideTopics.find((topic) => topic.id === 'emoji-shortcodes')?.support).toBe(
      'unsupported'
    );
    expect(markdownGuideTopics.find((topic) => topic.id === 'bare-url-autolinks')?.support).toBe('supported');
    expect(markdownGuideTopics.find((topic) => topic.id === 'smart-punctuation')?.support).toBe('unsupported');
    expect(markdownGuideTopics.find((topic) => topic.id === 'inline-html')?.support).toBe(
      'limited'
    );
    expect(markdownGuideTopics.find((topic) => topic.id === 'tables')?.support).toBe(
      'supported'
    );
    expect(markdownGuideTopics.find((topic) => topic.id === 'math')?.support).toBe('limited');
    expect(markdownGuideTopics.find((topic) => topic.id === 'mermaid')?.support).toBe('limited');
  });

  test('every named capability has a guide fixture and derives its support from the contract', () => {
    expect(Object.keys(namedExtensionGuideTopics).sort()).toEqual(
      capabilityContract.namedExtensions.map((extension) => extension.id).sort()
    );
    for (const language of ['en', 'zh-CN'] as const) {
      const topics = markdownGuideTopicsFor(language);
      expect(topics.every((topic) => topic.fixture && topic.example)).toBe(true);
      for (const extension of capabilityContract.namedExtensions) {
        const topic = topics.find((item) => item.id === namedExtensionGuideTopics[extension.id as keyof typeof namedExtensionGuideTopics]);
        expect(topic?.fixture).toBe(extension.fixture);
        expect(topic?.support).toBe(extension.statuses.preview === 'source' ? 'unsupported'
          : extension.statuses.preview === 'conditional' ? 'limited' : 'supported');
      }
    }
    expect(markdownGuideTopicsFor('zh-CN').find((topic) => topic.id === 'escaped-backticks')?.example)
      .toBe('``在 Markdown 中写 `code` ``');
  });

  test('filters only within the selected group', () => {
    expect(filterMarkdownGuide('basic', 'Setext').map((topic) => topic.id)).toEqual([
      'setext-headings'
    ]);
    expect(filterMarkdownGuide('extended', 'Setext')).toEqual([]);
  });

  test('does not expose a product-owned external learning reference', () => {
    const moduleText = JSON.stringify(markdownGuideTopics);
    expect(moduleText).not.toContain('markdown.com.cn');
    expect(moduleText).not.toContain('github.com');
  });
});
