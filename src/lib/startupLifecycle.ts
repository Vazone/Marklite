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

export type FrontendStartupStatus = 'started' | 'succeeded' | 'degraded' | 'failed';

export type FrontendStartupCode =
  | 'appRootMissing'
  | 'moduleLoadFailed'
  | 'svelteMountFailed'
  | 'interactiveFrameTimeout'
  | 'readySentinelMissing'
  | 'readyHandshakeFailed'
  | 'unhandledError'
  | 'unhandledRejection'
  | 'stageDegraded'
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
    throw new StartupLifecycleError('appRootMissing', 'The application root element was not found.');
  }
  return root;
}

export function startupFailureCode(error: unknown, fallback: FrontendStartupCode): FrontendStartupCode {
  return error instanceof StartupLifecycleError ? error.code : fallback;
}

export async function waitForInteractiveFrame(
  requestFrame: (callback: FrameRequestCallback) => number = window.requestAnimationFrame.bind(window),
  timeoutMs = 2_000,
  cancelFrame: (handle: number) => void = window.cancelAnimationFrame.bind(window)
): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    let settled = false;
    let frameHandle: number | undefined;
    const timeoutHandle = window.setTimeout(() => {
      if (settled) return;
      settled = true;
      if (frameHandle !== undefined) cancelFrame(frameHandle);
      reject(
        new StartupLifecycleError(
          'interactiveFrameTimeout',
          'The application did not reach an interactive frame within the startup deadline.'
        )
      );
    }, timeoutMs);

    try {
      frameHandle = requestFrame(() => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timeoutHandle);
        resolve();
      });
    } catch (error) {
      settled = true;
      window.clearTimeout(timeoutHandle);
      reject(error);
    }
  });
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
    throw new StartupLifecycleError('readySentinelMissing', 'The application interface did not finish mounting.');
  }

  sentinel.dataset.markliteInteractive = 'true';
  try {
    await acknowledge(startupElapsedMs(startedAt));
  } catch {
    delete sentinel.dataset.markliteInteractive;
    throw new StartupLifecycleError('readyHandshakeFailed', 'The startup readiness handshake failed.');
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
  title.textContent = 'MarkLite could not finish starting';
  title.style.margin = '0';
  title.style.fontSize = '22px';

  const message = documentRef.createElement('p');
  message.textContent = `The startup stage was recorded (${code}). You can retry once. Diagnostics remain on this device and are never uploaded automatically.`;
  message.style.margin = '0';
  message.style.maxWidth = '560px';

  const actions = documentRef.createElement('div');
  Object.assign(actions.style, { display: 'flex', justifyContent: 'center', gap: '10px', flexWrap: 'wrap' });

  const retry = documentRef.createElement('button');
  retry.type = 'button';
  retry.textContent = 'Retry startup';
  retry.addEventListener('click', onRetry);

  const clear = documentRef.createElement('button');
  clear.type = 'button';
  clear.textContent = 'Clear startup diagnostics';
  clear.addEventListener('click', () => {
    clear.disabled = true;
    void onClearDiagnostics()
      .then(() => {
        clear.textContent = 'Startup diagnostics cleared';
      })
      .catch(() => {
        clear.disabled = false;
        clear.textContent = 'Clear failed. Try again.';
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
