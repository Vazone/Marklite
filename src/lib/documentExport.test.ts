import { describe, expect, test, vi } from 'vitest';
import type { ExportRequest } from './tauriApi';
import {
  createExportJobId,
  defaultExportOptions,
  defaultExportPath,
  ExportProtocolError,
  exportSuccessMessage,
  freezeExportSnapshot,
  runExportJob,
  type ExportSource
} from './documentExport';

function source(): ExportSource {
  return {
    id: 'tab-a',
    path: 'C:\\notes\\draft.md',
    title: 'draft.md',
    content: 'version one',
    contentRevision: 4
  };
}

describe('document export snapshots', () => {
  test.each(['html', 'pdf', 'docx', 'svg'] as const)('suggests remembered location and commits after validated %s success', async (format) => {
    const remember = vi.fn(async () => {});
    const onWarning = vi.fn();
    const pick = vi.fn(async (path: string) => path);
    const current = { ...source(), mindMapSvg: '<svg>frozen</svg>' };
    const execute = vi.fn(async (request: ExportRequest) => {
      expect(remember).not.toHaveBeenCalled();
      expect(request.snapshot.content).toBe('version one');
      if (format === 'svg') expect(request.mindMapSvg).toBe('<svg>frozen</svg>');
      return {jobId:request.snapshot.jobId,format:request.format,path:request.targetPath,warnings:[]};
    });
    const result = await runExportJob(current, format, defaultExportOptions, pick, execute, 'remember', {
      suggest: async (path) => {
        expect(path).toBe(`C:\\notes\\draft.${format}`);
        current.content = 'edited'; current.mindMapSvg = '<svg>changed</svg>';
        return {path:`C:\\last\\draft.${format}`, warning:null};
      }, remember, onWarning
    });
    expect(pick).toHaveBeenCalledWith(`C:\\last\\draft.${format}`);
    expect(remember).toHaveBeenCalledExactlyOnceWith(result?.path);
    expect(onWarning).not.toHaveBeenCalled();
  });

  test.each(['cancel', 'failure', 'job', 'path', 'format'])('does not remember after %s', async (mode) => {
    const remember = vi.fn(async () => {});
    const pending = runExportJob(source(), 'html', defaultExportOptions, async () => mode === 'cancel' ? null : 'out.html', async request => {
      if (mode === 'failure') throw new Error('writer failed');
      return { jobId: mode === 'job' ? 'wrong' : request.snapshot.jobId, path:mode === 'path' ? 'wrong' : request.targetPath, format: mode === 'format' ? 'pdf' : request.format, warnings:[] };
    }, 'job', {suggest:async path => ({path,warning:null}),remember,onWarning:vi.fn()});
    if (mode === 'cancel') expect(await pending).toBeNull();
    else await expect(pending).rejects.toBeDefined();
    expect(remember).not.toHaveBeenCalled();
  });

  test('read and write failures remain auxiliary to successful export', async () => {
    const onWarning = vi.fn(); const pick = vi.fn(async (path: string) => path);
    const result = await runExportJob(source(), 'html', defaultExportOptions, pick, async request => ({jobId:request.snapshot.jobId,path:request.targetPath,format:request.format,warnings:[]}), 'job', {
      suggest:async () => { throw new Error('read failure'); },
      remember:async () => { throw new Error('write failure'); }, onWarning
    });
    expect(result?.path).toBe('C:\\notes\\draft.html');
    expect(onWarning.mock.calls.map(([error]) => error.message)).toEqual(['read failure','write failure']);
  });

  test('uses backend fallback while exposing its warning', async () => {
    const onWarning = vi.fn(); const warning = {code:'EXPORT_LOCATION_READ_FAILED',message:'read failed'};
    const pick = vi.fn(async () => null);
    await runExportJob(source(),'html',defaultExportOptions,pick,vi.fn(),'job', {
      suggest:async () => ({path:'draft.html',warning}),remember:vi.fn(),onWarning
    });
    expect(pick).toHaveBeenCalledWith('draft.html');
    expect(onWarning).toHaveBeenCalledWith(warning);
  });
  test('freezes tab identity, revision and content before save-path waiting', () => {
    const current = source();
    const snapshot = freezeExportSnapshot(current, 'job-fixed');
    current.content = 'version two';
    current.contentRevision = 5;
    current.id = 'tab-b';

    expect(snapshot).toEqual({
      jobId: 'job-fixed',
      tabId: 'tab-a',
      contentRevision: 4,
      sourcePath: 'C:\\notes\\draft.md',
      title: 'draft.md',
      content: 'version one'
    });
    expect(Object.isFrozen(snapshot)).toBe(true);
  });

  test('creates stable format-specific default paths', () => {
    expect(defaultExportPath(source(), 'docx')).toBe(
      'C:\\notes\\draft.docx'
    );
    expect(defaultExportPath({ ...source(), path: null, title: '中文.markdown' }, 'pdf')).toBe(
      '中文.pdf'
    );
    expect(defaultExportPath({ ...source(), path: null, title: '' }, 'html')).toBe(
      'Untitled.html'
    );
    expect(defaultExportPath(source(), 'svg')).toBe('C:\\notes\\draft.svg');
  });

  test('keeps job ids distinct and reports structured-warning completion', () => {
    expect(createExportJobId(1, 0.1)).not.toBe(createExportJobId(1, 0.2));
    expect(exportSuccessMessage('html', 0)).toBe('HTML exported');
    expect(exportSuccessMessage('docx', 2)).toContain('2 warnings');
    expect(exportSuccessMessage('svg', 0)).toBe('SVG exported');
  });

  test('attaches a mind-map payload only to SVG export requests', async () => {
    let received: ExportRequest | undefined;
    await runExportJob(
      { ...source(), mindMapSvg: '<svg data-marklite-mind-map="1"></svg>' },
      'svg',
      { paperSize: 'a4', orientation: 'portrait', margin: 'normal', includeTitle: true, includeLocalImages: false },
      async () => 'C:\\out\\draft.svg',
      async (request) => {
        received = request;
        return { jobId: request.snapshot.jobId, format: request.format, path: request.targetPath, warnings: [] };
      },
      'svg-job'
    );

    expect(received?.mindMapSvg).toContain('data-marklite-mind-map');
  });

  test('keeps concurrent, switched and closed source jobs independently owned', async () => {
    const first = source();
    const second = { ...source(), id: 'tab-b', content: 'second', contentRevision: 9 };
    let releaseFirst!: () => void;
    const firstPath = new Promise<string>((resolve) => {
      releaseFirst = () => resolve('C:\\out\\first.html');
    });
    const requests: Array<{ jobId: string; tabId: string; content: string }> = [];
    const execute = async (request: ExportRequest) => {
      requests.push({
        jobId: request.snapshot.jobId,
        tabId: request.snapshot.tabId,
        content: request.snapshot.content
      });
      return {
        jobId: request.snapshot.jobId,
        format: request.format,
        path: request.targetPath,
        warnings: []
      };
    };

    const pendingFirst = runExportJob(
      first,
      'html',
      { paperSize: 'a4', orientation: 'portrait', margin: 'normal', includeTitle: true, includeLocalImages: false },
      () => firstPath,
      execute,
      'job-first'
    );
    first.content = 'edited after confirmation';
    first.id = 'closed-tab';
    const completedSecond = await runExportJob(
      second,
      'docx',
      { paperSize: 'letter', orientation: 'landscape', margin: 'wide', includeTitle: false, includeLocalImages: false },
      async () => 'C:\\out\\second.docx',
      execute,
      'job-second'
    );
    releaseFirst();
    const completedFirst = await pendingFirst;

    expect(completedSecond?.jobId).toBe('job-second');
    expect(completedFirst?.jobId).toBe('job-first');
    expect(requests).toEqual([
      { jobId: 'job-second', tabId: 'tab-b', content: 'second' },
      { jobId: 'job-first', tabId: 'tab-a', content: 'version one' }
    ]);
  });

  test('freezes export options before waiting for the save path', async () => {
    const options = {
      paperSize: 'a4' as const,
      orientation: 'portrait' as const,
      margin: 'normal' as const,
      includeTitle: true,
      includeLocalImages: false
    };
    let releasePath!: () => void;
    const path = new Promise<string>((resolve) => {
      releasePath = () => resolve('C:\\out\\stable.pdf');
    });
    let received: ExportRequest | undefined;
    const pending = runExportJob(
      source(),
      'pdf',
      options,
      () => path,
      async (request) => {
        received = request;
        return { jobId: request.snapshot.jobId, format: request.format, path: request.targetPath, warnings: [] };
      },
      'stable-options'
    );

    options.includeTitle = false;
    options.includeLocalImages = true;
    releasePath();
    await pending;

    expect(received?.options).toMatchObject({ includeTitle: true, includeLocalImages: false });
  });

  test.each([
    ['job id', { jobId: 'wrong-job', format: 'pdf' as const, path: 'C:\\out\\a.pdf' }],
    ['format', { jobId: 'expected-job', format: 'html' as const, path: 'C:\\out\\a.pdf' }],
    ['path', { jobId: 'expected-job', format: 'pdf' as const, path: 'C:\\out\\other.pdf' }]
  ])('rejects responses with a mismatched %s', async (_field, response) => {
    const pending = runExportJob(
      source(),
      'pdf',
      { paperSize: 'a4', orientation: 'portrait', margin: 'normal', includeTitle: true, includeLocalImages: false },
      async () => 'C:\\out\\a.pdf',
      async () => ({ ...response, warnings: [] }),
      'expected-job'
    );

    await expect(pending).rejects.toMatchObject({
      name: 'ExportProtocolError',
      code: 'EXPORT_RESULT_MISMATCH'
    });
    await expect(pending).rejects.toBeInstanceOf(ExportProtocolError);
  });
});

test('SVG enters payload validation after selection without claiming Markdown parsing', async () => {
  const phases: string[] = [];
  await runExportJob({ ...source(), mindMapSvg: '<svg></svg>' }, 'svg', defaultExportOptions,
    async () => 'C:/out/draft.svg', async request => {
      expect(phases.at(-1)).toBe('validating');
      return { jobId: request.snapshot.jobId, format: request.format, path: request.targetPath, warnings: [] };
    }, 'svg-phases', undefined, stage => phases.push(stage));
  expect(phases).toContain('choosingTarget');
  expect(phases).not.toContain('parsing');
  expect(phases.at(-1)).toBe('finalizing');
});

test('PNG bypasses remembered file directories and requires a directory result', async () => {
  const suggest = vi.fn(); const remember = vi.fn();
  const result = await runExportJob(source(), 'png', defaultExportOptions, async () => 'C:/notes/draft', async request => {
    expect(request.targetKind).toBe('directory');
    return {jobId: request.snapshot.jobId, format: 'png', targetKind: 'directory', path: request.targetPath, warnings: []};
  }, 'png-job', {suggest, remember, onWarning: vi.fn()});
  expect(result?.path).toBe('C:/notes/draft');
  expect(suggest).not.toHaveBeenCalled(); expect(remember).not.toHaveBeenCalled();
  await expect(runExportJob(source(), 'png', defaultExportOptions, async () => 'C:/notes/draft', async request => ({
    jobId: request.snapshot.jobId, format: 'png', path: request.targetPath, warnings: []
  }))).rejects.toMatchObject({code:'EXPORT_RESULT_MISMATCH'});
});
