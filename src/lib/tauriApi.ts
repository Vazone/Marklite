import { createTauriClient } from './platform/tauriClient';
import { runtimeCommandTransport } from './platform/runtime';

export * from './platform/contracts';
export * from './platform/contractValidation';
export * from './platform/dialogs';
export * from './platform/externalLinks';
export { isTauriRuntime } from './platform/runtime';
export { createTauriClient, type TauriClient } from './platform/tauriClient';
export type { CommandTransport } from './platform/runtime';

export const api = createTauriClient(runtimeCommandTransport);
