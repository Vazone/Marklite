import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { CdpClient } from './cdp.mjs';
import { run } from './process.mjs';
import { verifyScrollFeedback } from '../performance/verify-scroll-feedback.mjs';

const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

export async function waitReady(client, timeout = 60000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      if (await client.evaluate(`document.querySelector('.app-shell')?.dataset.markliteReady === 'true' &&
        Boolean(document.querySelector('.cm-content')) && document.querySelector('.editor-stage')?.getAttribute('aria-busy') === 'false'`)) return;
    } catch (error) {
      // WebView replaces its initial document when the application page navigates in.
      // Retry only that observed startup race; protocol and application errors remain failures.
      if (!String(error).includes('Execution context was destroyed')) throw error;
    }
    await pause(30);
  }
  throw new Error('Editor startup timeout');
}

async function allocatePort() {
  const server = createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}

async function connect(port, child) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null || child.signalCode !== null) throw new Error('Test application exited before WebView startup');
    let target;
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(1000) });
      if (response.ok) target = (await response.json()).find(t => t.type === 'page' && t.webSocketDebuggerUrl);
    } catch { /* WebView binds the endpoint asynchronously. */ }
    if (target) {
      const client = new CdpClient(target.webSocketDebuggerUrl);
      await client.connect();
      return client;
    }
    await pause(50);
  }
  throw new Error('WebView debugger did not start within 30 seconds');
}

export async function verifyScroll(context, input) {
  const { executable, output, env, root } = context;
  const port = await allocatePort();
  const profile = join(output, 'scroll-profile');
  await mkdir(profile);
  const child = spawn(executable, [input], { cwd: root, windowsHide: true, stdio: 'ignore', env: {
    ...env, MARKLITE_BENCHMARK_DATA_DIR: profile, WEBVIEW2_USER_DATA_FOLDER: join(profile, 'webview'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`
  } });
  let client, spawnError;
  child.once('error', error => { spawnError = error; });
  try {
    client = await connect(port, child);
    if (spawnError) throw spawnError;
    await waitReady(client);
    const results = {};
    for (const [mode, index] of [['split', 1], ['preview', 2]]) {
      await client.evaluate(`document.querySelectorAll('.layout-switch button')[${index}].click()`);
      await client.evaluate('(async () => { await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame); })()');
      await client.evaluate(`document.querySelector('.preview-content').scrollTop = 0`);
      results[mode] = await verifyScrollFeedback(client, mode === 'split');
    }
    return { userAgent: await client.evaluate('navigator.userAgent'), ...results };
  } finally {
    client?.close();
    if (child.pid && child.exitCode === null && child.signalCode === null) {
      const stopped = await run('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { timeout: 10000 });
      if (stopped.code !== 0 && child.exitCode === null && child.signalCode === null) throw new Error('Test process tree did not stop');
    }
  }
}
