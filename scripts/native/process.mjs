import { spawn, execFileSync } from 'node:child_process';

export function requireIdle() {
  if (process.platform !== 'win32') throw new Error('This native suite currently requires Windows WebView2');
  const ids = execFileSync('powershell.exe', ['-NoProfile', '-Command',
    'Get-Process -Name marklite,marklite-cli -ErrorAction SilentlyContinue | ForEach-Object { $_.Id }; exit 0'],
  { encoding: 'utf8', windowsHide: true }).trim();
  if (ids) throw new Error('Save documents and close MarkLite before running native regressions');
}

export async function run(program, args, { cwd, env = process.env, timeout = 120000 } = {}) {
  const child = spawn(program, args, { cwd, env, windowsHide: true, shell: false, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.setEncoding('utf8');
  child.stderr.setEncoding('utf8');
  return new Promise((resolve, reject) => {
    let stdout = '', stderr = '', failure;
    const terminate = (message) => {
      failure ??= new Error(message);
      if (child.pid && child.exitCode === null) {
        // Kill only the process tree created by this invocation, including a hung export child.
        if (process.platform === 'win32') {
          try { execFileSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore', timeout: 10000 }); }
          catch { child.kill(); }
        } else child.kill('SIGKILL');
      }
    };
    const timer = setTimeout(() => terminate(`Native command timed out after ${timeout} ms`), timeout);
    child.stdout.on('data', bytes => { stdout += bytes.toString('utf8'); if (stdout.length > 4 * 1024 * 1024) terminate('Native stdout exceeded limit'); });
    child.stderr.on('data', bytes => { stderr += bytes.toString('utf8'); if (stderr.length > 4 * 1024 * 1024) terminate('Native stderr exceeded limit'); });
    child.once('error', (error) => { clearTimeout(timer); reject(error); });
    child.once('close', (code) => {
      clearTimeout(timer);
      if (failure) reject(failure);
      else resolve({ code, stdout, stderr });
    });
  });
}

export async function successful(program, args, options) {
  const result = await run(program, args, options);
  if (result.code !== 0) throw new Error(`Native command failed (${result.code}): ${result.stderr.slice(-4000)}`);
  return result;
}
