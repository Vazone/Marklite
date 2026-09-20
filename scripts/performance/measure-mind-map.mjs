import { writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';
import { build } from 'esbuild';

const bundledMindMap = await build({
  entryPoints: [fileURLToPath(new URL('../../src/lib/mindMap.ts', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false
});
const {
  buildMindMap,
  createMindMapViewportIndex,
  layoutMindMap,
  projectMindMapViewport
} = await import(`data:text/javascript;base64,${Buffer.from(bundledMindMap.outputFiles[0].contents).toString('base64')}`);

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const outputIndex = process.argv.indexOf('--output');
const outputPath = outputIndex >= 0
  ? resolve(process.argv[outputIndex + 1])
  : resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'mind-map-0053.json');
const sampleCount = 5;

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

function makeOutline(kind, size) {
  return Array.from({ length: size }, (_, index) => ({
    level: kind === 'layered' ? index % 6 + 1 : 1,
    title: kind === 'long'
      ? `第 ${index + 1} 节项目管理机构人员配比与安全责任完整说明 LongUnbrokenHeading${index + 1}`
      : `Heading ${index + 1}`,
    line: index + 1,
    slug: `heading-${index + 1}`
  }));
}

function measureCase(kind, size) {
  const outline = makeOutline(kind, size);
  const buildSamples = [];
  const layoutSamples = [];
  const indexSamples = [];
  const projectionSamples = [];
  let peakHeapBytes = process.memoryUsage().heapUsed;
  let finalNodeCount = 0;
  let maxProjectedNodes = 0;

  for (let sample = 0; sample < sampleCount; sample += 1) {
    globalThis.gc?.();
    let startedAt = performance.now();
    const tree = buildMindMap('Performance.md', outline);
    buildSamples.push(performance.now() - startedAt);
    peakHeapBytes = Math.max(peakHeapBytes, process.memoryUsage().heapUsed);

    startedAt = performance.now();
    const layout = layoutMindMap(tree, new Set());
    layoutSamples.push(performance.now() - startedAt);
    finalNodeCount = layout.nodes.length;
    peakHeapBytes = Math.max(peakHeapBytes, process.memoryUsage().heapUsed);

    startedAt = performance.now();
    const index = createMindMapViewportIndex(layout);
    indexSamples.push(performance.now() - startedAt);

    startedAt = performance.now();
    for (const fraction of [0, 0.25, 0.5, 0.75, 1]) {
      const projection = projectMindMapViewport(index, {
        left: 0,
        top: Math.max(0, (layout.height - 800) * fraction),
        width: 1200,
        height: 800
      });
      maxProjectedNodes = Math.max(maxProjectedNodes, projection.nodes.length);
    }
    projectionSamples.push(performance.now() - startedAt);
    peakHeapBytes = Math.max(peakHeapBytes, process.memoryUsage().heapUsed);
  }
  globalThis.gc?.();

  if (finalNodeCount !== size + 1) throw new Error(`${kind} layout lost nodes`);
  return {
    kind,
    headingCount: size,
    layoutNodeCount: finalNodeCount,
    maxProjectedNodes,
    build: summarize(buildSamples),
    layout: summarize(layoutSamples),
    viewportIndex: summarize(indexSamples),
    fiveViewportProjections: summarize(projectionSamples),
    peakHeapBytes
  };
}

globalThis.gc?.();
const heapBeforeBytes = process.memoryUsage().heapUsed;
const cases = [
  measureCase('broad', 150_000),
  measureCase('layered', 150_000),
  measureCase('long', 10_000)
];
globalThis.gc?.();
const heapAfterReleaseBytes = process.memoryUsage().heapUsed;

const result = {
  schemaVersion: 1,
  capturedAt: new Date().toISOString(),
  methodology: {
    runtime: 'Node executes an esbuild bundle of the production TypeScript module and records five complete samples per corpus',
    corpora: '150k broad H1, 150k repeating H1-H6, and 10k long mixed Chinese/ASCII headings',
    projection: 'five 1200x800 viewports from the top through the bottom of the complete layout',
    memory: 'Node runs with --expose-gc; heap is collected before cases and after all case-local references are released'
  },
  runtime: { node: process.version, platform: process.platform, architecture: process.arch },
  heapBeforeBytes,
  heapAfterReleaseBytes,
  releaseDeltaBytes: heapAfterReleaseBytes - heapBeforeBytes,
  cases
};

await writeFile(outputPath, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
process.stdout.write(`${JSON.stringify({ output: outputPath, ...result }, null, 2)}\n`);
