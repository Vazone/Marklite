export const STARTUP_READY_SELECTOR = '[data-marklite-ready="true"]';
const STARTUP_RETRY_KEY = 'marklite.startup-retry-count';

export type FrontendStartupStage =
  | 'frontendEntry'
  | 'svelteMount'
  | 'domReady'
  | 'initialization'
  | 'settings'
  | 'recentFiles'
  | 'sessionRestore'
  | 'externalListeners'
  | 'startupFile'
  | 'firstRender'
  | 'dragDrop'
  | 'editorMount';

export type FrontendStartupStatus = 'started' | 'succeeded' | 'failed';

export type FrontendStartupCode =
  | 'appRootMissing'
  | 'moduleLoadFailed'
  | 'svelteMountFailed'
  | 'readySentinelMissing'
  | 'readyHandshakeFailed'
  | 'unhandledError'
  | 'unhandledRejection'
  | 'initializationFailed'
  | 'editorMountFailed';

export type FrontendStartupEvent = {
  stage: FrontendStartupStage;
  status: FrontendStartupStatus;
  code: FrontendStartupCode | null;
  elapsedMs: number;
};

export type StartupErrorGuards = {
  failureCode: () => FrontendStartupCode | null;
  stop: () => void;
};

export class StartupLifecycleError extends Error {
  constructor(
    public readonly code: FrontendStartupCode,
    message: string
  ) {
    super(message);
    this.name = 'StartupLifecycleError';
  }
}

export function startupElapsedMs(startedAt: number, now = performance.now()): number {
  return Math.max(0, Math.min(600_000, Math.round(now - startedAt)));
}

export function requireAppRoot(documentRef: Document): HTMLElement {
  const root = documentRef.getElementById('app');
  if (!(root instanceof HTMLElement)) {
    throw new StartupLifecycleError('appRootMissing', '无法找到应用根节点');
  }
  return root;
}

export function startupFailureCode(error: unknown, fallback: FrontendStartupCode): FrontendStartupCode {
  return error instanceof StartupLifecycleError ? error.code : fallback;
}

export async function waitForInteractiveFrame(
  requestFrame: (callback: FrameRequestCallback) => number = window.requestAnimationFrame.bind(window)
): Promise<void> {
  await new Promise<void>((resolve) => requestFrame(() => resolve()));
}

export async function confirmInteractiveReady(
  root: HTMLElement,
  acknowledge: (elapsedMs: number) => Promise<unknown>,
  startedAt: number,
  requestFrame?: (callback: FrameRequestCallback) => number
): Promise<void> {
  await waitForInteractiveFrame(requestFrame);
  const sentinel = root.querySelector<HTMLElement>(STARTUP_READY_SELECTOR);
  if (!sentinel) {
    throw new StartupLifecycleError('readySentinelMissing', '应用界面未完成挂载');
  }

  sentinel.dataset.markliteInteractive = 'true';
  try {
    await acknowledge(startupElapsedMs(startedAt));
  } catch {
    delete sentinel.dataset.markliteInteractive;
    throw new StartupLifecycleError('readyHandshakeFailed', '启动就绪握手失败');
  }
}

export function installStartupErrorGuards(
  report: (event: FrontendStartupEvent) => void,
  startedAt: number,
  windowRef: Window = window
): StartupErrorGuards {
  let active = true;
  let firstFailure: FrontendStartupCode | null = null;
  const onError = () => {
    if (!active || firstFailure) return;
    firstFailure = 'unhandledError';
    report({
      stage: 'frontendEntry',
      status: 'failed',
      code: 'unhandledError',
      elapsedMs: startupElapsedMs(startedAt)
    });
  };
  const onUnhandledRejection = () => {
    if (!active || firstFailure) return;
    firstFailure = 'unhandledRejection';
    report({
      stage: 'frontendEntry',
      status: 'failed',
      code: 'unhandledRejection',
      elapsedMs: startupElapsedMs(startedAt)
    });
  };

  windowRef.addEventListener('error', onError);
  windowRef.addEventListener('unhandledrejection', onUnhandledRejection);

  return {
    failureCode() {
      return firstFailure;
    },
    stop() {
      active = false;
      windowRef.removeEventListener('error', onError);
      windowRef.removeEventListener('unhandledrejection', onUnhandledRejection);
    }
  };
}

export function claimStartupRetry(storage: Storage): boolean {
  const attempts = Number.parseInt(storage.getItem(STARTUP_RETRY_KEY) ?? '0', 10);
  if (Number.isFinite(attempts) && attempts >= 1) return false;
  storage.setItem(STARTUP_RETRY_KEY, '1');
  return true;
}

export function clearStartupRetry(storage: Storage): void {
  storage.removeItem(STARTUP_RETRY_KEY);
}

export function renderStartupRecovery(
  documentRef: Document,
  code: FrontendStartupCode,
  onRetry: () => void,
  onClearDiagnostics: () => Promise<void>
): HTMLElement {
  const root = documentRef.getElementById('app') ?? documentRef.body.appendChild(documentRef.createElement('div'));
  root.id = 'app';
  root.replaceChildren();

  const panel = documentRef.createElement('main');
  panel.className = 'startup-recovery';
  panel.setAttribute('role', 'alert');
  panel.dataset.startupErrorCode = code;
  Object.assign(panel.style, {
    minHeight: '100%',
    display: 'grid',
    placeContent: 'center',
    gap: '12px',
    padding: '32px',
    color: '#e7ece8',
    background: '#171b18',
    fontFamily: "'Segoe UI', system-ui, sans-serif",
    textAlign: 'center'
  });

  const title = documentRef.createElement('h1');
  title.textContent = 'MarkLite 未能完成启动';
  title.style.margin = '0';
  title.style.fontSize = '22px';

  const message = documentRef.createElement('p');
  message.textContent = `启动阶段已记录（${code}）。可以重试一次；诊断只保存在本机且不会自动上传。`;
  message.style.margin = '0';
  message.style.maxWidth = '560px';

  const actions = documentRef.createElement('div');
  Object.assign(actions.style, { display: 'flex', justifyContent: 'center', gap: '10px', flexWrap: 'wrap' });

  const retry = documentRef.createElement('button');
  retry.type = 'button';
  retry.textContent = '重试启动';
  retry.addEventListener('click', onRetry);

  const clear = documentRef.createElement('button');
  clear.type = 'button';
  clear.textContent = '清除启动诊断';
  clear.addEventListener('click', () => {
    clear.disabled = true;
    void onClearDiagnostics().finally(() => {
      clear.textContent = '启动诊断已清除';
    });
  });

  for (const button of [retry, clear]) {
    Object.assign(button.style, {
      padding: '9px 14px',
      border: '1px solid #5f6c63',
      borderRadius: '6px',
      background: '#263029',
      color: 'inherit',
      cursor: 'pointer'
    });
  }

  actions.append(retry, clear);
  panel.append(title, message, actions);
  root.append(panel);
  retry.focus();
  return panel;
}
