import { describe, expect, test } from 'vitest';
import { exportWarningReport, serializeExportWarningReport } from './exportWarningReport';

describe('export warning report', () => {
  test('keeps a job bound to its frozen Unicode source and locates byte spans', () => {
    const source = { id: 'tab-1', path: 'C:/notes/图表.md', title: '图表', contentRevision: 7, content: '# 标题\n```mermaid\nA[中文]-->B\n```' };
    const start = new TextEncoder().encode('# 标题\n```mermaid\n').length;
    const result = { jobId: 'job-1', format: 'docx' as const, path: 'C:/out/图表.docx', warnings: [
      { code: 'DIAGRAM_PARSE_FAILED', message: 'bad syntax', target: `diagram-0:${start}-${start + 4}` },
      { code: 'LOCAL_IMAGE_MISSING', message: 'missing image', target: './missing.png' }
    ] };
    const report = exportWarningReport(source, result);
    source.content = 'changed later';
    expect(report.warnings[0]).toMatchObject({ line: 3, excerpt: 'A[中文]-->B' });
    expect(report.warnings[1].line).toBeNull();
    expect(serializeExportWarningReport(report)).toContain('job-1');
    expect(serializeExportWarningReport(report)).toContain('(line 3)');
    expect(report.contentRevision).toBe(7);
  });
});
