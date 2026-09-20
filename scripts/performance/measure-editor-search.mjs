import { writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';
import { EditorState } from '@codemirror/state';
import {
  editorSearchReplacementChange,
  findEditorSearchMatches,
  updateEditorSearchMatches,
  visibleEditorSearchMatches
} from '../../src/lib/editorSearch.ts';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const outputIndex = process.argv.indexOf('--output');
const outputPath = outputIndex >= 0
  ? resolve(process.argv[outputIndex + 1])
  : resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'editor-search-0052.json');
const sampleCount = 30;

function percentile(values, percent) {
  const ordered = [...values].sort((left, right) => left - right);
  return ordered[Math.ceil(percent * ordered.length) - 1];
}

function summarize(values) {
  return {
    samples: values.length,
    p50Ms: percentile(values, 0.5),
    p95Ms: percentile(values, 0.95),
    maxMs: Math.max(...values),
    rawMs: values
  };
}

function measure(operation) {
  for (let index = 0; index < 3; index += 1) operation();
  const values = [];
  for (let index = 0; index < sampleCount; index += 1) {
    const startedAt = performance.now();
    operation();
    values.push(performance.now() - startedAt);
  }
  return summarize(values);
}

const heapBeforeBytes = process.memoryUsage().heapUsed;
const cases = [];
for (const size of [1_000, 10_000, 150_000]) {
  const initial = EditorState.create({ doc: 'a'.repeat(size) });
  const matches = findEditorSearchMatches(initial.doc, 'a');
  const transaction = initial.update({ changes: { from: initial.doc.length, insert: 'a' } });
  const visibleRange = [{ from: Math.floor(size / 2), to: Math.floor(size / 2) + 80 }];
  const scan = measure(() => {
    const found = findEditorSearchMatches(initial.doc, 'a');
    if (found.length !== size) throw new Error(`scan lost matches at ${size}`);
  });
  const incrementalUpdate = measure(() => {
    const updated = updateEditorSearchMatches(transaction.state.doc, 'a', matches, transaction.changes);
    if (updated.length !== size + 1) throw new Error(`incremental update lost matches at ${size}`);
  });
  const visibleDecorations = measure(() => {
    const visible = visibleEditorSearchMatches(matches, visibleRange, size - 1);
    if (visible.length !== 81) throw new Error(`visible lookup returned ${visible.length} matches at ${size}`);
  });
  const replaceAll = measure(() => {
    const change = editorSearchReplacementChange(initial.doc, matches, 'b');
    if (!change || change.insert.length !== size) throw new Error(`replace-all lost content at ${size}`);
  });
  cases.push({ size, matchCount: matches.length, scan, incrementalUpdate, visibleDecorations, replaceAll });
}
const heapAfterBytes = process.memoryUsage().heapUsed;

const result = {
  schemaVersion: 1,
  capturedAt: new Date().toISOString(),
  methodology: {
    runtime: 'Node imports the production TypeScript module directly; three warmups precede 30 timed samples',
    scan: 'case-insensitive literal RegExpCursor scan without Text.toString()',
    incrementalUpdate: 'append one matching character, map complete results, and rescan query-sized context',
    visibleDecorations: 'binary-search one 80-code-unit viewport plus one active off-screen match',
    replaceAll: 'build one content-preserving replacement span for all matches',
    memory: 'process heap snapshots are observational and do not force garbage collection'
  },
  runtime: {
    node: process.version,
    platform: process.platform,
    architecture: process.arch
  },
  heapBeforeBytes,
  heapAfterBytes,
  cases
};

await writeFile(outputPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
process.stdout.write(`${JSON.stringify({ output: outputPath, heapBeforeBytes, heapAfterBytes, cases }, null, 2)}\n`);
