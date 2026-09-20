import { createHash } from 'node:crypto';
import { execFile, spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { deflateSync, gzipSync } from 'node:zlib';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { cpus, freemem, hostname, platform, release, totalmem } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);
const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const corpusDirectory = resolve(repositoryRoot, 'tmp', 'performance-corpus');
const manifestPath = resolve(repositoryRoot, 'scripts', 'fixtures', 'performance-corpus-manifest.json');
const executablePath = resolve(repositoryRoot, 'src-tauri', 'target', 'release', 'marklite.exe');
const defaultOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'windows-reference.json');
const defaultSearchOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'dense-search-0052.json');
const defaultMindMapOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'mind-map-webview-0053.json');
const defaultInteractionOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'analysis-interaction-0057.json');
const defaultLiveSplitOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-0075.json');
const defaultLiveSplitMixedOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-mixed-0075.json');
const defaultLiveSplitIpcOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-ipc-0075.json');
const defaultLiveSplitReleaseOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-release-0075.json');
const defaultLiveSplitScrollOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-scroll-0075.json');
const defaultLiveSplitInputOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-input-0075.json');
const defaultLiveSplitOrdinaryOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-ordinary-0075.json');
const defaultLiveSplitPreviewOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-preview-0075.json');
const defaultLiveSplitTraceOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-trace-0077.json');
const defaultLiveSplitArticleOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'live-split-article-0077.json');
const defaultExtensionOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'extension-ipc-0098.json');
const defaultExtensionDiagramsOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'extension-diagrams-0098.json');
const defaultExtensionExportsOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'extension-exports-0098.json');
const defaultExtensionStartupOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'extension-startup-0098.json');
const defaultPreviewResourceOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'preview-resources-0058.json');
const defaultPdfResourceOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'pdf-resource-isolation-0059.json');
const defaultExportSemanticsOutput = resolve(repositoryRoot, 'benchmarks', 'performance', 'results', 'export-semantics-0060.json');
let startupSamples = 30;
let operationSamples = 30;

function percentile(values, percent) {
  if (values.length === 0) return null;
  const ordered = [...values].sort((left, right) => left - right);
  return ordered[Math.ceil(percent * ordered.length) - 1];
}

function summary(values) {
  return {
    samples: values.length,
    p50Ms: percentile(values, 0.5),
    p95Ms: percentile(values, 0.95),
    p99Ms: percentile(values, 0.99),
    maxMs: values.length ? Math.max(...values) : null
  };
}

function parseArguments(argv) {
  const outputIndex = argv.indexOf('--output');
  const executableIndex = argv.indexOf('--executable');
  const startupIndex = argv.indexOf('--startup-samples');
  const operationIndex = argv.indexOf('--operation-samples');
  const corpusLimitIndex = argv.indexOf('--corpus-limit');
  const liveSplitCaseIndex = argv.indexOf('--live-split-case');
  const extensionDiagramCaseIndex = argv.indexOf('--extension-diagram-case');
  const searchOnly = argv.includes('--search-only');
  const mindMapOnly = argv.includes('--mind-map-only');
  const interactionOnly = argv.includes('--interaction-only');
  const liveSplitOnly = argv.includes('--live-split-only');
  const liveSplitMixedOnly = argv.includes('--live-split-mixed-only');
  const liveSplitIpcOnly = argv.includes('--live-split-ipc-only');
  const liveSplitReleaseOnly = argv.includes('--live-split-release-only');
  const liveSplitScrollOnly = argv.includes('--live-split-scroll-only');
  const liveSplitInputOnly = argv.includes('--live-split-input-only');
  const liveSplitOrdinaryOnly = argv.includes('--live-split-ordinary-only');
  const liveSplitPreviewOnly = argv.includes('--live-split-preview-only');
  const liveSplitTraceOnly = argv.includes('--live-split-trace-only');
  const liveSplitArticleOnly = argv.includes('--live-split-article-only');
  const extensionIpcOnly = argv.includes('--extension-ipc-only');
  const extensionDiagramsOnly = argv.includes('--extension-diagrams-only');
  const extensionExportsOnly = argv.includes('--extension-exports-only');
  const extensionStartupOnly = argv.includes('--extension-startup-only');
  const extensionDiagramCase = extensionDiagramCaseIndex >= 0 ? argv[extensionDiagramCaseIndex + 1] : null;
  const traceEditorOnly = argv.includes('--trace-editor-only');
  const traceChunks = argv.includes('--trace-chunks');
  const traceNestedChunks = argv.includes('--trace-nested-chunks');
  const traceVisibleOnly = argv.includes('--trace-visible-only');
  const traceNoToggleSample = argv.includes('--trace-no-toggle-sample');
  const tracePreviewContain = argv.includes('--trace-preview-contain');
  const diagnosePreview = argv.includes('--diagnose-preview');
  const traceChunkSizeIndex = argv.indexOf('--trace-chunk-size');
  const traceChunkSamplesIndex = argv.indexOf('--trace-chunk-samples');
  const traceLayoutIndex = argv.indexOf('--trace-layout');
  const expectLegacy = argv.includes('--expect-legacy');
  const previewResourcesOnly = argv.includes('--preview-resources-only');
  const pdfResourceIsolationOnly = argv.includes('--pdf-resource-isolation-only');
  const exportSemanticsOnly = argv.includes('--export-semantics-only');
  return {
    output: outputIndex >= 0
      ? resolve(argv[outputIndex + 1])
      : extensionIpcOnly
        ? defaultExtensionOutput
      : extensionDiagramsOnly
        ? defaultExtensionDiagramsOutput
      : extensionExportsOnly
        ? defaultExtensionExportsOutput
      : extensionStartupOnly
        ? defaultExtensionStartupOutput
      : searchOnly
        ? defaultSearchOutput
        : mindMapOnly
          ? defaultMindMapOutput
          : interactionOnly
            ? defaultInteractionOutput
            : liveSplitOnly
              ? liveSplitMixedOnly ? defaultLiveSplitMixedOutput : defaultLiveSplitOutput
              : liveSplitIpcOnly
                ? defaultLiveSplitIpcOutput
                : liveSplitReleaseOnly
                  ? defaultLiveSplitReleaseOutput
                  : liveSplitScrollOnly
                    ? defaultLiveSplitScrollOutput
                    : liveSplitInputOnly
                      ? defaultLiveSplitInputOutput
                      : liveSplitOrdinaryOnly
                        ? defaultLiveSplitOrdinaryOutput
                        : liveSplitPreviewOnly
                          ? defaultLiveSplitPreviewOutput
                      : liveSplitTraceOnly
                        ? defaultLiveSplitTraceOutput
                        : liveSplitArticleOnly
                          ? defaultLiveSplitArticleOutput
            : previewResourcesOnly
              ? defaultPreviewResourceOutput
              : pdfResourceIsolationOnly
                ? defaultPdfResourceOutput
                : exportSemanticsOnly
                  ? defaultExportSemanticsOutput
          : defaultOutput,
    executable: executableIndex >= 0 ? resolve(argv[executableIndex + 1]) : executablePath,
    startupSamples: startupIndex >= 0 ? Number(argv[startupIndex + 1]) : startupSamples,
    operationSamples: operationIndex >= 0 ? Number(argv[operationIndex + 1]) : operationSamples,
    corpusLimit: corpusLimitIndex >= 0 ? Number(argv[corpusLimitIndex + 1]) : null,
    liveSplitCase: liveSplitCaseIndex >= 0 ? argv[liveSplitCaseIndex + 1] : null,
    searchOnly,
    mindMapOnly,
    interactionOnly,
    liveSplitOnly,
    liveSplitMixedOnly,
    liveSplitIpcOnly,
    liveSplitReleaseOnly,
    liveSplitScrollOnly,
    liveSplitInputOnly,
    liveSplitOrdinaryOnly,
    liveSplitPreviewOnly,
    liveSplitTraceOnly,
    liveSplitArticleOnly,
    extensionIpcOnly,
    extensionDiagramsOnly,
    extensionExportsOnly,
    extensionStartupOnly,
    extensionDiagramCase,
    traceEditorOnly,
    traceChunks,
    traceNestedChunks,
    traceVisibleOnly,
    traceNoToggleSample,
    tracePreviewContain,
    diagnosePreview,
    traceChunkSize: traceChunkSizeIndex >= 0 ? Number(argv[traceChunkSizeIndex + 1]) : 48,
    traceChunkSamples: traceChunkSamplesIndex >= 0 ? Number(argv[traceChunkSamplesIndex + 1]) : 24,
    traceLayout: traceLayoutIndex >= 0 ? argv[traceLayoutIndex + 1] : 'split',
    expectLegacy,
    previewResourcesOnly,
    pdfResourceIsolationOnly,
    exportSemanticsOnly
  };
}

async function command(file, args = []) {
  const { stdout } = await execFileAsync(file, args, { cwd: repositoryRoot, windowsHide: true });
  return stdout.trim();
}

async function unusedLoopbackPort() {
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  const address = server.address();
  await new Promise((resolveClose) => server.close(resolveClose));
  if (!address || typeof address === 'string') throw new Error('Could not allocate a loopback CDP port');
  return address.port;
}

async function waitForTargets(port, timeoutMs = 30_000) {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      if (response.ok) {
        const targets = await response.json();
        const target = targets.find((item) => item.type === 'page' && item.webSocketDebuggerUrl);
        if (target) return target;
      }
    } catch {
      // The WebView2 browser process has not bound the loopback port yet.
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 25));
  }
  throw new Error(`Timed out waiting for WebView2 CDP on port ${port}`);
}

class CdpClient {
  constructor(url) {
    this.socket = new WebSocket(url);
    this.nextId = 1;
    this.pending = new Map();
  }

  async connect() {
    await new Promise((resolveConnect, reject) => {
      this.socket.addEventListener('open', resolveConnect, { once: true });
      this.socket.addEventListener('error', reject, { once: true });
    });
    this.socket.addEventListener('message', (event) => {
      const message = JSON.parse(String(event.data));
      if (!message.id) return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
    });
  }

  send(method, params = {}, timeoutMs = 30_000) {
    const id = this.nextId++;
    return new Promise((resolveSend, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP ${method} timed out after ${timeoutMs} ms`));
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (value) => { clearTimeout(timer); resolveSend(value); },
        reject: (error) => { clearTimeout(timer); reject(error); }
      });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression, awaitPromise = true, timeoutMs = 30_000) {
    const result = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise,
      returnByValue: true
    }, timeoutMs);
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    }
    return result.result.value;
  }

  close() {
    this.socket.close();
  }
}

async function processResources(pid) {
  const script = `Get-Process -Id ${pid} | Select-Object WorkingSet64,HandleCount | ConvertTo-Json -Compress`;
  try {
    return JSON.parse(await command('powershell.exe', ['-NoProfile', '-Command', script]));
  } catch {
    return { WorkingSet64: null, HandleCount: null };
  }
}

async function processTreeResources(pid) {
  const script = `$all = @(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId); $ids = [System.Collections.Generic.HashSet[int]]::new(); $null = $ids.Add(${pid}); do { $before = $ids.Count; foreach ($p in $all) { if ($ids.Contains([int]$p.ParentProcessId)) { $null = $ids.Add([int]$p.ProcessId) } } } while ($ids.Count -gt $before); $rows = @($ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,WorkingSet64,HandleCount }); @($rows) | ConvertTo-Json -Compress`;
  try {
    const rows = JSON.parse(await command('powershell.exe', ['-NoProfile', '-Command', script]));
    const processes = Array.isArray(rows) ? rows : [rows];
    return {
      count: processes.length,
      workingSetSumBytes: processes.reduce((sum, process) => sum + (process.WorkingSet64 ?? 0), 0),
      largestWebView2Bytes: Math.max(0, ...processes
        .filter((process) => /msedgewebview2/i.test(process.ProcessName))
        .map((process) => process.WorkingSet64 ?? 0)),
      largest: processes.sort((left, right) => (right.WorkingSet64 ?? 0) - (left.WorkingSet64 ?? 0)).slice(0, 5)
    };
  } catch (error) {
    return { error: String(error) };
  }
}

function startProcessTreeSampler(pid) {
  const samples = [];
  let active = true;
  const startedAt = performance.now();
  const task = (async () => {
    while (active) {
      const resources = await processTreeResources(pid);
      if (resources.workingSetSumBytes !== undefined) {
        samples.push({ elapsedMs: Math.round(performance.now() - startedAt), ...resources });
      }
      if (active) await new Promise((resolveWait) => setTimeout(resolveWait, 250));
    }
  })();
  return { async stop() {
    active = false;
    await task;
    return {
      intervalMs: 250,
      sampleCount: samples.length,
      peakProcessTreeBytes: Math.max(0, ...samples.map((sample) => sample.workingSetSumBytes)),
      peakWebView2Bytes: Math.max(0, ...samples.map((sample) => sample.largestWebView2Bytes)),
      samples
    };
  } };
}

async function measureVirtualPreviewViewport(client, pid) {
  const checkpoints = [];
  for (const ratio of [0, 0.5, 1]) {
    const viewport = await client.evaluate(`(async () => {
      const host = document.querySelector('.preview-content');
      if (!host) throw new Error('Preview host is missing');
      const requested = ${ratio};
      const started = performance.now();
      host.scrollTop = (host.scrollHeight - host.clientHeight) * requested;
      const immediateScrollTop = host.scrollTop;
      let visible = [];
      while (performance.now() - started < 5000) {
        const bounds = host.getBoundingClientRect();
        visible = [...host.querySelectorAll('[data-preview-segment]')].filter((segment) => {
          const rect = segment.getBoundingClientRect();
          return rect.bottom > bounds.top && rect.top < bounds.bottom;
        });
        if (visible.length) break;
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      await new Promise(requestAnimationFrame);
      return {
        ratio: requested,
        immediateScrollTop,
        waitMs: performance.now() - started,
        scrollTop: host.scrollTop,
        scrollHeight: host.scrollHeight,
        clientHeight: host.clientHeight,
        nodes: host.querySelectorAll('*').length,
        mountedSegments: host.querySelectorAll('[data-preview-segment]').length,
        visibleSegments: visible.length,
        firstMounted: host.querySelector('[data-preview-segment]')?.getAttribute('data-preview-segment') ?? null,
        lastMounted: [...host.querySelectorAll('[data-preview-segment]')].at(-1)?.getAttribute('data-preview-segment') ?? null
      };
    })()`, true, 10_000);
    const processTree = await processTreeResources(pid);
    checkpoints.push({ ...viewport, processTree });
  }
  return { checkpoints };
}

function waitForChildExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve(true);
  return new Promise((resolveWait) => {
    const finish = () => {
      clearTimeout(timer);
      child.off('exit', onExit);
      resolveWait(true);
    };
    const onExit = () => finish();
    const timer = setTimeout(() => {
      child.off('exit', onExit);
      resolveWait(false);
    }, timeoutMs);
    child.once('exit', onExit);
  });
}

async function terminateChild(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill();
  if (await waitForChildExit(child, 2_000)) return;

  if (platform() === 'win32') {
    try {
      await execFileAsync('taskkill.exe', ['/pid', String(child.pid), '/T', '/F'], { windowsHide: true });
    } catch {
      // A racing natural exit also makes taskkill report that the PID is absent.
    }
  } else {
    child.kill('SIGKILL');
  }

  if (!(await waitForChildExit(child, 2_000))) {
    throw new Error(`Benchmark child ${child.pid} did not exit after forced termination`);
  }
}

async function launch(executable, corpusPath, port, runtimeDirectory, webviewDirectory = resolve(runtimeDirectory, 'webview2'), onSpawn) {
  const startedAt = performance.now();
  const child = spawn(executable, [corpusPath], {
    cwd: repositoryRoot,
    env: {
      ...process.env,
      MARKLITE_BENCHMARK_MODE: '1',
      MARKLITE_BENCHMARK_DATA_DIR: resolve(runtimeDirectory, 'appdata'),
      WEBVIEW2_USER_DATA_FOLDER: webviewDirectory,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`
    },
    stdio: 'ignore',
    windowsHide: false
  });
  onSpawn?.(child);
  let client;
  try {
    const target = await waitForTargets(port);
    client = new CdpClient(target.webSocketDebuggerUrl);
    await client.connect();
    const deadline = performance.now() + 90_000;
    while (performance.now() < deadline) {
      try {
        const ready = await client.evaluate(`(() => ({
          interactive: Boolean(document.querySelector('.cm-content')) && document.querySelector('.editor-stage')?.getAttribute('aria-busy') === 'false',
          userAgent: navigator.userAgent,
          title: document.title
        }))()`, true, 90_000);
        if (ready.interactive) {
          await client.evaluate('(async () => { await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame); })()');
          return { child, client, ready, startupMs: performance.now() - startedAt };
        }
      } catch (error) {
        if (!String(error).includes('Execution context was destroyed')) throw error;
      }
      await new Promise((resolveWait) => setTimeout(resolveWait, 20));
    }
    throw new Error(`MarkLite did not become interactive after ${Math.round(performance.now() - startedAt)} ms`);
  } catch (error) {
    client?.close();
    await terminateChild(child);
    throw error;
  }
}

async function stop(run) {
  try {
    await run.client.evaluate(`(() => {
      setTimeout(() => window.__TAURI_INTERNALS__.invoke('plugin:window|destroy', { label: 'main' }), 0);
      return true;
    })()`);
  } catch {
    // The WebView may close before the acknowledgement reaches CDP.
  }
  run.client.close();
  if (!(await waitForChildExit(run.child, 2_000))) await terminateChild(run.child);
}

async function measureFrames(client, selector, property, sampleCount = operationSamples) {
  return client.evaluate(`(async () => {
    const target = document.querySelector(${JSON.stringify(selector)});
    if (!target) throw new Error('Missing frame target: ' + ${JSON.stringify(selector)});
    const longTasks = [];
    const observer = new PerformanceObserver(entries => {
      for (const entry of entries.getEntries()) longTasks.push(entry.duration);
    });
    try { observer.observe({ type: 'longtask' }); } catch {}
    const frames = [];
    let previous = performance.now();
    for (let index = 0; index <= ${sampleCount}; index += 1) {
      await new Promise(requestAnimationFrame);
      const now = performance.now();
      frames.push(now - previous);
      previous = now;
      ${property === 'scroll'
        ? `target.scrollTop = (target.scrollHeight - target.clientHeight) * ((index % 10) / 9);`
        : `target.style.setProperty('--split-left', (35 + index % 30) + '%'); target.style.setProperty('--split-right', (65 - index % 30) + '%');`}
    }
    await new Promise(requestAnimationFrame);
    observer.disconnect();
    return { frames: frames.slice(1), longTasks };
  })()`, true, Math.max(30_000, sampleCount * 5_000));
}

async function measureInputToPaint(client, sampleCount = operationSamples, diagnose = false) {
  const values = [];
  const longTasks = [];
  await client.send('Runtime.enable');
  const box = await client.evaluate(`(() => {
    const rect = document.querySelector('.cm-content').getBoundingClientRect();
    return { x: rect.left + Math.min(40, rect.width / 2), y: rect.top + Math.min(40, rect.height / 2) };
  })()`);
  await client.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: box.x, y: box.y, button: 'left', clickCount: 1 });
  await client.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: box.x, y: box.y, button: 'left', clickCount: 1 });
  await client.evaluate("document.querySelector('.cm-content').focus()");

  for (let index = 0; index < sampleCount; index += 1) {
    if (diagnose) process.stderr.write(`preview sample ${index + 1}/${sampleCount} start\n`);
    const beforeLength = await client.evaluate("document.querySelector('.cm-content').textContent.length");
    await client.evaluate(`(() => {
      window.__markliteBenchResult = new Promise(resolve => {
        const startedAt = performance.now();
        const longTasks = [];
        const observer = new PerformanceObserver(entries => {
          for (const entry of entries.getEntries()) longTasks.push(entry.duration);
        });
        try { observer.observe({ type: 'longtask' }); } catch {}
        const preview = document.querySelector('.preview-content');
        const beforePreviewLength = preview?.textContent?.length ?? 0;
        const finish = (converged) => requestAnimationFrame(() => requestAnimationFrame(() => {
          observer.disconnect();
          const elapsedMs = performance.now() - startedAt;
          resolve({ elapsedMs, longTasks, converged: converged && elapsedMs <= 12000 });
        }));
        if (!preview) return finish(false);
        const mutation = new MutationObserver(() => {
          if ((preview.textContent?.length ?? 0) <= beforePreviewLength) return;
          mutation.disconnect();
          clearTimeout(timeout);
          finish(true);
        });
        mutation.observe(preview, { childList: true, subtree: true, characterData: true });
        const timeout = setTimeout(() => { mutation.disconnect(); finish(false); }, 12000);
      });
    })()`, false);
    await client.send('Input.insertText', { text: String.fromCharCode(97 + index % 26) });
    const afterLength = await client.evaluate("document.querySelector('.cm-content').textContent.length");
    if (afterLength !== beforeLength + 1) {
      throw new Error(`CDP insertion did not change editor content (${beforeLength} -> ${afterLength})`);
    }
    const result = await client.evaluate('window.__markliteBenchResult');
    if (diagnose) process.stderr.write(`preview sample ${index + 1}/${sampleCount} ${Math.round(result.elapsedMs)} ms, converged=${result.converged}\n`);
    values.push(result.elapsedMs);
    longTasks.push(...result.longTasks);
    if (!result.converged) throw new Error(`Preview did not converge for input sample ${index + 1} within 12 seconds`);
  }
  return { values, longTasks };
}

async function traceLiveSplitInput(client, editorOnly) {
  const totals = new Map();
  const longest = [];
  const timeline = [];
  let complete;
  let traceTimer;
  const completed = new Promise((resolveDone, rejectDone) => {
    traceTimer = setTimeout(() => rejectDone(new Error('CDP trace did not complete within 30 seconds')), 30000);
    complete = () => { clearTimeout(traceTimer); resolveDone(); };
  });
  const onMessage = (messageEvent) => {
    const message = JSON.parse(String(messageEvent.data));
    if (message.method === 'Tracing.tracingComplete') complete();
    if (message.method !== 'Tracing.dataCollected') return;
    for (const event of message.params.value) {
      if (event.ph !== 'X' || typeof event.dur !== 'number') continue;
      const durationMs = event.dur / 1000;
      timeline.push({ name: event.name, startUs: event.ts, endUs: event.ts + event.dur, thread: event.tid });
      const current = totals.get(event.name) ?? { count: 0, totalMs: 0, maxMs: 0 };
      current.count += 1;
      current.totalMs += durationMs;
      current.maxMs = Math.max(current.maxMs, durationMs);
      totals.set(event.name, current);
      if (durationMs >= 20) longest.push({ name: event.name, durationMs, thread: event.tid });
    }
  };
  client.socket.addEventListener('message', onMessage);
  try {
    await client.send('Tracing.start', {
      categories: 'devtools.timeline,v8,blink,cc,disabled-by-default-devtools.timeline',
      transferMode: 'ReportEvents'
    });
    const input = editorOnly ? await measureEditorPaint(client, 1) : await measureInputToPaint(client, 1);
    await client.send('Tracing.end');
    await completed;
    const layouts = timeline
      .filter((event) => event.name === 'Layout' && event.endUs - event.startUs >= 20000)
      .sort((left, right) => right.endUs - right.startUs - (left.endUs - left.startUs))
      .slice(0, 10)
      .map((layout) => ({
        durationMs: (layout.endUs - layout.startUs) / 1000,
        ancestors: timeline
          .filter((event) => event.thread === layout.thread && event.startUs <= layout.startUs && event.endUs >= layout.endUs)
          .sort((left, right) => (left.endUs - left.startUs) - (right.endUs - right.startUs))
          .slice(0, 15)
          .map((event) => event.name)
      }));
    return {
      input,
      categories: Object.fromEntries([...totals].sort((left, right) => right[1].totalMs - left[1].totalMs).slice(0, 40)),
      longest: longest.sort((left, right) => right.durationMs - left.durationMs).slice(0, 40),
      layouts
    };
  } finally {
    clearTimeout(traceTimer);
    client.socket.removeEventListener('message', onMessage);
  }
}

async function measureEditorPaint(client, sampleCount = operationSamples) {
  const values = [];
  const longTasks = [];
  await client.send('Runtime.enable');
  const box = await client.evaluate(`(() => {
    const rect = document.querySelector('.cm-content').getBoundingClientRect();
    return { x: rect.left + Math.min(40, rect.width / 2), y: rect.top + Math.min(40, rect.height / 2) };
  })()`);
  await client.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: box.x, y: box.y, button: 'left', clickCount: 1 });
  await client.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: box.x, y: box.y, button: 'left', clickCount: 1 });
  await client.evaluate("document.querySelector('.cm-content').focus()");

  for (let index = 0; index < sampleCount; index += 1) {
    const beforeLength = await client.evaluate("document.querySelector('.cm-content').textContent.length");
    await client.evaluate(`(() => {
      window.__markliteEditorPaint = new Promise(resolve => {
        const startedAt = performance.now();
        const observed = [];
        const observer = new PerformanceObserver(entries => {
          for (const entry of entries.getEntries()) observed.push(entry.duration);
        });
        try { observer.observe({ type: 'longtask' }); } catch {}
        requestAnimationFrame(() => requestAnimationFrame(() => {
          observer.disconnect();
          resolve({ elapsedMs: performance.now() - startedAt, longTasks: observed });
        }));
      });
    })()`, false);
    await client.send('Input.insertText', { text: String.fromCharCode(97 + index % 26) });
    const afterLength = await client.evaluate("document.querySelector('.cm-content').textContent.length");
    if (afterLength !== beforeLength + 1) {
      throw new Error(`CDP insertion did not change editor content (${beforeLength} -> ${afterLength})`);
    }
    const result = await client.evaluate('window.__markliteEditorPaint');
    values.push(result.elapsedMs);
    longTasks.push(...result.longTasks);
  }
  return { values, longTasks };
}

async function selectLayout(client, mode) {
  await client.evaluate(`(() => {
    const index = ({ edit: 0, split: 1, preview: 2 })[${JSON.stringify(mode)}];
    const button = document.querySelectorAll('.layout-switch button')[index];
    if (!button) throw new Error('Missing layout button: ' + ${JSON.stringify(mode)});
    button.click();
  })()`);
  await client.evaluate('(async () => { await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame); })()');
}

async function measureDividerGhost(client, sampleCount = operationSamples, expectLegacy = false) {
  const geometry = await client.evaluate(`(() => {
    const separator = document.querySelector('.split-separator');
    const stage = document.querySelector('.editor-stage');
    if (!separator || !stage) throw new Error('Missing split interaction surface');
    const rect = separator.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + Math.min(80, rect.height / 2), stageStyle: stage.getAttribute('style') };
  })()`);
  await client.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: geometry.x, y: geometry.y, button: 'left', clickCount: 1 });
  const values = [];
  for (let index = 0; index < sampleCount; index += 1) {
    const x = geometry.x + 80 * Math.sin((index / Math.max(1, sampleCount - 1)) * Math.PI * 2);
    await client.evaluate('window.__markliteDividerFrameStart = performance.now()', false);
    await client.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y: geometry.y, button: 'left' });
    values.push(await client.evaluate('(async () => { await new Promise(requestAnimationFrame); return performance.now() - window.__markliteDividerFrameStart; })()'));
  }
  await client.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: geometry.x + 80, y: geometry.y, button: 'left' });
  await client.evaluate('(async () => { await new Promise(requestAnimationFrame); })()');
  const during = await client.evaluate(`(() => ({
    stageStyle: document.querySelector('.editor-stage').getAttribute('style'),
    previewOffset: document.querySelector('.split-separator').style.getPropertyValue('--split-preview-offset')
  }))()`);
  await client.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: geometry.x + 50, y: geometry.y, button: 'left', clickCount: 1 });
  await client.evaluate('(async () => { await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame); })()');
  const after = await client.evaluate(`(() => ({
    stageStyle: document.querySelector('.editor-stage').getAttribute('style'),
    previewOffset: document.querySelector('.split-separator').style.getPropertyValue('--split-preview-offset')
  }))()`);
  if (!expectLegacy) {
    if (during.stageStyle !== geometry.stageStyle || during.previewOffset === '0px') {
      throw new Error('Divider drag changed the real grid or failed to move the ghost separator');
    }
    if (after.stageStyle === geometry.stageStyle || after.previewOffset !== '0px') {
      throw new Error('Divider release did not commit exactly once and clear its ghost offset');
    }
  }
  return { frames: values, before: geometry, during, after };
}

async function measureScrollAlignment(client, widthPercent = null, previewFontPx = null) {
  return client.evaluate(`(async () => {
    const editor = document.querySelector('.cm-scroller');
    const preview = document.querySelector('.preview-content');
    if (!editor || !preview) throw new Error('Missing split scroll surface');
    const widthPercent = ${JSON.stringify(widthPercent)};
    const previewFontPx = ${JSON.stringify(previewFontPx)};
    if (widthPercent !== null) {
      const stage = document.querySelector('.editor-stage');
      stage.style.setProperty('--split-left', widthPercent + '%');
      stage.style.setProperty('--split-right', (100 - widthPercent) + '%');
    }
    if (previewFontPx !== null) preview.style.fontSize = previewFontPx + 'px';
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    const samples = [];
    const marker = (surface, selector) => {
      const top = surface.getBoundingClientRect().top;
      const nodes = [...surface.querySelectorAll(selector)];
      const visible = nodes.find(node => node.getBoundingClientRect().bottom > top + 4);
      const match = visible?.textContent?.match(/MARKLITE-SCROLL-(\\d{5})/);
      return match ? Number(match[1]) : null;
    };
    for (const ratio of [0, 0.1, 0.25, 0.5, 0.75, 0.9, 1]) {
      editor.scrollTop = (editor.scrollHeight - editor.clientHeight) * ratio;
      await new Promise(requestAnimationFrame);
      await new Promise(requestAnimationFrame);
      await new Promise(resolve => setTimeout(resolve, 100));
      const sourceMarker = marker(editor, '.cm-line');
      const previewMarker = marker(preview, 'h1, h2');
      samples.push({ ratio, sourceMarker, previewMarker,
        markerError: sourceMarker === null || previewMarker === null ? null : previewMarker - sourceMarker,
        editorScrollTop: editor.scrollTop, previewScrollTop: preview.scrollTop,
        editorWidth: editor.getBoundingClientRect().width,
        previewWidth: preview.getBoundingClientRect().width,
        previewFontSize: getComputedStyle(preview).fontSize });
    }
    return samples;
  })()`, true, 30000);
}

async function measureDividerRelease(client, sampleCount) {
  await client.evaluate(`(() => {
    const internals = window.__TAURI_INTERNALS__;
    const original = internals.invoke;
    window.__markliteRenderCount = 0;
    window.__markliteOriginalInvoke = original;
    internals.invoke = function(command, ...args) {
      if (command === 'render_markdown') window.__markliteRenderCount += 1;
      return original.call(this, command, ...args);
    };
  })()`);
  const values = [];
  let failure = null;
  try {
    for (let index = 0; index < sampleCount; index += 1) {
      const geometry = await client.evaluate(`(() => {
        const rect = document.querySelector('.split-separator')?.getBoundingClientRect();
        if (!rect) throw new Error('Missing split separator');
        return { x: rect.left + rect.width / 2, y: rect.top + Math.min(80, rect.height / 2) };
      })()`);
      const targetX = geometry.x + (index % 2 === 0 ? 35 : -35);
      await client.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: geometry.x, y: geometry.y, button: 'left', clickCount: 1 });
      await client.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: targetX, y: geometry.y, button: 'left' });
      await client.evaluate('window.__markliteReleaseStarted = performance.now()', false);
      await client.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: targetX, y: geometry.y, button: 'left', clickCount: 1 });
      try {
        values.push(await client.evaluate(`(async () => {
        const stage = document.querySelector('.editor-stage');
        const editor = document.querySelector('.cm-scroller');
        const preview = document.querySelector('.preview-content');
        let previous = null;
        let stableFrames = 0;
        while (performance.now() - window.__markliteReleaseStarted < 5000) {
          await new Promise(requestAnimationFrame);
          const widths = [stage?.getBoundingClientRect().width, editor?.getBoundingClientRect().width, preview?.getBoundingClientRect().width];
          if (previous && widths.every((width, i) => Number.isFinite(width) && Math.abs(width - previous[i]) < 0.1)) stableFrames += 1;
          else stableFrames = 0;
          previous = widths;
          if (stableFrames >= 3) return performance.now() - window.__markliteReleaseStarted;
        }
        throw new Error('Split layout did not stabilize within 5 seconds after pointerup');
      })()`, true, 10000));
      } catch (error) {
        failure = { sample: index + 1, error: String(error) };
        break;
      }
    }
    return { values, failure, renderCount: await client.evaluate('window.__markliteRenderCount') };
  } finally {
    await client.evaluate(`(() => {
      window.__TAURI_INTERNALS__.invoke = window.__markliteOriginalInvoke;
      delete window.__markliteOriginalInvoke;
      delete window.__markliteRenderCount;
    })()`).catch(() => undefined);
  }
}

async function measureLiveSplitIpc(client, source, sampleCount) {
  try {
    await client.evaluate(`window.__markliteIpcSource = ${JSON.stringify(source)}`, false, 120000);
  } catch (error) {
    throw new Error(`Source injection failed: ${error}`);
  }
  const ipc = [];
  for (let index = 0; index < sampleCount; index += 1) {
    try {
      ipc.push(await client.evaluate(`(async () => {
      const startedAt = performance.now();
      const rendered = await window.__TAURI_INTERNALS__.invoke('render_markdown', { content: window.__markliteIpcSource });
      const ipcMs = performance.now() - startedAt;
      if (!rendered?.html) throw new Error('render_markdown returned no HTML');
      window.__markliteIpcHtml = rendered.html;
      return { ipcMs, htmlBytes: new Blob([rendered.html]).size, outlineCount: rendered.outline?.length ?? null };
    })()`, true, 120000));
    } catch (error) {
      throw new Error(`IPC sample ${index + 1} failed: ${error}`);
    }
  }
  let preparation;
  try {
    preparation = await client.evaluate(`(async () => {
    const values = [];
    for (let index = 0; index < 3; index += 1) {
      const template = document.createElement('template');
      const startedAt = performance.now();
      template.innerHTML = window.__markliteIpcHtml;
      values.push({ parseMs: performance.now() - startedAt, nodes: template.content.querySelectorAll('*').length });
      template.innerHTML = '';
    }
    delete window.__markliteIpcSource;
    delete window.__markliteIpcHtml;
    return values;
  })()`, true, 120000);
  } catch (error) {
    throw new Error(`HTML preparation failed: ${error}`);
  }
  return { ipc, preparation };
}

async function measureLiveSplitInputPatterns(client) {
  const box = await client.evaluate(`(() => {
    const rect = document.querySelector('.cm-content').getBoundingClientRect();
    return { x: rect.left + Math.min(40, rect.width / 2), y: rect.top + Math.min(40, rect.height / 2) };
  })()`);
  await client.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: box.x, y: box.y, button: 'left', clickCount: 1 });
  await client.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: box.x, y: box.y, button: 'left', clickCount: 1 });
  await client.evaluate(`(() => {
    document.querySelector('.cm-content').focus();
    window.__markliteCompositionEvents = [];
    const internals = window.__TAURI_INTERNALS__;
    const originalInvoke = internals.invoke;
    window.__markliteInputRenderCount = 0;
    internals.invoke = function(command, ...args) {
      if (command === 'render_markdown') window.__markliteInputRenderCount += 1;
      return originalInvoke.call(this, command, ...args);
    };
    for (const type of ['compositionstart', 'compositionupdate', 'compositionend']) {
      document.addEventListener(type, () => window.__markliteCompositionEvents.push(type), { capture: true });
    }
  })()`);
  const beforeIme = await client.evaluate("document.querySelector('.cm-content').textContent.length");
  await client.send('Input.imeSetComposition', { text: '中文', selectionStart: 2, selectionEnd: 2 });
  await client.send('Input.insertText', { text: '中文' });
  const ime = await client.evaluate(`(async () => {
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    return { visibleDelta: document.querySelector('.cm-content').textContent.length - ${beforeIme},
      events: window.__markliteCompositionEvents };
  })()`);
  const paste = await client.evaluate(`(async () => {
    const content = document.querySelector('.cm-content');
    const before = content.textContent.length;
    const transfer = new DataTransfer();
    transfer.setData('text/plain', 'PASTE-PROBE-' + 'x'.repeat(2048));
    const event = new ClipboardEvent('paste', { bubbles: true, cancelable: true, clipboardData: transfer });
    content.dispatchEvent(event);
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    return { eventTrusted: event.isTrusted, defaultPrevented: event.defaultPrevented,
      visibleDelta: content.textContent.length - before };
  })()`);
  const beforeBulk = await client.evaluate("document.querySelector('.cm-content').textContent.length");
  await client.evaluate('window.__markliteBulkStarted = performance.now()', false);
  await client.send('Input.insertText', { text: 'BULK-PROBE-' + 'y'.repeat(2048) });
  const bulk = await client.evaluate(`(async () => {
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    return { editorPaintMs: performance.now() - window.__markliteBulkStarted,
      visibleDelta: document.querySelector('.cm-content').textContent.length - ${beforeBulk} };
  })()`);
  const postInputDragStart = await client.evaluate(`(() => {
    const rect = document.querySelector('.split-separator').getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + Math.min(80, rect.height / 2),
      renderBefore: window.__markliteInputRenderCount };
  })()`);
  await client.send('Input.insertText', { text: 'q' });
  await client.send('Input.dispatchMouseEvent', {
    type: 'mousePressed', x: postInputDragStart.x, y: postInputDragStart.y, button: 'left', clickCount: 1
  });
  await client.evaluate('window.__marklitePostInputDragStarted = performance.now()', false);
  await client.send('Input.dispatchMouseEvent', {
    type: 'mouseMoved', x: postInputDragStart.x + 35, y: postInputDragStart.y, button: 'left'
  });
  const postInputDragFrameMs = await client.evaluate(`(async () => {
    await new Promise(requestAnimationFrame);
    return performance.now() - window.__marklitePostInputDragStarted;
  })()`);
  await client.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased', x: postInputDragStart.x + 35, y: postInputDragStart.y, button: 'left', clickCount: 1
  });
  const postInputDrag = await client.evaluate(`(async () => {
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    return { frameMs: ${postInputDragFrameMs}, renderBefore: ${postInputDragStart.renderBefore},
      renderAfter: window.__markliteInputRenderCount };
  })()`);
  await client.evaluate(`(() => {
    const preview = document.querySelector('.preview-content');
    const before = preview?.textContent?.length ?? 0;
    window.__markliteBurstStarted = performance.now();
    window.__markliteBurstPreview = new Promise(resolve => {
      if (!preview) return resolve({ converged: false, elapsedMs: null });
      const observer = new MutationObserver(() => {
        if ((preview.textContent?.length ?? 0) < before + 20) return;
        observer.disconnect();
        clearTimeout(timer);
        resolve({ converged: true, elapsedMs: performance.now() - window.__markliteBurstStarted });
      });
      observer.observe(preview, { childList: true, subtree: true, characterData: true });
      const timer = setTimeout(() => { observer.disconnect(); resolve({ converged: false, elapsedMs: performance.now() - window.__markliteBurstStarted }); }, 12000);
    });
  })()`, false);
  const beforeBurst = await client.evaluate("document.querySelector('.cm-content').textContent.length");
  for (let index = 0; index < 20; index += 1) {
    await client.send('Input.insertText', { text: 'z' });
  }
  const burst = await client.evaluate(`(async () => {
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    return { editorPaintMs: performance.now() - window.__markliteBurstStarted,
      visibleDelta: document.querySelector('.cm-content').textContent.length - ${beforeBurst} };
  })()`);
  burst.preview = await client.evaluate('window.__markliteBurstPreview', true, 30000);
  return { ime, paste, bulk, postInputDrag, burst };
}

async function measureDenseSearch(client) {
  process.stdout.write('dense search: open panel\n');
  await client.send('Runtime.enable');
  await client.evaluate(`(() => {
    window.__markliteSearchErrors = [];
    window.addEventListener('error', event => window.__markliteSearchErrors.push(String(event.error ?? event.message)));
    window.addEventListener('unhandledrejection', event => window.__markliteSearchErrors.push(String(event.reason)));
  })()`);
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyDown', key: 'f', code: 'KeyF', modifiers: 2
  });
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyUp', key: 'f', code: 'KeyF', modifiers: 2
  });
  await client.evaluate(`(async () => {
    const deadline = performance.now() + 5000;
    while (!document.querySelector('.find-popover input')) {
      if (performance.now() >= deadline) throw new Error('Search input did not open');
      await new Promise(resolve => setTimeout(resolve, 10));
    }
  })()`);

  await client.evaluate(`(() => {
    window.__markliteSearchReady = new Promise((resolve, reject) => {
      const startedAt = performance.now();
      const longTasks = [];
      const observer = new PerformanceObserver(entries => {
        for (const entry of entries.getEntries()) longTasks.push(entry.duration);
      });
      try { observer.observe({ type: 'longtask' }); } catch {}
      const deadline = setTimeout(() => {
        observer.disconnect();
        reject(new Error('Dense search did not produce a match counter'));
      }, 30000);
      const form = document.querySelector('.find-popover');
      const mutation = new MutationObserver(() => {
        const counter = form?.querySelector('.find-input-shell span')?.textContent ?? '';
        if (!/^1\\/\\d+$/.test(counter)) return;
        mutation.disconnect();
        clearTimeout(deadline);
        requestAnimationFrame(() => requestAnimationFrame(() => {
          observer.disconnect();
          resolve({
            elapsedMs: performance.now() - startedAt,
            counter,
            decorations: document.querySelectorAll('.cm-searchMatch').length,
            longTasks
          });
        }));
      });
      mutation.observe(form, { childList: true, subtree: true, characterData: true });
    });
  })()`, false);
  process.stdout.write('dense search: collect matches\n');
  await client.send('Input.insertText', { text: 'SEARCH_TARGET' });
  const search = await client.evaluate('window.__markliteSearchReady');

  process.stdout.write('dense search: edit with results active\n');
  const box = await client.evaluate(`(() => {
    const rect = document.querySelector('.cm-content').getBoundingClientRect();
    return { x: rect.left + Math.min(40, rect.width / 2), y: rect.top + Math.min(40, rect.height / 2) };
  })()`);
  await client.send('Input.dispatchMouseEvent', {
    type: 'mousePressed', x: box.x, y: box.y, button: 'left', clickCount: 1
  });
  await client.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased', x: box.x, y: box.y, button: 'left', clickCount: 1
  });
  await client.evaluate(`(() => {
    window.__markliteSearchEdit = new Promise((resolve, reject) => {
      const startedAt = performance.now();
      const longTasks = [];
      const observer = new PerformanceObserver(entries => {
        for (const entry of entries.getEntries()) longTasks.push(entry.duration);
      });
      try { observer.observe({ type: 'longtask' }); } catch {}
      const deadline = setTimeout(() => {
        observer.disconnect();
        reject(new Error('Dense-search edit did not update the editor'));
      }, 30000);
      const content = document.querySelector('.cm-content');
      const mutation = new MutationObserver(() => {
        mutation.disconnect();
        clearTimeout(deadline);
        requestAnimationFrame(() => requestAnimationFrame(() => {
          observer.disconnect();
          resolve({
            elapsedMs: performance.now() - startedAt,
            counter: document.querySelector('.find-input-shell span')?.textContent ?? '',
            decorations: document.querySelectorAll('.cm-searchMatch').length,
            longTasks
          });
        }));
      });
      mutation.observe(content, { childList: true, subtree: true, characterData: true });
    });
  })()`, false);
  await client.send('Input.insertText', { text: 'x' });
  const edit = await client.evaluate('window.__markliteSearchEdit');
  process.stdout.write('dense search: scroll\n');
  const scroll = await measureFrames(client, '.cm-scroller', 'scroll', 10);
  const runtime = await client.evaluate(`(() => ({
    errors: window.__markliteSearchErrors,
    usedJsHeapBytes: performance.memory?.usedJSHeapSize ?? null,
    totalJsHeapBytes: performance.memory?.totalJSHeapSize ?? null,
    decorations: document.querySelectorAll('.cm-searchMatch').length,
    counter: document.querySelector('.find-input-shell span')?.textContent ?? ''
  }))()`);
  return { search, edit, scroll, runtime };
}

async function measureNativeRender(client, content) {
  return client.evaluate(`(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const content = ${JSON.stringify(content)};
    const values = [];
    for (let index = 0; index < ${operationSamples}; index += 1) {
      const startedAt = performance.now();
      await invoke('render_markdown', { content });
      values.push(performance.now() - startedAt);
    }
    return values;
  })()`);
}

async function measureExports(client, content, outputDirectory) {
  await mkdir(outputDirectory, { recursive: true });
  const results = {};
  for (const format of ['html', 'docx']) {
    const targets = Array.from({ length: operationSamples }, (_, index) =>
      resolve(outputDirectory, `${format}-${String(index).padStart(2, '0')}.${format}`)
    );
    results[format] = await client.evaluate(`(async () => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const content = ${JSON.stringify(content)};
      const targets = ${JSON.stringify(targets)};
      const values = [];
      for (let index = 0; index < targets.length; index += 1) {
        const startedAt = performance.now();
        await invoke('export_document', { request: {
          snapshot: {
            jobId: 'performance-' + ${JSON.stringify(format)} + '-' + index,
            tabId: 'performance-baseline',
            contentRevision: index,
            sourcePath: null,
            title: 'Performance baseline',
            content
          },
          targetPath: targets[index],
          format: ${JSON.stringify(format)},
          options: {
            paperSize: 'a4', orientation: 'portrait', margin: 'normal',
            includeTitle: true, includeLocalImages: false
          },
          mindMapSvg: null
        }});
        values.push(performance.now() - startedAt);
      }
      return values;
    })()`);
  }
  return results;
}

async function startupDiagnosticDurations(runtimeDirectory, count) {
  const directory = resolve(runtimeDirectory, 'appdata', 'diagnostics', 'startup');
  const names = (await readdir(directory)).filter((name) => name.endsWith('.jsonl')).sort().slice(0, count);
  const values = [];
  for (const name of names) {
    const rows = (await readFile(resolve(directory, name), 'utf8'))
      .trim()
      .split(/\r?\n/)
      .map((line) => JSON.parse(line));
    const start = rows[0];
    const ready = rows.findLast((row) => row.stage === 'frontendReady' && row.status === 'succeeded');
    if (start && ready) values.push(Date.parse(ready.timestamp) - Date.parse(start.timestamp));
  }
  return values;
}

async function measureMindMapWebView(client) {
  await client.send('Runtime.enable');
  await client.send('HeapProfiler.enable');
  await client.send('HeapProfiler.collectGarbage');
  const heapBefore = await client.send('Runtime.getHeapUsage');
  const switchResult = await client.evaluate(`(async () => {
    const errors = [];
    const captureError = event => errors.push(String(event.error ?? event.reason ?? event.message));
    window.addEventListener('error', captureError);
    window.addEventListener('unhandledrejection', captureError);
    const buttons = document.querySelectorAll('.preview-mode-switcher button');
    if (buttons.length !== 2) throw new Error('Missing preview mode controls');
    const startedAt = performance.now();
    buttons[1].click();
    const deadline = performance.now() + 30000;
    while (!document.querySelector('.mind-map-canvas')) {
      if (performance.now() > deadline) throw new Error('Mind map did not mount within 30 seconds');
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    const canvas = document.querySelector('.mind-map-canvas');
    return {
      switchMs: performance.now() - startedAt,
      totalLayoutNodes: Number(canvas.dataset.totalLayoutNodes),
      renderedLayoutNodes: Number(canvas.dataset.renderedLayoutNodes),
      canvasWidth: parseFloat(canvas.style.width),
      canvasHeight: parseFloat(canvas.style.height),
      errors
    };
  })()`, true, 45_000);
  const scroll = await measureFrames(client, '.mind-map-viewport', 'scroll', 10);
  const bottom = await client.evaluate(`(async () => {
    const viewport = document.querySelector('.mind-map-viewport');
    viewport.scrollTop = viewport.scrollHeight - viewport.clientHeight;
    viewport.scrollLeft = viewport.scrollWidth - viewport.clientWidth;
    await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
    const labels = [...document.querySelectorAll('.mind-map-node-label span')].map(node => node.textContent);
    return {
      renderedLayoutNodes: Number(document.querySelector('.mind-map-canvas').dataset.renderedLayoutNodes),
      hasLastHeading: labels.includes('Heading 150000 branch 24999')
    };
  })()`);
  await client.evaluate(`(async () => {
    document.querySelectorAll('.preview-mode-switcher button')[0].click();
    while (document.querySelector('.mind-map-canvas')) await new Promise(requestAnimationFrame);
    await new Promise(requestAnimationFrame);
  })()`);
  const releaseHeapSamples = [];
  for (let cycle = 0; cycle < 3; cycle += 1) {
    if (cycle) {
      await client.evaluate(`(async () => {
        const buttons = document.querySelectorAll('.preview-mode-switcher button');
        buttons[1].click();
        while (!document.querySelector('.mind-map-canvas')) await new Promise(requestAnimationFrame);
        buttons[0].click();
        while (document.querySelector('.mind-map-canvas')) await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
      })()`, true, 45_000);
    }
    await client.send('HeapProfiler.collectGarbage');
    releaseHeapSamples.push(await client.send('Runtime.getHeapUsage'));
  }
  return {
    ...switchResult,
    bottom,
    scrollFrames: summary(scroll.frames),
    scrollRawMs: scroll.frames,
    scrollLongTasks: summary(scroll.longTasks),
    heapBefore,
    releaseHeapSamples,
    heapAfterRelease: releaseHeapSamples.at(-1)
  };
}

async function createPreviewResourceFixture(runtimeDirectory) {
  const directory = resolve(runtimeDirectory, 'preview-resource-fixture');
  await mkdir(directory, { recursive: true });
  const imagePath = resolve(directory, 'pixel.png');
  const largeImagePath = resolve(directory, 'large.png');
  const markdownPath = resolve(directory, 'preview-resources.md');
  const image = Buffer.from(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=',
    'base64'
  );
  const aliases = Array.from({ length: 64 }, (_, index) => `${'./'.repeat(index + 1)}pixel.png`);
  const largeImage = createSolidPng(4_096, 4_096);
  const tableRows = Array.from({ length: 5_000 }, (_, index) => `| Row ${index + 1} | value-${index + 1} |`).join('\n');
  const markdown = [
    '# Preview resource lifecycle',
    '',
    '![Decoded budget sample](large.png)',
    '',
    ...aliases.map((alias, index) => `![Alias ${index + 1}](${alias})`),
    '',
    '[Local section](#long-table)',
    '',
    '```text',
    'const previewResource = true;'.repeat(1_000),
    '```',
    '',
    '## Long table',
    '',
    '| Name | Value |',
    '| --- | --- |',
    tableRows
  ].join('\n');
  await writeFile(imagePath, image);
  await writeFile(largeImagePath, largeImage);
  await writeFile(markdownPath, markdown, 'utf8');
  return {
    markdownPath,
    imagePath,
    largeImagePath,
    markdownBytes: Buffer.byteLength(markdown),
    imageBytes: image.byteLength,
    largeImageBytes: largeImage.byteLength,
    estimatedDecodedBytes: 4_096 * 4_096 * 4 + 4
  };
}

async function createImageScrollFixture(runtimeDirectory) {
  const directory = resolve(runtimeDirectory, 'image-scroll-fixture');
  await mkdir(directory, { recursive: true });
  const image = createSolidPng(600, 400);
  const imagePath = resolve(directory, 'image.png');
  const markdownPath = resolve(directory, 'scroll-image-markers.md');
  const lines = Array.from({ length: 1000 }, (_, index) =>
    `# MARKLITE-SCROLL-${String(index).padStart(5, '0')}${index === 500 ? ' ![Layout image](image.png)' : ''}`
  );
  const markdown = `${lines.join('\n')}\n`;
  await writeFile(imagePath, image);
  await writeFile(markdownPath, markdown, 'utf8');
  return {
    markdownPath,
    markdownBytes: Buffer.byteLength(markdown),
    markdownSha256: createHash('sha256').update(markdown).digest('hex'),
    imageBytes: image.byteLength,
    imageSha256: createHash('sha256').update(image).digest('hex')
  };
}

async function createPdfResourceFixture(runtimeDirectory, endpoint) {
  const directory = resolve(runtimeDirectory, 'pdf-resource-fixture');
  await mkdir(directory, { recursive: true });
  const markdownPath = resolve(directory, 'resource-policy.md');
  const imagePath = resolve(directory, 'pixel.png');
  const pdfPath = resolve(directory, 'resource-policy.pdf');
  const image = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=', 'base64');
  const markdown = [
    '# Resource policy',
    '',
    '![local](pixel.png)',
    '![alias](./pixel.png)',
    `![remote](${endpoint}/markdown.png)`,
    '',
    `<img src="${endpoint}/raw.png" srcset="${endpoint}/raw-1.png 1x, ${endpoint}/raw-2.png 2x" alt="raw">`,
    `<link rel="stylesheet" href="${endpoint}/style.css">`,
    `<style>@import url('${endpoint}/import.css'); body { background: url('${endpoint}/background.png') }</style>`,
    `<video poster="${endpoint}/poster.png"></video>`,
    `<object data="${endpoint}/object.bin"></object>`
  ].join('\n');
  await writeFile(markdownPath, markdown, 'utf8');
  await writeFile(imagePath, image);
  return { markdownPath, imagePath, pdfPath, markdown, image };
}

async function createExportSemanticsFixture() {
  const directory = resolve(repositoryRoot, 'tmp', 'pdfs');
  await mkdir(directory, { recursive: true });
  const markdownPath = resolve(directory, '0060-export-semantics.md');
  const markdown = [
    '# Semantic contract {#semantic-contract}',
    '',
    '- first **bold** item',
    '- second [external link](https://example.com/export)',
    '',
    'soft',
    'line  ',
    'hard[^note]',
    '',
    '| Left | Center | Right |',
    '| :--- | :----: | ----: |',
    '| alpha | beta | gamma |',
    '',
    '[^note]: **Bold footnote** and *italic detail*.'
  ].join('\n');
  await writeFile(markdownPath, markdown, 'utf8');
  return {
    directory,
    markdown,
    markdownPath,
    htmlPath: resolve(directory, '0060-export-semantics.html'),
    docxPath: resolve(directory, '0060-export-semantics.docx'),
    pdfPath: resolve(directory, '0060-export-semantics.pdf')
  };
}

async function startResourceCapture() {
  const requests = [];
  const server = createServer((request, response) => {
    requests.push({ method: request.method, url: request.url });
    response.writeHead(200, { 'content-type': 'image/png', 'cache-control': 'no-store' });
    response.end(Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=', 'base64'));
  });
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Resource capture did not bind a TCP port');
  return {
    endpoint: `http://127.0.0.1:${address.port}`,
    requests,
    close: () => new Promise((resolveClose, reject) => server.close((error) => error ? reject(error) : resolveClose()))
  };
}

function createSolidPng(width, height) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.set([8, 6, 0, 0, 0], 8);
  const rowBytes = width * 4 + 1;
  const pixels = Buffer.alloc(rowBytes * height);
  const idat = deflateSync(pixels, { level: 9 });
  return Buffer.concat([signature, pngChunk('IHDR', ihdr), pngChunk('IDAT', idat), pngChunk('IEND', Buffer.alloc(0))]);
}

function pngChunk(type, data) {
  const name = Buffer.from(type, 'ascii');
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.byteLength);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(Buffer.concat([name, data])));
  return Buffer.concat([length, name, data, checksum]);
}

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}

async function measurePreviewResources(run, cycleCount = 100, expectLegacy = false) {
  const { client } = run;
  await client.send('Runtime.enable');
  await client.send('HeapProfiler.enable');
  await client.evaluate(`(async () => {
    const deadline = performance.now() + 30000;
    while (document.querySelectorAll('.markdown-preview img').length !== 65) {
      if (performance.now() > deadline) throw new Error('Preview aliases did not settle');
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    await document.querySelector('.markdown-preview img').decode();
  })()`, true, 35_000);
  const article = await client.evaluate(`(() => ({
    images: document.querySelectorAll('.markdown-preview img').length,
    uniqueObjectUrls: new Set([...document.querySelectorAll('.markdown-preview img')].map(image => image.src)).size,
    lazyImages: [...document.querySelectorAll('.markdown-preview img')].filter(image => image.loading === 'lazy' && image.decoding === 'async').length,
    tables: document.querySelectorAll('.markdown-preview table').length,
    codeBlocks: document.querySelectorAll('.markdown-preview pre code').length,
    links: document.querySelectorAll('.markdown-preview a').length,
    largeNaturalSize: (() => {
      const image = document.querySelector('.markdown-preview img');
      return { width: image.naturalWidth, height: image.naturalHeight };
    })()
  }))()`);
  const fragmentNavigationMs = await client.evaluate(`(async () => {
    const host = document.querySelector('.markdown-preview');
    const link = host.querySelector('a');
    const startedAt = performance.now();
    link.click();
    const deadline = performance.now() + 5000;
    while (host.scrollTop === 0) {
      if (performance.now() > deadline) throw new Error('Fragment navigation did not scroll the preview pane');
      await new Promise(resolve => requestAnimationFrame(resolve));
    }
    await new Promise(requestAnimationFrame);
    const elapsedMs = performance.now() - startedAt;
    host.scrollTop = 0;
    await new Promise(requestAnimationFrame);
    return elapsedMs;
  })()`);
  const firstPassScroll = await measureFrames(client, '.markdown-preview', 'scroll', operationSamples);
  const scroll = await measureFrames(client, '.markdown-preview', 'scroll', operationSamples);
  await client.evaluate(`(async () => {
    document.querySelectorAll('.preview-mode-switcher button')[1].click();
    while (document.querySelector('.markdown-preview')) await new Promise(requestAnimationFrame);
  })()`);
  await client.evaluate(`(() => {
    const create = URL.createObjectURL.bind(URL);
    const revoke = URL.revokeObjectURL.bind(URL);
    const outstanding = new Set();
    window.__markliteResourceAudit = { created: 0, revoked: 0, outstanding, peakOutstanding: 0 };
    URL.createObjectURL = blob => {
      const url = create(blob);
      const audit = window.__markliteResourceAudit;
      audit.created += 1;
      audit.outstanding.add(url);
      audit.peakOutstanding = Math.max(audit.peakOutstanding, audit.outstanding.size);
      return url;
    };
    URL.revokeObjectURL = url => {
      const audit = window.__markliteResourceAudit;
      if (audit.outstanding.delete(url)) audit.revoked += 1;
      revoke(url);
    };
  })()`);
  await client.send('HeapProfiler.collectGarbage');
  const heapBefore = await client.send('Runtime.getHeapUsage');
  const processBefore = await processResources(run.child.pid);
  const cycleDurations = await client.evaluate(`(async () => {
    const values = [];
    const buttons = document.querySelectorAll('.preview-mode-switcher button');
    for (let cycle = 0; cycle < ${cycleCount}; cycle += 1) {
      const startedAt = performance.now();
      buttons[0].click();
      const deadline = performance.now() + 30000;
      while (document.querySelectorAll('.markdown-preview img').length !== 65) {
        if (performance.now() > deadline) throw new Error('Article resource cycle timed out');
        await new Promise(resolve => setTimeout(resolve, 5));
      }
      const images = [...document.querySelectorAll('.markdown-preview img')];
      await images[0].decode();
      if (!${expectLegacy} && new Set(images.slice(1).map(image => image.src)).size !== 1) {
        throw new Error('Canonical aliases did not share one Object URL');
      }
      buttons[1].click();
      while (document.querySelector('.markdown-preview')) await new Promise(requestAnimationFrame);
      if (window.__markliteResourceAudit.outstanding.size !== 0) {
        throw new Error('Preview mode switch retained Object URLs');
      }
      values.push(performance.now() - startedAt);
    }
    return values;
  })()`, true, 120_000);
  await client.send('HeapProfiler.collectGarbage');
  const heapAfter = await client.send('Runtime.getHeapUsage');
  const processAfter = await processResources(run.child.pid);
  const ownership = await client.evaluate(`(() => ({
    created: window.__markliteResourceAudit.created,
    revoked: window.__markliteResourceAudit.revoked,
    outstanding: window.__markliteResourceAudit.outstanding.size,
    peakOutstanding: window.__markliteResourceAudit.peakOutstanding
  }))()`);
  return {
    article,
    fragmentNavigationMs,
    firstPassScroll: summary(firstPassScroll.frames),
    firstPassScrollRawMs: firstPassScroll.frames,
    firstPassScrollLongTasks: summary(firstPassScroll.longTasks),
    scroll: summary(scroll.frames),
    scrollRawMs: scroll.frames,
    scrollLongTasks: summary(scroll.longTasks),
    cycles: summary(cycleDurations),
    cycleRawMs: cycleDurations,
    ownership,
    heapBefore,
    heapAfter,
    processBefore,
    processAfter
  };
}

async function main() {
  if (platform() !== 'win32') throw new Error('This baseline runner requires Windows WebView2');
  const options = parseArguments(process.argv.slice(2));
  if (!Number.isInteger(options.startupSamples) || options.startupSamples < 1) throw new Error('--startup-samples must be a positive integer');
  if (!Number.isInteger(options.operationSamples) || options.operationSamples < 1) throw new Error('--operation-samples must be a positive integer');
  if (!Number.isInteger(options.traceChunkSize) || options.traceChunkSize < 1) throw new Error('--trace-chunk-size must be a positive integer');
  if (!Number.isInteger(options.traceChunkSamples) || options.traceChunkSamples < 1) throw new Error('--trace-chunk-samples must be a positive integer');
  startupSamples = options.startupSamples;
  operationSamples = options.operationSamples;
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  const executable = await readFile(options.executable);
  const runtimeDirectory = resolve(repositoryRoot, 'tmp', 'performance-runtime', String(Date.now()));
  await mkdir(runtimeDirectory, { recursive: true });
  const toolchain = {
    node: process.version,
    npm: process.env.npm_config_user_agent ?? 'unavailable',
    rustc: await command('rustc', ['--version']).catch(() => 'unavailable')
  };
  if (options.extensionStartupOnly) {
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2 release executable, fixed ordinary 10 KiB document',
        sampleCount: options.startupSamples,
        ready: 'editor interactive aria-busy=false plus two animation frames',
        webviewProfile: 'a fresh isolated WebView2 user-data folder for every sample'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      rawMs: [],
      errors: []
    };
    for (let index = 0; index < options.startupSamples; index += 1) {
      let run;
      try {
        run = await launch(options.executable, resolve(corpusDirectory, 'representative-10-kib.md'), await unusedLoopbackPort(), runtimeDirectory, resolve(runtimeDirectory, `webview2-startup-${index}`));
        result.rawMs.push(run.startupMs);
        if (index === 0) {
          result.userAgent = run.ready.userAgent;
          result.firstProcessResources = await processResources(run.child.pid);
          result.plainRuntime = await run.client.evaluate(`(() => ({
            iframes: document.querySelectorAll('iframe').length,
            figures: document.querySelectorAll('.mermaid-diagram').length
          }))()`);
        }
      } catch (error) {
        result.errors.push({ index, error: String(error) });
      } finally {
        if (run) await stop(run).catch((error) => { result.errors.push({ index, teardownError: String(error) }); });
      }
    }
    result.startup = summary(result.rawMs);
    result.internalStartup = summary(await startupDiagnosticDurations(runtimeDirectory, options.startupSamples));
    await mkdir(dirname(options.output), { recursive: true });
    await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    process.stdout.write(`${JSON.stringify({ output: options.output, startup: result.startup, internalStartup: result.internalStartup, plainRuntime: result.plainRuntime, errors: result.errors }, null, 2)}\n`);
    if (result.errors.length > 0 || result.plainRuntime?.iframes !== 0) process.exitCode = 1;
    return;
  }
  if (options.extensionExportsOnly) {
    const packPath = resolve(repositoryRoot, 'src-tauri', 'target', 'optional', 'mermaid-offline-11.17.2.zip');
    const cliPath = resolve(repositoryRoot, 'src-tauri', 'target', 'release', 'marklite-cli.exe');
    const source = '# Extension export\n\nInline formula $x^2 + y^2$.\n\n```mermaid\nflowchart TD\nA[Start] --> B[Done]\n```\n\n```mermaid\nsequenceDiagram\nAlice->>Bob: Hello\nBob-->>Alice: Ack\n```\n';
    const fixturePath = resolve(runtimeDirectory, 'extension-export.md');
    const outputDirectory = resolve(runtimeDirectory, 'extension-exports');
    await mkdir(outputDirectory, { recursive: true });
    await writeFile(fixturePath, source, 'utf8');
    const formats = ['html', 'docx', 'pdf'];
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2 production export_document IPC and release console launcher',
        sampleCountPerFormatAndEntry: options.operationSamples,
        profile: 'one locally installed, validated optional Mermaid pack; fresh target for each export'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      cli: { name: basename(cliPath), sha256: createHash('sha256').update(await readFile(cliPath)).digest('hex') },
      pack: { sha256: createHash('sha256').update(await readFile(packPath)).digest('hex') },
      fixture: { bytes: Buffer.byteLength(source), sha256: createHash('sha256').update(source).digest('hex') },
      toolchain,
      guiIpc: {},
      cliExport: {}
    };
    let run;
    try {
      run = await launch(options.executable, fixturePath, 9380, runtimeDirectory, resolve(runtimeDirectory, 'webview2-gui'));
      result.install = await run.client.evaluate(`window.__TAURI_INTERNALS__.invoke('install_diagram_runtime', { packPath: ${JSON.stringify(packPath)} })`, true, 30000);
      for (const format of formats) {
        const requests = Array.from({ length: options.operationSamples }, (_, index) => ({
          snapshot: {
            jobId: `extension-gui-${format}-${index}`,
            tabId: 'extension-benchmark',
            contentRevision: 1,
            sourcePath: fixturePath,
            title: 'Extension export',
            content: source
          },
          targetPath: resolve(outputDirectory, `gui-${format}-${index}.${format}`),
          format,
          options: { paperSize: 'a4', orientation: 'portrait', margin: 'normal', includeTitle: true, includeLocalImages: false },
          mindMapSvg: null
        }));
        const samples = await run.client.evaluate(`(async () => {
          const requests = ${JSON.stringify(requests)};
          const samples = [];
          for (const request of requests) {
            const start = performance.now();
            const outcome = await window.__TAURI_INTERNALS__.invoke('export_document', { request });
            samples.push({ elapsedMs: performance.now() - start, warningCodes: outcome.warnings.map(warning => warning.code) });
          }
          return samples;
        })()`, true, Math.max(120_000, options.operationSamples * 60_000));
        const output = await readFile(requests.at(-1).targetPath);
        const signature = format === 'html' ? '<!doctype html>' : format === 'docx' ? 'PK' : '%PDF-';
        const header = output.subarray(0, signature.length).toString(format === 'docx' ? 'ascii' : 'utf8').toLowerCase();
        const signatureValid = header === signature.toLowerCase();
        result.guiIpc[format] = {
          duration: summary(samples.map(sample => sample.elapsedMs)),
          rawMs: samples.map(sample => sample.elapsedMs),
          warningCodes: samples.map(sample => sample.warningCodes),
          lastArtifactBytes: output.byteLength,
          signatureValid
        };
        if (!signatureValid || samples.some(sample => sample.warningCodes.length > 0)) throw new Error(`GUI ${format} export did not produce a clean artifact`);
      }
      await stop(run);
      run = null;
      for (const format of formats) {
        const samples = [];
        for (let index = 0; index < options.operationSamples; index += 1) {
          const target = resolve(outputDirectory, `cli-${format}-${index}.${format}`);
          const start = performance.now();
          let stdout;
          try {
            ({ stdout } = await execFileAsync(cliPath, ['--export', fixturePath, '--format', format, '--output', target, '--json'], {
              cwd: repositoryRoot,
              windowsHide: true,
              timeout: 120_000,
              env: {
                ...process.env,
                MARKLITE_BENCHMARK_MODE: '1',
                MARKLITE_BENCHMARK_DATA_DIR: resolve(runtimeDirectory, 'appdata'),
                WEBVIEW2_USER_DATA_FOLDER: resolve(runtimeDirectory, `webview2-cli-${format}-${index}`)
              }
            }));
          } catch (error) {
            result.cliExport[format] = { failedIndex: index, precedingSamples: samples.length, exitCode: error.code,
              stdout: String(error.stdout ?? '').slice(0, 4000), stderr: String(error.stderr ?? '').slice(0, 4000) };
            throw error;
          }
          const outcome = JSON.parse(stdout.trim());
          const output = await readFile(target);
          samples.push({ elapsedMs: performance.now() - start, warningCodes: outcome.warnings?.map(warning => warning.code) ?? [], bytes: output.byteLength });
        }
        result.cliExport[format] = {
          duration: summary(samples.map(sample => sample.elapsedMs)),
          rawMs: samples.map(sample => sample.elapsedMs),
          warningCodes: samples.map(sample => sample.warningCodes),
          lastArtifactBytes: samples.at(-1).bytes
        };
        if (samples.some(sample => sample.warningCodes.length > 0 || sample.bytes < 1000)) throw new Error(`CLI ${format} export did not produce a clean artifact`);
      }
    } catch (error) {
      result.error = String(error);
    } finally {
      if (run) await stop(run).catch((error) => { result.teardownError = String(error); });
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, gui: Object.fromEntries(Object.entries(result.guiIpc).map(([format, item]) => [format, item.duration])), cli: Object.fromEntries(Object.entries(result.cliExport).map(([format, item]) => [format, item.duration])), error: result.error }, null, 2)}\n`);
    if (result.error || result.teardownError) process.exitCode = 1;
    return;
  }
  if (options.extensionIpcOnly) {
    const targetBytes = 512_000;
    const fillToSize = (prefix) => {
      const filler = 'ordinary reference prose for profiling.\n';
      return prefix + filler.repeat(Math.ceil((targetBytes - prefix.length) / filler.length)).slice(0, targetBytes - prefix.length);
    };
    const formula = '$\\frac{x^2+\\alpha}{\\sqrt{y}}$';
    const cases = {
      plain: fillToSize('# Plain reference\n\n'),
      math256: fillToSize(`# Formula reference\n\n${Array(256).fill(formula).join(' ')}\n\n`),
      math276: fillToSize(`# Formula overflow\n\n${Array(276).fill(formula).join(' ')}\n\n`)
    };
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2, production render_markdown IPC and detached HTML parse',
        sampleCount: options.operationSamples,
        fixtureBytes: targetBytes,
        comparison: 'same byte count and filler; 256 formulas at the supported cap and 276 formulas beyond it'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      cases: {}
    };
    let run;
    try {
      run = await launch(options.executable, resolve(corpusDirectory, 'representative-10-kib.md'), 9370, runtimeDirectory);
      await selectLayout(run.client, 'edit');
      for (const [name, source] of Object.entries(cases)) {
        const bytes = Buffer.byteLength(source);
        if (bytes !== targetBytes) throw new Error(`${name} fixture has ${bytes} bytes`);
        const measured = await measureLiveSplitIpc(run.client, source, options.operationSamples);
        result.cases[name] = {
          bytes,
          sha256: createHash('sha256').update(source).digest('hex'),
          ipc: summary(measured.ipc.map((sample) => sample.ipcMs)),
          ipcRawMs: measured.ipc.map((sample) => sample.ipcMs),
          htmlBytes: measured.ipc.at(-1).htmlBytes,
          preparation: summary(measured.preparation.map((sample) => sample.parseMs)),
          preparationNodes: measured.preparation.at(-1).nodes
        };
      }
      result.plainRuntime = await run.client.evaluate(`(async () => {
        const original = window.__TAURI_INTERNALS__.invoke;
        let runtimeLoads = 0;
        window.__TAURI_INTERNALS__.invoke = (...args) => {
          if (args[0] === 'load_diagram_runtime') runtimeLoads += 1;
          return original(...args);
        };
        try {
          const buttons = document.querySelectorAll('.layout-switch button');
          for (let index = 0; index < 10; index += 1) {
            buttons[1].click();
            await new Promise(requestAnimationFrame);
            buttons[0].click();
            await new Promise(requestAnimationFrame);
          }
          return { runtimeLoads, remainingIframes: document.querySelectorAll('iframe').length };
        } finally {
          window.__TAURI_INTERNALS__.invoke = original;
        }
      })()`, true, 30000);
      result.resources = await processResources(run.child.pid);
    } catch (error) {
      result.error = String(error);
    } finally {
      if (run) await stop(run).catch((error) => { result.teardownError = String(error); });
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, cases: Object.fromEntries(Object.entries(result.cases).map(([name, value]) => [name, value.ipc])), plainRuntime: result.plainRuntime, error: result.error }, null, 2)}\n`);
    if (result.error || result.teardownError) process.exitCode = 1;
    return;
  }
  if (options.extensionDiagramsOnly) {
    const packPath = resolve(repositoryRoot, 'src-tauri', 'target', 'optional', 'mermaid-offline-11.17.2.zip');
    const diagramFixtures = JSON.parse(await readFile(resolve(repositoryRoot, 'src', 'shared', 'mermaid-fixtures.json'), 'utf8'));
    const fence = (source) => `\`\`\`mermaid\n${source}\n\`\`\``;
    const validSources = diagramFixtures.slice(0, 4).map((fixture) => fixture.source);
    const documents = {
      valid4: `# Four diagrams\n\n${validSources.map(fence).join('\n\n')}\n`,
      invalid: `# Invalid diagram\n\n${fence('flowchart TD\nA[')}\n`,
      oversized: `# Oversized diagram\n\n${fence(`flowchart TD\n${'A-->B\n'.repeat(44_000)}`)}\n`,
      excess65: `# Excess diagrams\n\n${Array(65).fill(fence('flowchart TD\nA-->B')).join('\n\n')}\n`,
      cancel16: `# Cancelled diagrams\n\n${Array.from({ length: 16 }, (_, index) => fence(`flowchart TD\nA${index}-->B${index}`)).join('\n\n')}\n`
    };
    const fixtureDirectory = resolve(runtimeDirectory, 'extension-fixtures');
    await mkdir(fixtureDirectory, { recursive: true });
    const fixtures = {};
    for (const [name, source] of Object.entries(documents)) {
      const path = resolve(fixtureDirectory, `${name}.md`);
      await writeFile(path, source, 'utf8');
      fixtures[name] = { path, bytes: Buffer.byteLength(source), sha256: createHash('sha256').update(source).digest('hex') };
    }
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2, production preview with installed optional Mermaid pack',
        coldReady: 'process launch to expected rendered figures or explicit source fallback',
        lifecycle: '100 article/mind-map cycles with four validated diagrams, forced GC before and after',
        boundaries: 'one invalid syntax, one over 256 KiB, 65 diagram document limit, and cancellation during 16 diagram document'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      pack: { bytes: (await readFile(packPath)).byteLength, sha256: createHash('sha256').update(await readFile(packPath)).digest('hex') },
      toolchain,
      fixtures: Object.fromEntries(Object.entries(fixtures).map(([name, fixture]) => [name, { bytes: fixture.bytes, sha256: fixture.sha256 }])),
      cases: {}
    };
    let installer;
    try {
      installer = await launch(options.executable, resolve(corpusDirectory, 'representative-10-kib.md'), 9371, runtimeDirectory, resolve(runtimeDirectory, 'webview2-install'));
      result.install = await installer.client.evaluate(`window.__TAURI_INTERNALS__.invoke('install_diagram_runtime', { packPath: ${JSON.stringify(packPath)} })`, true, 30000);
    } finally {
      if (installer) await stop(installer);
    }
    const caseNames = ['valid4', 'invalid', 'oversized', 'excess65', 'cancel16'];
    if (options.extensionDiagramCase && !caseNames.includes(options.extensionDiagramCase)) {
      throw new Error(`Unknown extension diagram case: ${options.extensionDiagramCase}`);
    }
    for (const [index, name] of caseNames.entries()) {
      if (options.extensionDiagramCase && options.extensionDiagramCase !== name) continue;
      let run;
      const started = performance.now();
      try {
        run = await launch(options.executable, fixtures[name].path, 9372 + index, runtimeDirectory, resolve(runtimeDirectory, `webview2-${name}`));
        const browserEvents = [];
        result.cases[name] = { browserEvents };
        run.client.socket.addEventListener('message', (event) => {
          const message = JSON.parse(String(event.data));
          if (message.method === 'Log.entryAdded') browserEvents.push({ type: 'log', text: message.params?.entry?.text });
          if (message.method === 'Runtime.exceptionThrown') browserEvents.push({ type: 'exception', text: message.params?.exceptionDetails?.text });
        });
        await run.client.send('Log.enable');
        await run.client.send('Runtime.enable');
        await run.client.evaluate(`(() => {
          window.__markliteDiagramTrace = [];
          const originalInvoke = window.__TAURI_INTERNALS__.invoke;
          window.__TAURI_INTERNALS__.invoke = (...args) => {
            if (args[0] !== 'load_diagram_runtime') return originalInvoke(...args);
            const started = performance.now();
            window.__markliteDiagramTrace.push({ event: 'load-started', atMs: started });
            return originalInvoke(...args).then((asset) => {
              window.__markliteDiagramTrace.push({ event: 'load-succeeded', elapsedMs: performance.now() - started,
                scriptLength: asset.scriptUtf8?.length ?? null });
              return asset;
            }, (error) => {
              window.__markliteDiagramTrace.push({ event: 'load-failed', elapsedMs: performance.now() - started,
                error: String(error) });
              throw error;
            });
          };
          new MutationObserver((records) => {
            for (const record of records) for (const node of record.addedNodes) {
              if (node.nodeName !== 'IFRAME') continue;
              window.__markliteDiagramTrace.push({ event: 'frame-added', atMs: performance.now(),
                srcdocLength: node.srcdoc?.length ?? null });
              const scripts = [...node.srcdoc.matchAll(/<script>([\\s\\S]*?)<\\/script>/g)];
              for (const [index, script] of scripts.entries()) {
                crypto.subtle.digest('SHA-256', new TextEncoder().encode(script[1])).then((digest) => {
                  const bytes = new Uint8Array(digest);
                  window.__markliteDiagramTrace.push({ event: 'script-hash', index,
                    sha256Base64: btoa(String.fromCharCode(...bytes)) });
                });
              }
              node.addEventListener('load', () => window.__markliteDiagramTrace.push({ event: 'frame-loaded', atMs: performance.now() }));
            }
          }).observe(document.body, { childList: true, subtree: true });
        })()`);
        await selectLayout(run.client, 'split');
        if (name === 'cancel16') {
          result.cases[name] = await run.client.evaluate(`(async () => {
            document.querySelectorAll('.preview-mode-switcher button')[1].click();
            const deadline = performance.now() + 10000;
            while (document.querySelector('.markdown-preview') && performance.now() < deadline) {
              await new Promise(requestAnimationFrame);
            }
            await new Promise(resolve => setTimeout(resolve, 500));
            return { remainingIframes: document.querySelectorAll('iframe').length,
              articleMounted: Boolean(document.querySelector('.markdown-preview')) };
          })()`, true, 15000);
        } else {
          const expectedFigures = name === 'valid4' ? 4 : 0;
          const expectedFallbacks = name === 'valid4' ? 0 : name === 'excess65' ? 65 : 1;
          const preview = await run.client.evaluate(`(async () => {
            const deadline = performance.now() + 30000;
            while (performance.now() < deadline) {
              const figures = document.querySelectorAll('.mermaid-diagram').length;
              const fallbacks = document.querySelectorAll('.mermaid-source-fallback').length;
              if (figures === ${expectedFigures} && fallbacks === ${expectedFallbacks}) {
                await new Promise(requestAnimationFrame);
                await new Promise(requestAnimationFrame);
                return { figures, fallbacks, remainingIframes: document.querySelectorAll('iframe').length,
                  diagnostics: [...document.querySelectorAll('.mermaid-diagram-error')].slice(0, 2).map(node => node.textContent) };
              }
              await new Promise(resolve => setTimeout(resolve, 20));
            }
            throw new Error('Preview did not reach expected diagram/fallback counts: ' +
              document.querySelectorAll('.mermaid-diagram').length + '/' +
              document.querySelectorAll('.mermaid-source-fallback').length);
          })()`, true, 35000);
          result.cases[name] = { ...result.cases[name], ...preview, coldReadyMs: performance.now() - started, resources: await processResources(run.child.pid) };
          if (name === 'valid4') {
            result.cases[name].cspHashes = await run.client.evaluate(`(async () => {
              const csp = await fetch(document.URL).then(response => response.headers.get('content-security-policy'));
              const hashes = (window.__markliteDiagramTrace ?? []).filter(entry => entry.event === 'script-hash')
                .sort((left, right) => left.index - right.index).map(entry => entry.sha256Base64);
              return { hashes, allAllowed: hashes.length === 2 && hashes.every(hash => csp?.includes("'sha256-" + hash + "'")) };
            })()`, true, 3000);
            if (!result.cases[name].cspHashes.allAllowed) throw new Error('Sandbox inline script hashes are absent from the production CSP');
            result.cases[name].themeSwitches = await run.client.evaluate(`(async () => {
              const initialTheme = document.documentElement.dataset.theme;
              const targets = [initialTheme === 'dark' ? 'light' : 'dark', initialTheme === 'dark' ? 'dark' : 'light'];
              const results = [];
              for (const target of targets) {
                const started = performance.now();
                const settingsButton = document.querySelectorAll('.top-actions button')[8];
                if (!settingsButton) throw new Error('Settings toolbar button missing');
                settingsButton.click();
                const modalDeadline = performance.now() + 3000;
                while (!document.querySelector('.settings-dialog .settings-panel select') && performance.now() < modalDeadline) {
                  await new Promise(requestAnimationFrame);
                }
                const select = document.querySelectorAll('.settings-dialog .settings-panel select')[1];
                if (!select) throw new Error('Theme selector missing');
                select.value = target;
                select.dispatchEvent(new Event('change', { bubbles: true }));
                document.querySelector('.settings-dialog .dialog-footer .primary-button').click();
                const deadline = performance.now() + 10000;
                while (performance.now() < deadline) {
                  if (document.documentElement.dataset.theme === target &&
                      !document.querySelector('.settings-dialog') &&
                      document.querySelectorAll('.mermaid-diagram').length === 4) break;
                  await new Promise(requestAnimationFrame);
                }
                const figures = document.querySelectorAll('.mermaid-diagram').length;
                if (figures !== 4 || document.documentElement.dataset.theme !== target) throw new Error('Theme switch did not redraw diagrams');
                results.push({ theme: target, elapsedMs: performance.now() - started, figures });
              }
              return results;
            })()`, true, 30000);
            await run.client.send('HeapProfiler.enable');
            await run.client.send('HeapProfiler.collectGarbage');
            const heapBefore = await run.client.send('Runtime.getHeapUsage');
            const processBefore = await processResources(run.child.pid);
            const cycles = await run.client.evaluate(`(async () => {
              const buttons = document.querySelectorAll('.preview-mode-switcher button');
              const durations = [];
              let peakIframes = 0;
              for (let index = 0; index < 100; index += 1) {
                const started = performance.now();
                buttons[1].click();
                while (document.querySelector('.markdown-preview')) await new Promise(requestAnimationFrame);
                peakIframes = Math.max(peakIframes, document.querySelectorAll('iframe').length);
                buttons[0].click();
                const deadline = performance.now() + 10000;
                while (document.querySelectorAll('.mermaid-diagram').length !== 4) {
                  if (performance.now() > deadline) throw new Error('Diagram cycle ' + index + ' timed out');
                  await new Promise(resolve => setTimeout(resolve, 10));
                }
                durations.push(performance.now() - started);
              }
              buttons[1].click();
              while (document.querySelector('.markdown-preview')) await new Promise(requestAnimationFrame);
              return { durations, peakIframes, remainingIframes: document.querySelectorAll('iframe').length };
            })()`, true, 180000);
            await run.client.send('HeapProfiler.collectGarbage');
            const heapAfter = await run.client.send('Runtime.getHeapUsage');
            const processAfter = await processResources(run.child.pid);
            result.cycles = {
              count: cycles.durations.length,
              duration: summary(cycles.durations),
              peakIframes: cycles.peakIframes,
              remainingIframes: cycles.remainingIframes,
              heapBefore,
              heapAfter,
              processBefore,
              processAfter
            };
          }
        }
      } catch (error) {
        result.cases[name] = { ...(result.cases[name] ?? {}), error: String(error) };
        if (run) {
          result.cases[name].diagnostic = await run.client.evaluate(`(async () => ({
            figures: document.querySelectorAll('.mermaid-diagram').length,
            fallbacks: document.querySelectorAll('.mermaid-source-fallback').length,
            messages: [...document.querySelectorAll('.mermaid-diagram-error')].slice(0, 5).map(node => node.textContent),
            iframes: document.querySelectorAll('iframe').length,
            trace: window.__markliteDiagramTrace ?? [],
            parentScriptNonce: document.querySelector('script')?.nonce ?? null,
            documentUrl: document.URL,
            csp: await fetch(document.URL).then(response => response.headers.get('content-security-policy')).catch(String),
            previewText: document.querySelector('.markdown-preview')?.textContent?.slice(0, 500) ?? null
          }))()`, true, 3000).catch((diagnosticError) => ({ error: String(diagnosticError) }));
        }
      } finally {
        if (run) {
          try {
            if (result.cases[name]?.error) {
              run.client.close();
              await terminateChild(run.child);
            } else {
              await stop(run);
            }
          } catch (error) {
            result.cases[name].teardownError = String(error);
          }
        }
        await mkdir(dirname(options.output), { recursive: true });
        await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      }
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, cases: result.cases, cycles: result.cycles?.duration }, null, 2)}\n`);
    if (Object.values(result.cases).some((value) => value.error || value.teardownError)) process.exitCode = 1;
    return;
  }
  if (options.liveSplitArticleOnly) {
    const name = options.liveSplitCase ?? 'representative-500-kib.md';
    const file = manifest.files.find((entry) => entry.name === name);
    if (!file) throw new Error(`Missing corpus manifest entry: ${name}`);
    let run;
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2 production article, local fragment navigation and full DOM selection',
        caveat: 'programmatic selection does not exercise the system clipboard or keyboard selection'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      corpus: { name, bytes: file.bytes, sha256: file.sha256 },
      toolchain
    };
    try {
      run = await launch(options.executable, resolve(corpusDirectory, name), 9368, runtimeDirectory);
      await selectLayout(run.client, 'split');
      result.article = await run.client.evaluate(`(async () => {
        const host = document.querySelector('.preview-content');
        const headings = Array.from(host.querySelectorAll('h1[id]'));
        const target = headings[Math.floor(headings.length * 0.8)];
        if (!target) throw new Error('Missing heading anchor');
        await new Promise(resolve => setTimeout(resolve, 500));
        const beforeHeight = host.scrollHeight;
        const clientHeight = host.clientHeight;
        host.scrollTop = 1000;
        const manualScrollTop = host.scrollTop;
        host.scrollTop = 0;
        const beforeNodes = host.querySelectorAll('*').length;
        const topLevelElementCount = host.querySelector('.preview-chunk')
          ? [...host.querySelectorAll('.preview-chunk')].reduce((count, leaf) => count + leaf.children.length, 0)
          : host.children.length;
        const opened = await window.__TAURI_INTERNALS__.invoke('open_markdown_file', {
          path: ${JSON.stringify(resolve(corpusDirectory, name))}
        });
        const sourceMap = await window.__TAURI_INTERNALS__.invoke('render_markdown', {
          content: opened.document.content
        });
        const freshTemplate = document.createElement('template');
        freshTemplate.innerHTML = sourceMap.html;
        const renderedElementCount = freshTemplate.content.children.length;
        const taggedElements = [...host.querySelectorAll('[data-marklite-source-block]')];
        const taggedOrdinalsValid = taggedElements.every((element, index) => element.getAttribute('data-marklite-source-block') === String(index));
        const markerTextVisible = host.textContent.includes('MARKLITE_BLOCK_');
        const sourceSync = [];
        if (${JSON.stringify(name)} === 'scroll-markers-1000.md') {
          const scroller = document.querySelector('.cm-scroller');
          for (const leftPercent of [35, 50, 65]) {
            document.querySelector('.editor-stage').style.setProperty('--split-left', leftPercent + '%');
            document.querySelector('.editor-stage').style.setProperty('--split-right', (100 - leftPercent) + '%');
            await new Promise(requestAnimationFrame);
            for (const ratio of [0.1, 0.5, 0.85]) {
              scroller.scrollTop = (scroller.scrollHeight - scroller.clientHeight) * ratio;
              await new Promise(requestAnimationFrame);
              await new Promise(requestAnimationFrame);
              await new Promise(resolve => setTimeout(resolve, 100));
              const top = scroller.getBoundingClientRect().top;
              const visibleLine = [...scroller.querySelectorAll('.cm-line')].find(line => line.getBoundingClientRect().bottom > top + 1);
              const marker = visibleLine?.textContent?.match(/MARKLITE-SCROLL-\\d{5}/)?.[0];
              const heading = marker && [...host.querySelectorAll('h1')].find(element => element.textContent.includes(marker));
              const previewTop = heading ? heading.getBoundingClientRect().top - host.getBoundingClientRect().top : null;
              const editorHeight = visibleLine?.getBoundingClientRect().height ?? null;
              const previewHeight = heading?.getBoundingClientRect().height ?? null;
              const editorProgress = editorHeight ? Math.min(1, Math.max(0, - (visibleLine.getBoundingClientRect().top - top) / editorHeight)) : null;
              const previewProgress = previewHeight ? Math.min(1, Math.max(0, (2 - previewTop) / previewHeight)) : null;
              const firstPreviewHeading = [...host.querySelectorAll('h1')].find(element => element.getBoundingClientRect().bottom > host.getBoundingClientRect().top);
              sourceSync.push({ leftPercent, ratio, marker, previewTop, editorLineTop: visibleLine?.getBoundingClientRect().top - top,
                editorHeight, previewHeight, editorProgress, previewProgress,
                previewScrollTop: host.scrollTop, firstPreviewHeading: firstPreviewHeading?.textContent });
            }
          }
        }
        const anchor = document.createElement('a');
        anchor.href = '#' + target.id;
        anchor.textContent = 'Jump to heading';
        host.prepend(anchor);
        const clickPrevented = !anchor.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
        const deadline = performance.now() + 5000;
        while (host.scrollTop === 0 && performance.now() < deadline) {
          await new Promise(resolve => setTimeout(resolve, 20));
        }
        await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
        const targetTop = target.getBoundingClientRect().top - host.getBoundingClientRect().top;
        const stage = document.querySelector('.editor-stage');
        const widths = [];
        for (const leftPercent of [30, 70, 50]) {
          stage.style.setProperty('--split-left', leftPercent + '%');
          stage.style.setProperty('--split-right', (100 - leftPercent) + '%');
          await new Promise(requestAnimationFrame);
          await new Promise(requestAnimationFrame);
          await new Promise(resolve => setTimeout(resolve, 100));
          const estimatedHeight = host.scrollHeight;
          const chunks = host.querySelectorAll('.preview-chunk, .preview-chunk-group');
          for (const chunk of chunks) chunk.style.contentVisibility = 'visible';
          const actualHeight = host.scrollHeight;
          for (const chunk of chunks) chunk.style.contentVisibility = 'auto';
          host.scrollTop = 0;
          anchor.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
          await new Promise(resolve => setTimeout(resolve, 100));
          widths.push({ leftPercent, width: host.clientWidth, estimatedHeight, actualHeight,
            targetTop: target.getBoundingClientRect().top - host.getBoundingClientRect().top });
        }
        const selection = window.getSelection();
        const range = document.createRange();
        range.selectNodeContents(host);
        selection.removeAllRanges();
        selection.addRange(range);
        const selectionLength = selection.toString().length;
        selection.removeAllRanges();
        return {
          headingCount: headings.length, targetId: target.id,
          scrollTop: host.scrollTop, targetTop, clientHeight, manualScrollTop, clickPrevented, widths,
          beforeHeight, afterHeight: host.scrollHeight,
          beforeNodes, afterNodes: host.querySelectorAll('*').length,
          topLevelElementCount, renderedElementCount, sourceBlockCount: sourceMap.sourceBlocks.length,
          taggedElementCount: taggedElements.length, taggedOrdinalsValid, markerTextVisible,
          sourceSync,
          textLength: host.textContent.length, selectionLength
        };
      })()`, true, name === 'scroll-markers-1000.md' ? 60_000 : 30_000);
    } catch (error) {
      result.error = String(error);
    } finally {
      if (run) await stop(run).catch((error) => { result.teardownError = String(error); });
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, article: result.article, error: result.error }, null, 2)}\n`);
    return;
  }
  if (options.liveSplitTraceOnly) {
    const name = options.liveSplitCase ?? 'representative-500-kib.md';
    const file = manifest.files.find((entry) => entry.name === name);
    if (!file) throw new Error(`Missing corpus manifest entry: ${name}`);
    let run;
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2, production split layout, live preview and scroll sync',
        trace: options.traceEditorOnly
          ? 'CDP Tracing ReportEvents during one trusted input through two animation frames'
          : 'CDP Tracing ReportEvents during one trusted input through changed preview and two animation frames',
        layout: options.traceLayout,
        categoryDurations: 'inclusive durations may overlap; totals do not add to wall time'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      corpus: { name, bytes: file.bytes, sha256: file.sha256 },
      toolchain
    };
    try {
      run = await launch(options.executable, resolve(corpusDirectory, name), 9369, runtimeDirectory);
      if (!['edit', 'split'].includes(options.traceLayout)) throw new Error('Invalid --trace-layout');
      await selectLayout(run.client, options.traceLayout);
      if (options.tracePreviewContain) {
        result.previewContain = await run.client.evaluate(`(() => {
          const host = document.querySelector('.preview-content');
          const before = host.scrollHeight;
          host.style.contain = 'size layout paint';
          return { before, after: host.scrollHeight };
        })()`);
      }
      if (options.traceChunks) {
        result.chunks = await run.client.evaluate(`(async () => {
          const host = document.querySelector('.preview-content');
          const stage = document.querySelector('.editor-stage');
          const originalHeight = host.scrollHeight;
          stage.style.setProperty('--split-left', '30%');
          stage.style.setProperty('--split-right', '70%');
          const originalNarrowHeight = host.scrollHeight;
          stage.style.setProperty('--split-left', '50%');
          stage.style.setProperty('--split-right', '50%');
          const originalNodes = host.querySelectorAll('*').length;
          const nodes = Array.from(host.childNodes);
          const fragment = document.createDocumentFragment();
          const chunks = [];
          const groups = [];
          for (let index = 0; index < nodes.length; index += ${options.traceChunkSize}) {
            const chunk = document.createElement('div');
            chunk.style.display = 'flow-root';
            chunk.append(...nodes.slice(index, index + ${options.traceChunkSize}));
            chunks.push(chunk);
          }
          if (${options.traceNestedChunks}) {
            for (let index = 0; index < chunks.length; index += 24) {
              const group = document.createElement('div');
              group.style.display = 'flow-root';
              group.append(...chunks.slice(index, index + 24));
              fragment.append(group);
              groups.push(group);
            }
          } else {
            fragment.append(...chunks);
          }
          host.replaceChildren(fragment);
          const wrappedHeight = host.scrollHeight;
          const heights = chunks.map(chunk => chunk.getBoundingClientRect().height);
          const groupHeights = groups.map(group => group.getBoundingClientRect().height);
          for (let index = 0; index < chunks.length; index += 1) {
            chunks[index].style.contentVisibility = 'auto';
            chunks[index].style.setProperty('--base-height', heights[index] + 'px');
            chunks[index].style.containIntrinsicSize = 'calc(var(--base-height) * var(--leaf-scale, 1))';
          }
          for (let index = 0; index < groups.length; index += 1) {
            groups[index].style.contentVisibility = 'auto';
            groups[index].style.setProperty('--base-group-height', groupHeights[index] + 'px');
            groups[index].style.containIntrinsicSize = 'calc(var(--base-group-height) * var(--chunk-scale, 1))';
          }
          window.__markliteTraceChunks = { chunks, heights, groups, groupHeights };
          await new Promise(requestAnimationFrame);
          await new Promise(requestAnimationFrame);
          return { chunks: chunks.length, groups: groups.length, originalNodes, originalHeight, originalNarrowHeight, wrappedHeight, nextHeight: host.scrollHeight };
        })()`, true, 120_000);
      }
      result.trace = await traceLiveSplitInput(run.client, options.traceEditorOnly);
      if (options.traceChunks) {
        result.chunksAfter = await run.client.evaluate(`(async () => {
          const stage = document.querySelector('.editor-stage');
          const host = document.querySelector('.preview-content');
          stage.style.setProperty('--split-left', '30%');
          stage.style.setProperty('--split-right', '70%');
          await new Promise(requestAnimationFrame);
          await new Promise(requestAnimationFrame);
          const oldNarrowHeight = host.scrollHeight;
          const resizeStarted = performance.now();
          const { chunks, heights, groups } = window.__markliteTraceChunks;
          if (${options.traceNestedChunks}) {
            const visibleGroup = ${options.traceVisibleOnly}
              ? Math.max(0, groups.findIndex(group => group.getBoundingClientRect().bottom > host.getBoundingClientRect().top))
              : -1;
            const sampleIndexes = Array.from({ length: Math.min(${options.traceChunkSamples}, ${options.traceVisibleOnly} ? 24 : chunks.length) }, (_, index) =>
              ${options.traceVisibleOnly}
                ? Math.min(chunks.length - 1, visibleGroup * 24 + Math.floor((index + 0.5) * 24 / Math.min(${options.traceChunkSamples}, 24)))
                : Math.min(chunks.length - 1, Math.floor((index + 0.5) * chunks.length / Math.min(${options.traceChunkSamples}, chunks.length))));
            const sampledGroups = ${options.traceVisibleOnly} ? [] : [...new Set(sampleIndexes.map(index => Math.floor(index / 24)))];
            const measuredIndexes = ${options.traceNoToggleSample} ? [visibleGroup * 24] : sampleIndexes;
            if (!${options.traceNoToggleSample}) {
              for (const index of sampledGroups) groups[index].style.contentVisibility = 'visible';
              for (const index of measuredIndexes) chunks[index].style.contentVisibility = 'visible';
            }
            const sampleHeights = measuredIndexes.map(index => chunks[index].getBoundingClientRect().height);
            const sampleLayoutMs = performance.now() - resizeStarted;
            const scale = sampleHeights.reduce((sum, height) => sum + height, 0) /
              measuredIndexes.reduce((sum, index) => sum + heights[index], 0);
            host.style.setProperty('--chunk-scale', String(scale));
            for (const group of groups) group.style.setProperty('--leaf-scale', String(scale));
            if (!${options.traceNoToggleSample}) {
              for (const index of measuredIndexes) chunks[index].style.contentVisibility = 'auto';
              for (const index of sampledGroups) groups[index].style.contentVisibility = 'auto';
            }
            await new Promise(requestAnimationFrame);
            await new Promise(requestAnimationFrame);
            const narrowHeight = host.scrollHeight;
            return { oldNarrowHeight, narrowHeight, scale, sampleLayoutMs, resizeMs: performance.now() - resizeStarted };
          }
          const sampleIndexes = Array.from({ length: Math.min(${options.traceChunkSamples}, chunks.length) }, (_, index) =>
            Math.min(chunks.length - 1, Math.floor((index + 0.5) * chunks.length / Math.min(${options.traceChunkSamples}, chunks.length))));
          for (const index of sampleIndexes) chunks[index].style.contentVisibility = 'visible';
          const sampleHeights = sampleIndexes.map(index => chunks[index].getBoundingClientRect().height);
          const sampleLayoutMs = performance.now() - resizeStarted;
          const scale = sampleHeights.reduce((sum, height) => sum + height, 0) /
            sampleIndexes.reduce((sum, index) => sum + heights[index], 0);
          host.style.setProperty('--chunk-scale', String(scale));
          for (const index of sampleIndexes) chunks[index].style.contentVisibility = 'auto';
          const scaleStyleMs = performance.now() - resizeStarted - sampleLayoutMs;
          await new Promise(requestAnimationFrame);
          await new Promise(requestAnimationFrame);
          const narrowHeight = host.scrollHeight;
          const resizeMs = performance.now() - resizeStarted;
          const selection = window.getSelection();
          const range = document.createRange();
          range.selectNodeContents(host);
          selection.removeAllRanges();
          selection.addRange(range);
          const selectionLength = selection.toString().length;
          selection.removeAllRanges();
          return { oldNarrowHeight, narrowHeight, scale, resizeMs, sampleLayoutMs, scaleStyleMs, selectionLength };
        })()`, true, 120_000);
      }
    } catch (error) {
      result.error = String(error);
    } finally {
      if (run) await stop(run).catch((error) => { result.teardownError = String(error); });
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, error: result.error, top: Object.entries(result.trace?.categories ?? {}).slice(0, 8) }, null, 2)}\n`);
    return;
  }
  if (options.liveSplitPreviewOnly) {
    const files = [
      'representative-10-kib.md', 'representative-100-kib.md',
      'representative-500-kib.md', 'representative-1-mib.md', 'representative-5-mib.md'
    ];
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2 in production preview-only layout',
        frames: '30 real requestAnimationFrame intervals while scrolling the full article preview',
        samples: options.operationSamples
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      cases: {}
    };
    for (const [index, name] of files.entries()) {
      const file = manifest.files.find((entry) => entry.name === name);
      if (!file) throw new Error(`Missing corpus manifest entry: ${name}`);
      let run;
      try {
        run = await launch(options.executable, resolve(corpusDirectory, name), 9390 + index, runtimeDirectory);
        await selectLayout(run.client, 'preview');
        const frames = await measureFrames(run.client, '.preview-content', 'scroll', options.operationSamples);
        result.cases[name] = {
          bytes: file.bytes, sha256: file.sha256,
          scroll: summary(frames.frames), scrollRawMs: frames.frames,
          longTasks: summary(frames.longTasks),
          previewNodes: await run.client.evaluate("document.querySelector('.preview-content')?.querySelectorAll('*').length ?? null")
        };
      } catch (error) {
        result.cases[name] = { bytes: file.bytes, sha256: file.sha256, error: String(error) };
      } finally {
        if (run) await stop(run).catch((error) => { result.cases[name].teardownError = String(error); });
        await mkdir(dirname(options.output), { recursive: true });
        await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      }
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, cases: Object.fromEntries(Object.entries(result.cases).map(([name, value]) => [name, value.error ?? value.scroll])) }, null, 2)}\n`);
    return;
  }
  if (options.liveSplitOrdinaryOnly) {
    const name = 'representative-10-kib.md';
    const file = manifest.files.find((entry) => entry.name === name);
    let run;
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2, production 10 KiB document tab and search UI',
        tabSwitch: 'new empty tab plus 30 round trips to source tab, each to two animation frames',
        search: 'trusted CDP Ctrl+F and text insertion until match counter updates',
        reading: '30 real preview scroll frames'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      corpus: file
    };
    try {
      run = await launch(options.executable, resolve(corpusDirectory, name), 9380, runtimeDirectory);
      result.environment = await run.client.evaluate(`(() => ({
        userAgent: navigator.userAgent,
        devicePixelRatio: window.devicePixelRatio,
        viewport: { width: window.innerWidth, height: window.innerHeight },
        editorFont: getComputedStyle(document.querySelector('.cm-content')).fontFamily,
        previewFont: getComputedStyle(document.querySelector('.preview-content')).fontFamily,
        previewFontSize: getComputedStyle(document.querySelector('.preview-content')).fontSize
      }))()`);
      const switches = await run.client.evaluate(`(async () => {
        document.querySelector('.top-actions button')?.click();
        await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
        const tabs = [...document.querySelectorAll('.document-tab .tab-main')];
        if (tabs.length !== 2) throw new Error('New-document action did not create a second tab');
        const values = [];
        for (let index = 0; index < 30; index += 1) {
          for (const tab of tabs) {
            const startedAt = performance.now();
            tab.click();
            await new Promise(requestAnimationFrame);
            await new Promise(requestAnimationFrame);
            if (tab.getAttribute('aria-selected') !== 'true') throw new Error('Tab selection did not settle');
            values.push(performance.now() - startedAt);
          }
        }
        tabs[0].click();
        await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
        return values;
      })()`, true, 120000);
      result.tabSwitch = summary(switches);
      result.tabSwitchRawMs = switches;
      await run.client.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'f', code: 'KeyF', modifiers: 2 });
      await run.client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'f', code: 'KeyF', modifiers: 2 });
      await run.client.evaluate(`(async () => {
        const deadline = performance.now() + 5000;
        while (!document.querySelector('.find-popover input')) {
          if (performance.now() > deadline) throw new Error('Find popover did not open');
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        window.__markliteOrdinarySearchStarted = performance.now();
      })()`);
      await run.client.send('Input.insertText', { text: 'MarkLite' });
      result.search = await run.client.evaluate(`(async () => {
        const deadline = performance.now() + 10000;
        while (performance.now() < deadline) {
          const counter = document.querySelector('.find-input-shell span')?.textContent ?? '';
          if (/^1\\/\\d+$/.test(counter)) {
            await new Promise(requestAnimationFrame);
            await new Promise(requestAnimationFrame);
            return { elapsedMs: performance.now() - window.__markliteOrdinarySearchStarted, counter };
          }
          await new Promise(resolve => setTimeout(resolve, 10));
        }
        throw new Error('Ordinary search did not produce a match counter');
      })()`, true, 15000);
      const reading = await measureFrames(run.client, '.preview-content', 'scroll', 30);
      result.readingScroll = summary(reading.frames);
      result.readingScrollRawMs = reading.frames;
      result.readingLongTasks = summary(reading.longTasks);
    } catch (error) {
      result.error = String(error);
    } finally {
      if (run) await stop(run).catch((error) => { result.teardownError = String(error); });
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, tabSwitch: result.tabSwitch, search: result.search, reading: result.readingScroll, error: result.error }, null, 2)}\n`);
    return;
  }
  if (options.liveSplitInputOnly) {
    const allFiles = [
      'representative-10-kib.md', 'representative-100-kib.md',
      'representative-500-kib.md', 'representative-1-mib.md', 'representative-5-mib.md'
    ];
    if (options.liveSplitCase && !allFiles.includes(options.liveSplitCase)) {
      throw new Error(`Unknown --live-split-case: ${options.liveSplitCase}`);
    }
    const files = options.liveSplitCase ? [options.liveSplitCase] : allFiles;
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2, production CodeMirror and live split preview',
        ime: 'CDP candidate composition followed by inserted Chinese text',
        paste: 'untrusted DOM ClipboardEvent with text/plain payload; records whether CodeMirror accepted it',
        bulk: 'trusted CDP bulk text insertion, not an OS clipboard operation',
        postInputDrag: 'one character immediately before 35px divider drag; frame waits one animation frame, render count covers only through two frames after release',
        burst: '20 trusted CDP single-character inserts without waiting for preview between keys'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      cases: {}
    };
    for (const [index, name] of files.entries()) {
      const file = manifest.files.find((entry) => entry.name === name);
      if (!file) throw new Error(`Missing corpus manifest entry: ${name}`);
      let run;
      try {
        run = await launch(options.executable, resolve(corpusDirectory, name), 9370 + index, runtimeDirectory);
        await selectLayout(run.client, 'split');
        result.cases[name] = { bytes: file.bytes, sha256: file.sha256, ...(await measureLiveSplitInputPatterns(run.client)) };
      } catch (error) {
        result.cases[name] = { bytes: file.bytes, sha256: file.sha256, error: String(error) };
      } finally {
        if (run) await stop(run).catch((error) => { result.cases[name].teardownError = String(error); });
        await mkdir(dirname(options.output), { recursive: true });
        await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      }
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, cases: Object.fromEntries(Object.entries(result.cases).map(([name, value]) => [name, value.error ?? value.burst])) }, null, 2)}\n`);
    return;
  }
  if (options.liveSplitScrollOnly) {
    const name = 'scroll-markers-1000.md';
    const file = manifest.files.find((entry) => entry.name === name);
    if (!file) throw new Error(`Missing corpus manifest entry: ${name}`);
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2, production editor-to-preview sync with marker headings',
        variation: 'direct split CSS widths and preview font size in isolated WebView; actual pixel widths recorded',
        marker: 'first visible source and preview heading number after two animation frames and 100 ms idle'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      corpus: { name, bytes: file.bytes, sha256: file.sha256 },
      scenarios: {}
    };
    let run;
    try {
      run = await launch(options.executable, resolve(corpusDirectory, name), 9360, runtimeDirectory);
      await selectLayout(run.client, 'split');
      for (const [widthPercent, fontPx] of [[30, 14], [50, 14], [70, 14], [50, 20]]) {
        const key = `${widthPercent}-${fontPx}`;
        result.scenarios[key] = await measureScrollAlignment(run.client, widthPercent, fontPx);
      }
      result.widthReplay = await run.client.evaluate(`(async () => {
        const editor = document.querySelector('.cm-scroller');
        const preview = document.querySelector('.preview-content');
        const stage = document.querySelector('.editor-stage');
        const visibleMarker = (surface, selector) => {
          const top = surface.getBoundingClientRect().top;
          const line = [...surface.querySelectorAll(selector)].find(node => node.getBoundingClientRect().bottom > top + 4);
          return line?.textContent?.match(/MARKLITE-SCROLL-(\\d{5})/)?.[1] ?? null;
        };
        stage.style.setProperty('--split-left', '50%');
        stage.style.setProperty('--split-right', '50%');
        editor.scrollTop = (editor.scrollHeight - editor.clientHeight) * 0.75;
        await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
        await new Promise(resolve => setTimeout(resolve, 100));
        const before = { source: visibleMarker(editor, '.cm-line'), preview: visibleMarker(preview, 'h1') };
        stage.style.setProperty('--split-left', '35%');
        stage.style.setProperty('--split-right', '65%');
        await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
        await new Promise(resolve => setTimeout(resolve, 100));
        const after = { source: visibleMarker(editor, '.cm-line'), preview: visibleMarker(preview, 'h1') };
        return { before, after };
      })()`, true, 30000);
      result.imageReplay = await run.client.evaluate(`(async () => {
        const editor = document.querySelector('.cm-scroller');
        const preview = document.querySelector('.preview-content');
        const visibleMarker = (surface, selector) => {
          const top = surface.getBoundingClientRect().top;
          const line = [...surface.querySelectorAll(selector)].find(node => node.getBoundingClientRect().bottom > top + 4);
          return line?.textContent?.match(/MARKLITE-SCROLL-(\\d{5})/)?.[1] ?? null;
        };
        editor.scrollTop = (editor.scrollHeight - editor.clientHeight) * 0.75;
        await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
        const before = { source: visibleMarker(editor, '.cm-line'), preview: visibleMarker(preview, 'h1') };
        const headings = document.querySelectorAll('.preview-content h1');
        const image = document.createElement('img');
        image.alt = 'Delayed layout probe';
        image.width = 600;
        image.height = 400;
        headings[500].after(image);
        const loaded = new Promise(resolve => { image.onload = () => resolve(true); image.onerror = () => resolve(false); });
        image.src = 'data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs=';
        await loaded;
        await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
        await new Promise(resolve => setTimeout(resolve, 100));
        const after = { source: visibleMarker(editor, '.cm-line'), preview: visibleMarker(preview, 'h1') };
        return { before, after, naturalWidth: image.naturalWidth };
      })()`, true, 30000);
      result.scenarios['50-14-delayed-image'] = await measureScrollAlignment(run.client, 50, 14);
      await stop(run);
      run = undefined;
      run = await launch(options.executable, resolve(corpusDirectory, 'scroll-fold-markers-1000.md'), 9361, runtimeDirectory);
      await selectLayout(run.client, 'split');
      result.fold = { before: await measureScrollAlignment(run.client, 50, 14) };
      result.fold.action = await run.client.evaluate(`(async () => {
        const gutter = document.querySelector('.cm-foldGutter');
        const marker = [...(gutter?.querySelectorAll('span[title]') ?? [])].find(
          span => span.title === 'Fold line' && span.getBoundingClientRect().height > 0
        );
        if (!marker) return { found: false, placeholderCount: 0 };
        marker.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
        await new Promise(requestAnimationFrame);
        await new Promise(requestAnimationFrame);
        return { found: true, placeholderCount: document.querySelectorAll('.cm-foldPlaceholder').length };
      })()`);
      result.fold.after = await measureScrollAlignment(run.client, 50, 14);
      await stop(run);
      run = undefined;
      const settingsRun = await launch(options.executable, resolve(corpusDirectory, 'representative-10-kib.md'), 9362, runtimeDirectory);
      try {
        await settingsRun.client.evaluate(`(async () => {
          const invoke = window.__TAURI_INTERNALS__.invoke;
          const settings = await invoke('get_settings');
          await invoke('update_settings', { settings: { ...settings, allowLocalImages: true } });
        })()`);
      } finally {
        await stop(settingsRun);
      }
      const imageFixture = await createImageScrollFixture(runtimeDirectory);
      run = await launch(options.executable, imageFixture.markdownPath, 9363, runtimeDirectory);
      await selectLayout(run.client, 'split');
      result.localImage = {
        fixture: imageFixture,
        before: await measureScrollAlignment(run.client, 50, 14)
      };
      result.localImage.loaded = await run.client.evaluate(`(async () => {
        const deadline = performance.now() + 30000;
        while (performance.now() < deadline) {
          const image = document.querySelector('.preview-content img');
          if (image?.complete && image.naturalWidth === 600) {
            return { loaded: true, naturalWidth: image.naturalWidth, naturalHeight: image.naturalHeight,
              objectUrl: image.currentSrc.startsWith('blob:') };
          }
          await new Promise(resolve => setTimeout(resolve, 50));
        }
        return { loaded: false };
      })()`, true, 35000);
      result.localImage.after = await measureScrollAlignment(run.client, 50, 14);
    } catch (error) {
      result.error = String(error);
    } finally {
      if (run) await stop(run).catch((error) => { result.teardownError = String(error); });
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, scenarios: Object.keys(result.scenarios), error: result.error }, null, 2)}\n`);
    return;
  }
  if (options.liveSplitReleaseOnly) {
    const allFiles = [
      'representative-10-kib.md', 'representative-100-kib.md',
      'representative-500-kib.md', 'representative-1-mib.md', 'representative-5-mib.md'
    ];
    if (options.liveSplitCase && !allFiles.includes(options.liveSplitCase)) {
      throw new Error(`Unknown --live-split-case: ${options.liveSplitCase}`);
    }
    const files = options.liveSplitCase ? [options.liveSplitCase] : allFiles;
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2, production separator pointer events',
        releaseStable: 'pointerup to three consecutive animation frames with unchanged stage/editor/preview widths; five second timeout',
        renderCount: 'production render_markdown calls observed through Tauri invoke during drag/release',
        samples: options.operationSamples
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      cases: {}
    };
    for (const [index, name] of files.entries()) {
      const file = manifest.files.find((entry) => entry.name === name);
      if (!file) throw new Error(`Missing corpus manifest entry: ${name}`);
      let run;
      try {
        run = await launch(options.executable, resolve(corpusDirectory, name), 9350 + index, runtimeDirectory);
        await selectLayout(run.client, 'split');
        const measured = await measureDividerRelease(run.client, options.operationSamples);
        result.cases[name] = {
          bytes: file.bytes, sha256: file.sha256,
          releaseStable: summary(measured.values), releaseStableRawMs: measured.values,
          failure: measured.failure,
          renderCount: measured.renderCount,
          budget: !measured.failure && measured.values.length === options.operationSamples && percentile(measured.values, 0.95) <= 100
        };
      } catch (error) {
        result.cases[name] = { bytes: file.bytes, sha256: file.sha256, error: String(error), budget: false };
      } finally {
        if (run) await stop(run).catch((error) => {
          result.cases[name].teardownError = String(error);
        });
        await mkdir(dirname(options.output), { recursive: true });
        await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      }
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, cases: Object.fromEntries(Object.entries(result.cases).map(([name, value]) => [name, value.error ?? value.releaseStable])) }, null, 2)}\n`);
    return;
  }
  if (options.liveSplitIpcOnly) {
    const allFiles = [
      'representative-10-kib.md', 'representative-100-kib.md',
      'representative-500-kib.md', 'representative-1-mib.md', 'representative-5-mib.md'
    ];
    if (options.liveSplitCase && !allFiles.includes(options.liveSplitCase)) {
      throw new Error(`Unknown --live-split-case: ${options.liveSplitCase}`);
    }
    const files = options.liveSplitCase ? [options.liveSplitCase] : allFiles;
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        surface: 'isolated Windows WebView2 launched with 10 KiB document, direct production render_markdown IPC in edit layout for each injected corpus',
        ipc: 'browser performance.now around Tauri invoke promise, including serialization and response transport',
        preparation: 'three detached template.innerHTML parses of final sanitized HTML; excludes mounted layout',
        samples: options.operationSamples
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      cases: {}
    };
    for (const [index, name] of files.entries()) {
      const file = manifest.files.find((entry) => entry.name === name);
      if (!file) throw new Error(`Missing corpus manifest entry: ${name}`);
      const source = await readFile(resolve(corpusDirectory, name));
      if (source.byteLength !== file.bytes || createHash('sha256').update(source).digest('hex') !== file.sha256) {
        throw new Error(`Corpus hash mismatch: ${name}`);
      }
      let run;
      try {
        run = await launch(options.executable, resolve(corpusDirectory, 'representative-10-kib.md'), 9340 + index, runtimeDirectory);
        await selectLayout(run.client, 'edit');
        const measured = await measureLiveSplitIpc(run.client, source.toString('utf8'), options.operationSamples);
        result.cases[name] = {
          bytes: file.bytes, sha256: file.sha256,
          ipc: summary(measured.ipc.map((sample) => sample.ipcMs)),
          ipcRawMs: measured.ipc.map((sample) => sample.ipcMs),
          htmlBytes: measured.ipc.at(-1).htmlBytes,
          outlineCount: measured.ipc.at(-1).outlineCount,
          preparation: summary(measured.preparation.map((sample) => sample.parseMs)),
          preparationRawMs: measured.preparation.map((sample) => sample.parseMs),
          preparationNodes: measured.preparation.at(-1).nodes
        };
      } catch (error) {
        result.cases[name] = { bytes: file.bytes, sha256: file.sha256, error: String(error) };
      } finally {
        if (run) await stop(run).catch((error) => {
          result.cases[name].teardownError = String(error);
        });
        await mkdir(dirname(options.output), { recursive: true });
        await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      }
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, cases: Object.fromEntries(Object.entries(result.cases).map(([name, value]) => [name, value.error ?? value.ipc])) }, null, 2)}\n`);
    return;
  }
  if (options.liveSplitOnly) {
    const smallCorpus = resolve(corpusDirectory, 'representative-10-kib.md');
    const settingsRun = await launch(options.executable, smallCorpus, 9330, runtimeDirectory);
    try {
      await settingsRun.client.evaluate(`(async () => {
        const invoke = window.__TAURI_INTERNALS__.invoke;
        const settings = await invoke('get_settings');
        await invoke('update_settings', { settings: { ...settings, syncScroll: true, livePreviewEnabled: true } });
      })()`);
    } finally {
      await stop(settingsRun);
    }
    const baseFiles = [
      'representative-10-kib.md', 'representative-100-kib.md',
      'representative-500-kib.md', 'representative-1-mib.md',
      'representative-5-mib.md', 'representative-10-mib.md'
    ];
    const mixedFiles = ['mixed-100-kib.md', 'mixed-500-kib.md'];
    const allFiles = [...baseFiles, ...mixedFiles];
    if (options.liveSplitCase && !allFiles.includes(options.liveSplitCase)) {
      throw new Error(`Unknown --live-split-case: ${options.liveSplitCase}`);
    }
    const files = options.liveSplitCase ? [options.liveSplitCase] : options.liveSplitMixedOnly ? mixedFiles : baseFiles;
    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        build: 'npm run tauri -- build --no-bundle (release, locked Cargo profile)',
        surface: 'real Windows WebView2 with isolated app data, live preview and sync scroll enabled',
        editorPaint: 'trusted CDP insertion to two animation frames',
        previewConvergence: 'trusted CDP insertion to changed preview text and two animation frames; 12 second timeout is failure',
        divider: 'trusted pointer drag through production separator',
        samples: options.operationSamples,
        boundary: '10 MiB virtual preview acceptance; sampled process working sets are not continuous peaks'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      corpus: manifest.files.filter((file) => files.includes(file.name)),
      cases: {}
    };
    for (const [index, name] of files.entries()) {
      const file = manifest.files.find((entry) => entry.name === name);
      if (!file) throw new Error(`Missing corpus manifest entry: ${name}`);
      let run;
      let resourceSampler;
      let stage = 'launch';
      try {
        run = await launch(options.executable, resolve(corpusDirectory, name), 9331 + index, runtimeDirectory,
          resolve(runtimeDirectory, 'webview2'), name === 'representative-10-mib.md'
            ? (child) => { resourceSampler = startProcessTreeSampler(child.pid); }
            : undefined);
        stage = 'edit';
        await selectLayout(run.client, 'edit');
        const edit = await measureEditorPaint(run.client, options.operationSamples);
        stage = 'split';
        await selectLayout(run.client, 'split');
        const split = await measureEditorPaint(run.client, options.operationSamples);
        stage = 'preview';
        const preview = await measureInputToPaint(run.client, options.operationSamples, options.diagnosePreview);
        stage = 'divider';
        const divider = await measureDividerGhost(run.client, options.operationSamples);
        const dom = await run.client.evaluate(`(() => ({
          previewNodes: document.querySelector('.preview-content')?.querySelectorAll('*').length ?? null,
          previewScrollHeight: document.querySelector('.preview-content')?.scrollHeight ?? null,
          editorScrollHeight: document.querySelector('.cm-scroller')?.scrollHeight ?? null,
          devicePixelRatio: window.devicePixelRatio
        }))()`);
        const virtualViewport = name === 'representative-10-mib.md'
          ? await measureVirtualPreviewViewport(run.client, run.child.pid)
          : null;
        const sampledResources = resourceSampler ? await resourceSampler.stop() : null;
        resourceSampler = null;
        result.cases[name] = {
          bytes: file.bytes, sha256: file.sha256, dom, virtualViewport, sampledResources,
          edit: summary(edit.values), editRawMs: edit.values, editLongTasks: summary(edit.longTasks),
          split: summary(split.values), splitRawMs: split.values, splitLongTasks: summary(split.longTasks),
          preview: summary(preview.values), previewRawMs: preview.values, previewLongTasks: summary(preview.longTasks),
          divider: summary(divider.frames), dividerRawMs: divider.frames,
          resources: await processResources(run.child.pid),
          budgets: {
            editP95AtMost50Ms: percentile(edit.values, 0.95) <= 50,
            splitWithinTwentyPercentOfEdit: percentile(split.values, 0.95) <= percentile(edit.values, 0.95) * 1.2,
            dividerP95AtMost20Ms: percentile(divider.frames, 0.95) <= 20,
            dividerP99AtMost50Ms: percentile(divider.frames, 0.99) <= 50,
            ...(virtualViewport ? {
              virtualWindowNodesAtMost10000: virtualViewport.checkpoints.every((checkpoint) => checkpoint.nodes <= 10_000),
              virtualWindowNeverBlank: virtualViewport.checkpoints.every((checkpoint) => checkpoint.visibleSegments > 0),
              processTreeAtMost2GiB: sampledResources.peakProcessTreeBytes <= 2 * 1024 ** 3,
              rendererAtMost1GiB: sampledResources.peakWebView2Bytes <= 1024 ** 3
            } : {})
          }
        };
      } catch (error) {
        result.cases[name] = { bytes: file.bytes, sha256: file.sha256, stage, error: String(error),
          resources: run ? await processTreeResources(run.child.pid) : null,
          sampledResources: resourceSampler ? await resourceSampler.stop() : null, budgets: null };
        resourceSampler = null;
      } finally {
        if (resourceSampler) await resourceSampler.stop();
        if (run) await stop(run).catch((error) => {
          result.cases[name].teardownError = String(error);
        });
        await mkdir(dirname(options.output), { recursive: true });
        await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      }
    }
    if (!options.liveSplitCase) {
      let scrollRun;
      try {
        scrollRun = await launch(options.executable, resolve(corpusDirectory, 'scroll-markers-1000.md'), 9337, runtimeDirectory);
        await selectLayout(scrollRun.client, 'split');
        result.scrollAlignment = await measureScrollAlignment(scrollRun.client);
      } catch (error) {
        result.scrollAlignment = { error: String(error) };
      } finally {
        if (scrollRun) await stop(scrollRun).catch((error) => {
          result.scrollAlignment.teardownError = String(error);
        });
        await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      }
    }
    process.stdout.write(`${JSON.stringify({ output: options.output, cases: Object.fromEntries(Object.entries(result.cases).map(([name, value]) => [name, value.error ?? value.budgets])) }, null, 2)}\n`);
    if (Object.values(result.cases).some((value) => value.error || value.teardownError || Object.values(value.budgets ?? {}).some((passed) => !passed))) process.exitCode = 1;
    return;
  }
  if (options.exportSemanticsOnly) {
    const fixture = await createExportSemanticsFixture();
    const run = await launch(options.executable, fixture.markdownPath, 9324, runtimeDirectory);
    try {
      const baseRequest = {
        snapshot: {
          jobId: 'export-semantics-0060',
          tabId: 'export-semantics-0060',
          contentRevision: 1,
          sourcePath: fixture.markdownPath,
          title: 'Export semantics',
          content: fixture.markdown
        },
        options: {
          paperSize: 'a4',
          orientation: 'portrait',
          margin: 'normal',
          includeTitle: true,
          includeLocalImages: false
        },
        mindMapSvg: null
      };
      const requests = [
        { ...baseRequest, targetPath: fixture.htmlPath, format: 'html' },
        { ...baseRequest, targetPath: fixture.docxPath, format: 'docx' },
        { ...baseRequest, targetPath: fixture.pdfPath, format: 'pdf' }
      ];
      const exportResults = await run.client.evaluate(`(async () => {
        const requests = ${JSON.stringify(requests)};
        const results = [];
        for (const request of requests) {
          try {
            results.push(await window.__TAURI_INTERNALS__.invoke('export_document', { request }));
          } catch (error) {
            throw new Error(JSON.stringify({ format: request.format, error }));
          }
        }
        return results;
      })()`, true, 120_000);
      const [html, docx, pdf] = await Promise.all([
        readFile(fixture.htmlPath),
        readFile(fixture.docxPath),
        readFile(fixture.pdfPath)
      ]);
      const artifacts = {
        html: { bytes: html.byteLength, sha256: createHash('sha256').update(html).digest('hex'), hasExpectedHeader: html.subarray(0, 15).toString('utf8').toLowerCase().startsWith('<!doctype html>') },
        docx: { bytes: docx.byteLength, sha256: createHash('sha256').update(docx).digest('hex'), hasExpectedHeader: docx.subarray(0, 2).toString('ascii') === 'PK' },
        pdf: { bytes: pdf.byteLength, sha256: createHash('sha256').update(pdf).digest('hex'), hasExpectedHeader: pdf.subarray(0, 5).toString('ascii') === '%PDF-' }
      };
      const result = {
        schemaVersion: 1,
        capturedAt: new Date().toISOString(),
        methodology: {
          build: 'npm run tauri -- build --no-bundle (release, default locked Cargo profile)',
          surface: 'one fixed Markdown snapshot exported through production Tauri IPC to HTML, DOCX, and real Windows WebView2 PDF',
          fixture: 'heading/bookmark, list and inline style, external link, distinct soft/hard breaks, aligned GFM table, and styled footnote'
        },
        artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
        toolchain,
        fixture: {
          markdownBytes: Buffer.byteLength(fixture.markdown),
          markdownSha256: createHash('sha256').update(fixture.markdown).digest('hex')
        },
        exportResults: exportResults.map((entry) => ({ ...entry, path: basename(entry.path) })),
        artifacts,
        budgets: {
          allFormatsReturned: exportResults.map((entry) => entry.format).join(',') === 'html,docx,pdf',
          htmlValid: artifacts.html.bytes > 1024 && artifacts.html.hasExpectedHeader,
          docxValid: artifacts.docx.bytes > 1024 && artifacts.docx.hasExpectedHeader,
          pdfValid: artifacts.pdf.bytes > 1024 && artifacts.pdf.hasExpectedHeader
        }
      };
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      if (Object.values(result.budgets).some((passed) => !passed)) {
        throw new Error(`Export semantics budget failed: ${JSON.stringify(result.budgets)}`);
      }
      process.stdout.write(`${JSON.stringify({ output: options.output, budgets: result.budgets, artifacts: result.artifacts, warnings: result.exportResults.map((entry) => entry.warnings) }, null, 2)}\n`);
    } finally {
      await stop(run);
    }
    return;
  }
  if (options.pdfResourceIsolationOnly) {
    const capture = await startResourceCapture();
    let run;
    try {
      const fixture = await createPdfResourceFixture(runtimeDirectory, capture.endpoint);
      run = await launch(options.executable, fixture.markdownPath, 9323, runtimeDirectory);
      const request = {
        snapshot: {
          jobId: 'pdf-resource-isolation-0059',
          tabId: 'pdf-resource-isolation-0059',
          contentRevision: 1,
          sourcePath: fixture.markdownPath,
          title: 'Resource policy',
          content: fixture.markdown
        },
        targetPath: fixture.pdfPath,
        format: 'pdf',
        options: {
          paperSize: 'a4',
          orientation: 'portrait',
          margin: 'normal',
          includeTitle: true,
          includeLocalImages: true
        },
        mindMapSvg: null
      };
      const exportResult = await run.client.evaluate(`(async () => {
        try {
          return await window.__TAURI_INTERNALS__.invoke('export_document', { request: ${JSON.stringify(request)} });
        } catch (error) {
          throw new Error(JSON.stringify(error));
        }
      })()`, true, 90_000);
      const pdf = await readFile(fixture.pdfPath);
      const normalizedMarkdown = fixture.markdown.replaceAll(capture.endpoint, 'http://127.0.0.1:<capture>');
      const normalizedExportResult = {
        ...exportResult,
        path: basename(exportResult.path),
        warnings: exportResult.warnings.map((warning) => ({
          ...warning,
          target: typeof warning.target === 'string'
            ? warning.target.replaceAll(capture.endpoint, 'http://127.0.0.1:<capture>')
            : warning.target
        }))
      };
      const result = {
        schemaVersion: 1,
        capturedAt: new Date().toISOString(),
        methodology: {
          build: 'npm run tauri -- build --no-bundle (release, default locked Cargo profile)',
          surface: 'real Windows WebView2 native PDF export invoked through production Tauri IPC',
          capture: 'loopback HTTP server records every request path while Markdown/raw HTML reference that endpoint; zero requests is required',
          fixture: 'two canonical aliases for one local PNG, one Markdown remote image, raw img src/srcset, stylesheet, CSS import/background, video poster, and object data'
        },
        artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
        toolchain,
        fixture: {
          normalizedMarkdownBytes: Buffer.byteLength(normalizedMarkdown),
          normalizedMarkdownSha256: createHash('sha256').update(normalizedMarkdown).digest('hex'),
          imageBytes: fixture.image.byteLength,
          imageSha256: createHash('sha256').update(fixture.image).digest('hex')
        },
        exportResult: normalizedExportResult,
        capturedRequests: capture.requests,
        pdf: {
          bytes: pdf.byteLength,
          sha256: createHash('sha256').update(pdf).digest('hex'),
          hasPdfHeader: pdf.subarray(0, 5).toString('ascii') === '%PDF-'
        },
        budgets: {
          noNetworkSubresourceRequests: capture.requests.length === 0,
          localImageEmbeddedWithoutNetwork: exportResult.warnings.every((warning) => warning.code !== 'LOCAL_IMAGE_NOT_EMBEDDED' && warning.code !== 'IMAGE_READ_FAILED'),
          remoteMarkdownImageWarned: exportResult.warnings.some((warning) => warning.code === 'REMOTE_IMAGE_SKIPPED'),
          rawHtmlImageWarned: exportResult.warnings.some((warning) => warning.code === 'RAW_HTML_IMAGE_SKIPPED'),
          validPdfArtifact: pdf.byteLength > 1024 && pdf.subarray(0, 5).toString('ascii') === '%PDF-'
        }
      };
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      if (Object.values(result.budgets).some((passed) => !passed)) {
        throw new Error(`PDF resource isolation budget failed: ${JSON.stringify(result.budgets)}`);
      }
      process.stdout.write(`${JSON.stringify({ output: options.output, budgets: result.budgets, capturedRequests: result.capturedRequests, warnings: normalizedExportResult.warnings, pdf: result.pdf }, null, 2)}\n`);
    } finally {
      if (run) await stop(run);
      await capture.close();
    }
    return;
  }
  if (options.previewResourcesOnly) {
    const fixture = await createPreviewResourceFixture(runtimeDirectory);
    const settingsRun = await launch(options.executable, resolve(corpusDirectory, 'representative-10-kib.md'), 9321, runtimeDirectory);
    try {
      await settingsRun.client.evaluate(`(async () => {
        const invoke = window.__TAURI_INTERNALS__.invoke;
        const settings = await invoke('get_settings');
        await invoke('update_settings', { settings: { ...settings, allowLocalImages: true, livePreviewEnabled: true, syncScroll: false } });
      })()`);
    } finally {
      await stop(settingsRun);
    }
    const run = await launch(options.executable, fixture.markdownPath, 9322, runtimeDirectory);
    try {
      const measured = await measurePreviewResources(run, 100, options.expectLegacy);
      const result = {
        schemaVersion: 1,
        capturedAt: new Date().toISOString(),
        methodology: {
          build: 'npm run tauri -- build --no-bundle (release, default locked Cargo profile)',
          surface: 'real Windows WebView2 in isolated app-data and WebView profile',
          fixture: '64 distinct source aliases for one 1x1 PNG plus one visible 4096x4096 RGBA PNG, a 5000-row table, code block, link, and headings',
          lifecycle: '100 article/mind-map cycles; URL creation/revocation instrumented after the initial article is released; visible large image decode awaited per cycle',
          memory: 'V8 HeapProfiler GC and main-process WorkingSet64/HandleCount before and after cycles; system/WebView child-process reclamation is not inferred'
        },
        artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
        fixture: {
          markdownBytes: fixture.markdownBytes,
          imageBytes: fixture.imageBytes,
          largeImageBytes: fixture.largeImageBytes,
          estimatedDecodedBytes: fixture.estimatedDecodedBytes,
          markdownSha256: createHash('sha256').update(await readFile(fixture.markdownPath)).digest('hex'),
          imageSha256: createHash('sha256').update(await readFile(fixture.imagePath)).digest('hex'),
          largeImageSha256: createHash('sha256').update(await readFile(fixture.largeImagePath)).digest('hex')
        },
        toolchain,
        legacyExpectation: options.expectLegacy,
        ...measured,
        budgets: {
          aliasesUseOneObjectUrl: measured.article.images === 65 && measured.article.uniqueObjectUrls === 2,
          imagesUseLazyAsyncDecode: measured.article.lazyImages === 65,
          scrollP95AtMost20Ms: measured.scroll.p95Ms <= 20,
          scrollP99AtMost50Ms: measured.scroll.p99Ms <= 50,
          lifecycleReturnsToZero: measured.ownership.created === 200 && measured.ownership.revoked === 200 && measured.ownership.outstanding === 0 && measured.ownership.peakOutstanding === 2,
          heapGrowthAtMost8MiB: measured.heapAfter.usedSize <= measured.heapBefore.usedSize + 8 * 1024 * 1024,
          mainWorkingSetGrowthAtMost32MiB: measured.processAfter.WorkingSet64 <= measured.processBefore.WorkingSet64 + 32 * 1024 * 1024,
          mainHandleGrowthAtMost8: measured.processAfter.HandleCount <= measured.processBefore.HandleCount + 8
        }
      };
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      if (!options.expectLegacy && Object.values(result.budgets).some((passed) => !passed)) {
        throw new Error(`Preview resource budget failed: ${JSON.stringify(result.budgets)}`);
      }
      process.stdout.write(`${JSON.stringify({ output: options.output, budgets: result.budgets, article: result.article, scroll: result.scroll, ownership: result.ownership, heapBefore: result.heapBefore, heapAfter: result.heapAfter, processBefore: result.processBefore, processAfter: result.processAfter }, null, 2)}\n`);
    } finally {
      await stop(run);
    }
    return;
  }
  if (options.interactionOnly) {
    const smallCorpus = resolve(corpusDirectory, 'representative-10-kib.md');
    const largeCorpus = resolve(corpusDirectory, 'representative-5-mib.md');
    const settingsRun = await launch(options.executable, smallCorpus, 9318, runtimeDirectory);
    try {
      await settingsRun.client.evaluate(`(async () => {
        const invoke = window.__TAURI_INTERNALS__.invoke;
        const settings = await invoke('get_settings');
        await invoke('update_settings', { settings: { ...settings, syncScroll: false, livePreviewEnabled: false } });
      })()`);
    } finally {
      await stop(settingsRun);
    }

    const largeRun = await launch(options.executable, largeCorpus, 9319, runtimeDirectory);
    let large;
    try {
      const divider = await measureDividerGhost(largeRun.client, options.operationSamples, options.expectLegacy);
      await selectLayout(largeRun.client, 'edit');
      const edit = await measureEditorPaint(largeRun.client, options.operationSamples);
      await selectLayout(largeRun.client, 'split');
      const split = await measureEditorPaint(largeRun.client, options.operationSamples);
      large = {
        edit: summary(edit.values), editRawMs: edit.values, editLongTasks: summary(edit.longTasks),
        split: summary(split.values), splitRawMs: split.values, splitLongTasks: summary(split.longTasks),
        divider: summary(divider.frames), dividerRawMs: divider.frames,
        dividerOwnership: { before: divider.before.stageStyle, during: divider.during, after: divider.after }
      };
    } finally {
      await stop(largeRun);
    }

    const smallRun = await launch(options.executable, smallCorpus, 9320, runtimeDirectory);
    let small;
    try {
      await selectLayout(smallRun.client, 'edit');
      const edit = await measureEditorPaint(smallRun.client, options.operationSamples);
      small = { edit: summary(edit.values), editRawMs: edit.values, editLongTasks: summary(edit.longTasks) };
    } finally {
      await stop(smallRun);
    }

    const result = {
      schemaVersion: 1,
      capturedAt: new Date().toISOString(),
      methodology: {
        build: 'npm run tauri -- build --no-bundle (release, default locked Cargo profile)',
        surface: 'real Windows WebView2 in an isolated app-data and WebView profile',
        editorPaint: 'trusted CDP insertion through CodeMirror to two animation frames; preview/semantic convergence is asynchronous and excluded',
        divider: 'trusted pointer drag through the production separator; frame intervals plus real-grid/ghost ownership are checked',
        settings: 'syncScroll=false and livePreviewEnabled=false persisted only in isolated benchmark app-data so edit/split samples exclude deferred preview work'
      },
      artifact: { name: basename(options.executable), sha256: createHash('sha256').update(executable).digest('hex'), bytes: executable.byteLength },
      toolchain,
      legacyExpectation: options.expectLegacy,
      large,
      small,
      budgets: {
        largeEditP95AtMost50Ms: large.edit.p95Ms <= 50,
        splitWithinTwentyPercentOfEdit: large.split.p95Ms <= large.edit.p95Ms * 1.2,
        dividerP95AtMost20Ms: large.divider.p95Ms <= 20,
        dividerP99AtMost50Ms: large.divider.p99Ms <= 50,
        smallEditP95AtMost50Ms: small.edit.p95Ms <= 50
      }
    };
    await mkdir(dirname(options.output), { recursive: true });
    await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    process.stdout.write(`${JSON.stringify({ output: options.output, budgets: result.budgets, large: result.large.edit, small: result.small.edit }, null, 2)}\n`);
    return;
  }
  if (options.searchOnly) {
    const corpus = manifest.files.find((file) => file.kind === 'denseSearch');
    if (!corpus) throw new Error('The fixed corpus manifest has no denseSearch case');
    const run = await launch(options.executable, resolve(corpusDirectory, corpus.name), 9320, runtimeDirectory);
    try {
      const beforeResources = await processResources(run.child.pid);
      const measurement = await measureDenseSearch(run.client);
      const afterResources = await processResources(run.child.pid);
      const result = {
        schemaVersion: 1,
        capturedAt: new Date().toISOString(),
        methodology: {
          build: 'npm run tauri -- build --no-bundle (release, default locked Cargo profile)',
          surface: 'real Windows WebView2 driven through the existing isolated CDP benchmark mode',
          search: 'Ctrl+F plus trusted CDP text input on the fixed 5 MiB dense-search corpus',
          edit: 'trusted CDP insertion while search remains open, measured to editor mutation plus two frames',
          scroll: 'real requestAnimationFrame intervals while changing the CodeMirror scroller',
          measurementPerturbation: 'CDP loopback, MutationObserver, PerformanceObserver and process sampling are benchmark-only'
        },
        corpus,
        startupMs: run.startupMs,
        webViewUserAgent: run.ready.userAgent,
        toolchain,
        beforeResources,
        ...measurement,
        afterResources
      };
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      process.stdout.write(`${JSON.stringify({ output: options.output, search: result.search, edit: result.edit, runtime: result.runtime }, null, 2)}\n`);
    } finally {
      await stop(run);
    }
    return;
  }
  if (options.mindMapOnly) {
    const corpus = manifest.files.find((file) => file.kind === 'manyHeadings');
    if (!corpus) throw new Error('The fixed corpus manifest has no manyHeadings case');
    const run = await launch(options.executable, resolve(corpusDirectory, corpus.name), 9320, runtimeDirectory);
    try {
      const beforeResources = await processResources(run.child.pid);
      const measurement = await measureMindMapWebView(run.client);
      const afterResources = await processResources(run.child.pid);
      if (measurement.totalLayoutNodes !== 150_001 || !measurement.bottom.hasLastHeading || measurement.errors.length) {
        throw new Error(`Mind-map integrity check failed: ${JSON.stringify(measurement)}`);
      }
      const result = {
        schemaVersion: 1,
        capturedAt: new Date().toISOString(),
        methodology: {
          build: 'npm run tauri -- build --no-bundle (release, default locked Cargo profile)',
          surface: 'real Windows WebView2 driven through the existing isolated CDP benchmark mode',
          interaction: 'switch article to mind map, inspect the complete layout count, sample ten full-range scroll frames, verify the final heading, then complete three destroy cycles',
          memory: 'V8 HeapProfiler garbage collection before opening and after each of three mind-map component destroy cycles; process resources are observational'
        },
        corpus,
        startupMs: run.startupMs,
        webViewUserAgent: run.ready.userAgent,
        toolchain,
        beforeResources,
        ...measurement,
        afterResources
      };
      await mkdir(dirname(options.output), { recursive: true });
      await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
      process.stdout.write(`${JSON.stringify({ output: options.output, switchMs: result.switchMs, renderedLayoutNodes: result.renderedLayoutNodes, bottom: result.bottom, heapBefore: result.heapBefore, heapAfterRelease: result.heapAfterRelease }, null, 2)}\n`);
    } finally {
      await stop(run);
    }
    return;
  }
  const startup = [];
  const startupResources = [];
  let userAgent = null;

  for (let index = 0; index < startupSamples; index += 1) {
    process.stdout.write(`startup ${index + 1}/${startupSamples}\n`);
    const run = await launch(options.executable, resolve(corpusDirectory, 'representative-10-kib.md'), 9320 + index, runtimeDirectory);
    startup.push(run.startupMs);
    startupResources.push(await processResources(run.child.pid));
    userAgent ??= run.ready.userAgent;
    await stop(run);
  }
  const diagnosticStartup = await startupDiagnosticDurations(runtimeDirectory, startupSamples);

  const corpusLoads = [];
  const corpusCases = options.corpusLimit === null ? manifest.files : manifest.files.slice(0, options.corpusLimit);
  for (let index = 0; index < corpusCases.length; index += 1) {
    const file = corpusCases[index];
    process.stdout.write(`corpus ${index + 1}/${corpusCases.length}: ${file.name}\n`);
    try {
      const run = await launch(options.executable, resolve(corpusDirectory, file.name), 9420 + index, runtimeDirectory);
      corpusLoads.push({ name: file.name, interactiveMs: run.startupMs, resources: await processResources(run.child.pid) });
      await stop(run);
    } catch (error) {
      corpusLoads.push({ name: file.name, error: String(error) });
    }
  }

  const representative = await readFile(resolve(corpusDirectory, 'representative-10-kib.md'), 'utf8');
  process.stdout.write('operations: representative-500-kib.md\n');
  let operations;
  try {
    const run = await launch(options.executable, resolve(corpusDirectory, 'representative-500-kib.md'), 9520, runtimeDirectory);
    try {
      const input = await measureInputToPaint(run.client);
      const scroll = await measureFrames(run.client, '.cm-scroller', 'scroll');
      const split = await measureFrames(run.client, '.editor-stage', 'split');
      const nativeRender = await measureNativeRender(run.client, representative);
      const exportValues = await measureExports(run.client, representative, resolve(runtimeDirectory, 'exports'));
      const finalResources = await processResources(run.child.pid);
      operations = {
        calibrationCorpus: 'representative-500-kib.md',
        inputToPreviewPaint: summary(input.values),
        inputRawMs: input.values,
        inputLongTasks: summary(input.longTasks),
        editorScrollFrames: summary(scroll.frames),
        editorScrollRawMs: scroll.frames,
        editorScrollLongTasks: summary(scroll.longTasks),
        splitFrames: summary(split.frames),
        splitRawMs: split.frames,
        splitLongTasks: summary(split.longTasks),
        nativeRender10KiB: summary(nativeRender),
        nativeRenderRawMs: nativeRender,
        htmlExport10KiB: summary(exportValues.html),
        htmlExportRawMs: exportValues.html,
        docxExport10KiB: summary(exportValues.docx),
        docxExportRawMs: exportValues.docx,
        finalResources
      };
    } finally {
      await stop(run);
    }
  } catch (error) {
    operations = { error: String(error) };
  }

  process.stdout.write('boundary input: representative-5-mib.md\n');
  try {
    const boundaryRun = await launch(options.executable, resolve(corpusDirectory, 'representative-5-mib.md'), 9521, runtimeDirectory);
    try {
      const boundaryInput = await measureInputToPaint(boundaryRun.client, 1);
      operations.input5MiBBoundary = {
        samples: boundaryInput.values.length,
        rawMs: boundaryInput.values,
        longTasks: boundaryInput.longTasks
      };
    } finally {
      await stop(boundaryRun);
    }
  } catch (error) {
    operations.input5MiBBoundary = { samples: 0, error: String(error) };
  }

  const activePowerSchemeOutput = await command('powercfg.exe', ['/getactivescheme']).catch(() => '');
  const activePowerScheme = activePowerSchemeOutput.match(/[0-9a-f]{8}-[0-9a-f-]{27}/i)?.[0] ?? 'unavailable';
  const result = {
    schemaVersion: 1,
    capturedAt: new Date().toISOString(),
    methodology: {
      build: 'npm run tauri -- build --no-bundle (release, default locked Cargo profile)',
      startup: 'process spawn to two requestAnimationFrame callbacks after interactive editor DOM; first sample cold within this run, remaining samples warm',
      inputToPaint: 'trusted CDP text insertion to preview DOM mutation plus two requestAnimationFrame callbacks',
      frames: 'real WebView requestAnimationFrame intervals while scrolling or changing split CSS; first interval discarded',
      longTasks: 'Web PerformanceObserver longtask entries; absence is not proof of zero native work',
      resources: 'Windows process working set and handle count; system reclamation is not forced',
      measurementPerturbation: 'CDP loopback, PerformanceObserver and process sampling are enabled only for this run'
    },
    referenceMachine: {
      hostnameHash: createHash('sha256').update(hostname()).digest('hex'),
      os: `${platform()} ${release()}`,
      cpu: cpus()[0]?.model ?? 'unknown',
      logicalCpus: cpus().length,
      totalMemoryBytes: totalmem(),
      freeMemoryBytesAtSummary: freemem(),
      powerScheme: activePowerScheme,
      fonts: ['system-ui', 'Consolas'],
      webViewUserAgent: userAgent,
      ...toolchain
    },
    corpus: manifest,
    artifact: {
      name: basename(options.executable),
      sha256: createHash('sha256').update(executable).digest('hex'),
      bytes: executable.byteLength,
      gzipBytes: gzipSync(executable, { level: 9 }).byteLength,
      requiredFiles: [basename(options.executable)],
      excluded: ['src-tauri/target build cache', 'system-provided WebView2 runtime', 'generated corpus']
    },
    startup: summary(startup),
    startupRawMs: startup,
    startupDiagnosticWallClock: summary(diagnosticStartup),
    startupProbeDeltaMs: {
      p50: summary(startup).p50Ms - summary(diagnosticStartup).p50Ms,
      p95: summary(startup).p95Ms - summary(diagnosticStartup).p95Ms
    },
    startupResources,
    corpusLoads,
    operations,
    budgets: {
      inputP95AtMost50Ms: operations.input5MiBBoundary?.rawMs
        ? percentile(operations.input5MiBBoundary.rawMs, 0.95) <= 50
        : false,
      scrollFrameP95AtMost20Ms: operations.editorScrollRawMs ? percentile(operations.editorScrollRawMs, 0.95) <= 20 : false,
      scrollFrameP99AtMost50Ms: operations.editorScrollRawMs ? percentile(operations.editorScrollRawMs, 0.99) <= 50 : false,
      splitFrameP95AtMost20Ms: operations.splitRawMs ? percentile(operations.splitRawMs, 0.95) <= 20 : false,
      splitFrameP99AtMost50Ms: operations.splitRawMs ? percentile(operations.splitRawMs, 0.99) <= 50 : false
    }
  };

  await mkdir(dirname(options.output), { recursive: true });
  await writeFile(options.output, `${JSON.stringify(result, null, 2)}\n`, 'utf8');
  process.stdout.write(`${JSON.stringify({ output: options.output, budgets: result.budgets }, null, 2)}\n`);
}

await main();
