import { mount, unmount } from 'svelte';
import './styles/globals.css';
import './styles/themes.css';
import './styles/markdown-preview.css';
import { api } from './lib/tauriApi';
import {
  claimStartupRetry,
  clearStartupRetry,
  confirmInteractiveReady,
  installStartupErrorGuards,
  renderStartupRecovery,
  requireAppRoot,
  startupElapsedMs,
  startupFailureCode,
  type FrontendStartupCode,
  type FrontendStartupEvent
} from './lib/startupLifecycle';
import { appInitialization } from './lib/appInitialization';

const startedAt = performance.now();
let app: ReturnType<typeof mount> | undefined;
let fallbackCode: FrontendStartupCode = 'moduleLoadFailed';

function reportStartupEvent(event: FrontendStartupEvent): void {
  void api.recordFrontendStartupEvent(event).catch(() => undefined);
}

const guards = installStartupErrorGuards(reportStartupEvent, startedAt);

async function bootstrap(): Promise<void> {
  reportStartupEvent({
    stage: 'frontendEntry',
    status: 'started',
    code: null,
    elapsedMs: startupElapsedMs(startedAt)
  });

  try {
    const root = requireAppRoot(document);
    const { default: App } = await import('./app/App.svelte');
    reportStartupEvent({
      stage: 'frontendEntry',
      status: 'succeeded',
      code: null,
      elapsedMs: startupElapsedMs(startedAt)
    });

    fallbackCode = 'svelteMountFailed';
    reportStartupEvent({
      stage: 'svelteMount',
      status: 'started',
      code: null,
      elapsedMs: startupElapsedMs(startedAt)
    });
    root.replaceChildren();
    const initialization = appInitialization.wait();
    app = mount(App, { target: root });
    reportStartupEvent({
      stage: 'svelteMount',
      status: 'succeeded',
      code: null,
      elapsedMs: startupElapsedMs(startedAt)
    });
    fallbackCode = 'initializationFailed';
    await initialization;

    reportStartupEvent({
      stage: 'domReady',
      status: 'started',
      code: null,
      elapsedMs: startupElapsedMs(startedAt)
    });
    await confirmInteractiveReady(
      root,
      async (elapsedMs) => {
        const guardedFailure = guards.failureCode();
        if (guardedFailure) {
          throw new Error(guardedFailure);
        }
        reportStartupEvent({ stage: 'domReady', status: 'succeeded', code: null, elapsedMs });
        await api.markFrontendReady(elapsedMs);
      },
      startedAt
    );
    clearStartupRetry(sessionStorage);
    guards.stop();
  } catch (error) {
    const code = guards.failureCode() ?? startupFailureCode(error, fallbackCode);
    const stage =
      code === 'appRootMissing' || code === 'moduleLoadFailed' || code === 'unhandledError' || code === 'unhandledRejection'
        ? 'frontendEntry'
        : code === 'svelteMountFailed'
          ? 'svelteMount'
          : code === 'initializationFailed'
            ? 'initialization'
          : 'domReady';
    reportStartupEvent({
      stage,
      status: 'failed',
      code,
      elapsedMs: startupElapsedMs(startedAt)
    });

    if (app) {
      await unmount(app);
      app = undefined;
    }

    renderStartupRecovery(
      document,
      code,
      () => {
        if (claimStartupRetry(sessionStorage)) {
          window.location.reload();
          return;
        }
        const button = document.querySelector<HTMLButtonElement>('.startup-recovery button');
        if (button) {
          button.disabled = true;
          button.textContent = 'The one-retry limit has been reached.';
        }
      },
      async () => {
        await api.clearStartupDiagnostics();
      }
    );
  }
}

void bootstrap();

export { app as default };
