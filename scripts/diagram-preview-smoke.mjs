import { spawn } from 'node:child_process';
import { access, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { get as httpGet } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { build } from 'esbuild';

const root = resolve(import.meta.dirname, '..');
const workspace = await mkdtemp(join(tmpdir(), 'marklite-diagram-preview-'));
const evidencePath = join(root, 'Review', 'diagram-preview-smoke-0093.json');

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const readJson = (url) => new Promise((resolve, reject) => {
  httpGet(url, (response) => {
    let body = '';
    response.setEncoding('utf8');
    response.on('data', (chunk) => { body += chunk; });
    response.on('end', () => {
      try { resolve(JSON.parse(body)); } catch (error) { reject(error); }
    });
  }).on('error', reject);
});

async function runBrowser(edge, pagePath, workspace) {
  const profile = join(workspace, 'edge-profile');
  const browser = spawn(edge, [
    '--headless=new', '--disable-gpu', '--allow-file-access-from-files',
    `--user-data-dir=${profile}`, '--remote-debugging-port=0', pathToFileURL(pagePath).href
  ], { windowsHide: true, stdio: 'ignore' });
  let socket;
  try {
    let port;
    for (let attempt = 0; attempt < 100; attempt += 1) {
      try {
        port = Number((await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0]);
        if (port) break;
      } catch {}
      await pause(100);
    }
    if (!port) throw new Error('Edge remote debugging port did not open');
    let page;
    for (let attempt = 0; attempt < 100; attempt += 1) {
      const pages = await readJson(`http://127.0.0.1:${port}/json/list`);
      page = pages.find((item) => item.type === 'page' && item.url === pathToFileURL(pagePath).href);
      if (page) break;
      await pause(100);
    }
    if (!page) throw new Error('Edge did not open the preview smoke page');
    socket = new WebSocket(page.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      socket.addEventListener('open', resolve, { once: true });
      socket.addEventListener('error', reject, { once: true });
    });
    let id = 0;
    const replies = new Map();
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data);
      const reply = replies.get(message.id);
      if (reply) { replies.delete(message.id); reply(message); }
    });
    const call = (method, params = {}) => new Promise((resolve) => {
      const requestId = ++id;
      replies.set(requestId, resolve);
      socket.send(JSON.stringify({ id: requestId, method, params }));
    });
    let resultText;
    for (let attempt = 0; attempt < 700; attempt += 1) {
      const response = await call('Runtime.evaluate', {
        expression: "document.getElementById('result')?.textContent",
        returnByValue: true
      });
      resultText = response.result?.result?.value;
      if (resultText && resultText !== 'pending') break;
      await pause(100);
    }
    if (!resultText || resultText === 'pending') throw new Error('Real-time Edge preview smoke timed out');
    return JSON.parse(Buffer.from(resultText, 'base64').toString('utf8'));
  } finally {
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ id: 999_999, method: 'Browser.close' }));
      await pause(300);
    }
    socket?.close();
    if (browser.exitCode === null) browser.kill();
    if (browser.exitCode === null) await new Promise((resolve) => browser.once('exit', resolve));
  }
}

try {
  const edgeCandidates = [
    process.env.PROGRAMFILES && join(process.env.PROGRAMFILES, 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
    process.env['PROGRAMFILES(X86)'] && join(process.env['PROGRAMFILES(X86)'], 'Microsoft', 'Edge', 'Application', 'msedge.exe')
  ].filter(Boolean);
  let edge;
  for (const candidate of edgeCandidates) {
    try { await access(candidate); edge = candidate; break; } catch {}
  }
  if (!edge) throw new Error('Microsoft Edge was not found');

  const bundlePath = join(workspace, 'port.js');
  await build({
    entryPoints: [join(root, 'src', 'lib', 'mermaidSandboxPort.ts')],
    bundle: true,
    format: 'esm',
    platform: 'browser',
    outfile: bundlePath
  });
  const runtime = await readFile(join(root, 'node_modules', 'mermaid', 'dist', 'mermaid.min.js'), 'utf8');
  const runtimeLiteral = JSON.stringify(runtime).replaceAll('</script', '<\\/script');
  const lifecycleRuntimeLiteral = JSON.stringify(
    'globalThis.mermaid={initialize(){},async render(){return {svg: \'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><text>cycle</text></svg>\'}}}'
  );
  const pagePath = join(workspace, 'preview.html');
  const page = `<!doctype html><html><body><pre id="result">pending</pre><script type="module">
import { MermaidSandboxPort } from './port.js';
const runtimeScript = ${runtimeLiteral};
const encode = (value) => { const bytes = new TextEncoder().encode(JSON.stringify(value)); let binary = ''; for (const byte of bytes) binary += String.fromCharCode(byte); return btoa(binary); };
const source = { diagramId: 'diagram-0-aaaaaaaaaaaa', ordinal: 0, sourceUtf8: 'flowchart TD\\nA-->B', sourceSha256: 'a'.repeat(64), sourceStartByte: 0, sourceEndByte: 18 };
const port = new MermaidSandboxPort(async () => ({ rendererId: 'mermaid-offline-11.17.2', scriptUtf8: runtimeScript }));
setTimeout(() => {
  if (document.getElementById('result').textContent === 'pending') {
    document.getElementById('result').textContent = encode({ ok: false, error: 'timed out', remainingFrames: document.querySelectorAll('iframe').length });
  }
}, 65000);
try {
  const results = [];
  for (const [index, theme, fontFamily, fontSize] of [[0, 'light', 'Arial, system-ui, sans-serif', 16], [1, 'dark', 'Segoe UI, system-ui, sans-serif', 18]]) {
    const execution = port.start({ ...source, diagramId: 'diagram-' + index + '-aaaaaaaaaaaa' }, {
      rendererId: 'mermaid-offline-11.17.2', configVersion: 2, theme,
      fontKey: fontFamily + '\\0' + fontSize + '\\0' + 1.6,
      fontFamily, fontSize, lineHeight: 1.6
    });
    const result = await execution.result;
    results.push({ theme, fontFamily, fontSize, svgBytes: new TextEncoder().encode(result.svgUtf8).length, viewBox: result.viewBox });
  }
  port.release();
  const lifecyclePort = new MermaidSandboxPort(async () => ({
    rendererId: 'mermaid-offline-11.17.2',
    scriptUtf8: ${lifecycleRuntimeLiteral}
  }));
  for (let index = 0; index < 100; index += 1) {
    const execution = lifecyclePort.start({ ...source, diagramId: 'diagram-' + index + '-aaaaaaaaaaaa' }, {
      rendererId: 'mermaid-offline-11.17.2', configVersion: 2, theme: 'light',
      fontKey: 'Arial\\0' + 16 + '\\0' + 1.6,
      fontFamily: 'Arial', fontSize: 16, lineHeight: 1.6
    });
    await execution.result;
    lifecyclePort.release();
    if (document.querySelectorAll('iframe').length !== 0) throw new Error('Sandbox iframe remained after cycle ' + index);
  }
  document.getElementById('result').textContent = encode({ ok: true, results, lifecycleCycles: 100, remainingFrames: document.querySelectorAll('iframe').length });
} catch (error) {
  port.release();
  document.getElementById('result').textContent = encode({ ok: false, error: error instanceof Error ? error.stack : String(error), remainingFrames: document.querySelectorAll('iframe').length });
}
</script></body></html>`;
  await writeFile(pagePath, page, 'utf8');
  const result = await runBrowser(edge, pagePath, workspace);
  if (!result.ok || result.results.length !== 2 || result.lifecycleCycles !== 100 || result.remainingFrames !== 0) {
    throw new Error(JSON.stringify(result));
  }
  const evidence = { schemaVersion: 1, taskCard: 'TaskCards/done/0093-integrate-mermaid-preview-lifecycle.md', browser: 'Microsoft Edge headless', ...result };
  await writeFile(evidencePath, `${JSON.stringify(evidence, null, 2)}\n`, 'utf8');
  console.log(JSON.stringify(evidence, null, 2));
} finally {
  await rm(workspace, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 });
}
