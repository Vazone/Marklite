import fixtures from '../shared/desktop-contract-fixtures.json';
import { describe, expect, test, vi } from 'vitest';
import { ExportProgressTask, parseExportProgress, type ExportProgressView } from './exportProgress';

const event = { jobId: 'job', format: 'png', sequence: 1, stage: 'rendering', status: 'running', work: {kind: 'chapter', index: 1, total: 2, completed: 0} };

describe('shared export progress', () => {
  test('accepts the shared Rust event fixture without changing its shape', () => {
    expect(parseExportProgress(fixtures.exportProgress)).toEqual(fixtures.exportProgress);
  });
  test('ignores other jobs, malformed counts, duplicates and late reports', async () => {
    const views: ExportProgressView[] = [];
    let receive: (value: unknown) => void = () => {};
    const unlisten = vi.fn();
    const task = new ExportProgressTask('job', 'png', view => views.push(view));
    await task.subscribe(async listener => { receive = listener; return unlisten; });
    receive({ ...event, jobId: 'old' });
    receive({ ...event, work: { ...event.work, completed: -1 } });
    expect(views).toHaveLength(1);
    receive(event);
    receive({ ...event, stage: 'writing' });
    expect(views).toHaveLength(2);
    task.finish('cancelled');
    receive({ ...event, sequence: 3 });
    expect(views.at(-1)?.status).toBe('cancelled');
    expect(unlisten).toHaveBeenCalledOnce();
    task.dispose();
    expect(unlisten).toHaveBeenCalledOnce();
  });

  test('backend completion waits for frontend result validation', async () => {
    const change = vi.fn();
    let receive: (value: unknown) => void = () => {};
    const task = new ExportProgressTask('job', 'png', change);
    await task.subscribe(async listener => { receive = listener; return () => {}; });
    receive({ ...event, stage: 'finalizing', status: 'succeeded', work: null });
    expect(change.mock.lastCall?.[0]).toMatchObject({status:'running', stage:'finalizing'});
    receive({ ...event, sequence: 2 });
    expect(change.mock.lastCall?.[0]).toMatchObject({status:'running', stage:'finalizing'});
    task.finish('failed'); // e.g. returned target does not match the frozen job.
    expect(change.mock.lastCall?.[0].status).toBe('failed');
  });

  test('disposal during listener setup releases the eventual subscription', async () => {
    let resolve!: (unlisten: () => void) => void;
    const unlisten = vi.fn();
    const task = new ExportProgressTask('job', 'pdf', vi.fn());
    const subscribed = task.subscribe(() => new Promise(done => { resolve = done; }));
    task.dispose();
    resolve(unlisten);
    await subscribed;
    expect(unlisten).toHaveBeenCalledOnce();
  });

  test('unmeasured waits have no invented count and malformed payloads are rejected', () => {
    const change = vi.fn();
    const task = new ExportProgressTask('job', 'html', change);
    task.phase('choosingTarget');
    expect(change.mock.lastCall?.[0]).toMatchObject({status:'waiting',work:null});
    expect(parseExportProgress({ ...event, sequence: Infinity })).toBeNull();
    expect(parseExportProgress({ ...event, work: {...event.work, index:3} })).toBeNull();
    expect(parseExportProgress({ ...event, stage:'unknown' })).toBeNull();
  });
});
