<script lang="ts">
  import type { AppSettings, ThemeMode } from '../../lib/tauriApi';
  import { listedShortcuts } from '../../lib/shortcuts';
  import { supportedLanguages, translator, type AppLanguage } from '../../lib/i18n';
  import ModalShell from './ModalShell.svelte';

  export let open = false;
  export let settings: AppSettings;
  export let busy = false;
  export let onSave: (settings: AppSettings) => void | Promise<AppSettings | void>;
  export let onReset: () => void | Promise<AppSettings | void>;
  export let onClose: () => void;

  let draft: AppSettings = { ...settings };
  let tab: 'appearance' | 'editor' | 'preview' | 'files' | 'shortcuts' = 'appearance';
  let previousOpen = false;
  let sessionRevision = 0;
  let localBusy = false;

  $: synchronizeOpen(open, settings);
  $: shortcutItems = listedShortcuts(navigator.platform, draft.language);
  $: operationBusy = busy || localBusy;

  function synchronizeOpen(nextOpen: boolean, source: AppSettings) {
    if (nextOpen === previousOpen) return;
    previousOpen = nextOpen;
    sessionRevision += 1;
    localBusy = false;
    if (nextOpen) draft = { ...source };
  }

  function update<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    draft = { ...draft, [key]: value };
  }

  async function runOperation(operation: () => void | Promise<AppSettings | void>) {
    if (operationBusy) return;
    const requestSession = sessionRevision;
    localBusy = true;
    try {
      const result = await operation();
      if (result && open && sessionRevision === requestSession) draft = { ...result };
    } finally {
      if (open && sessionRevision === requestSession) localBusy = false;
    }
  }

  function saveSettings() {
    const snapshot = { ...draft };
    void runOperation(() => onSave(snapshot));
  }

  function resetSettings() {
    void runOperation(onReset);
  }
</script>

{#if open}
  <ModalShell dialogClass="settings-dialog" ariaLabel={$translator('settings.title')} {onClose}>
      <header class="dialog-header">
        <div>
          <h2>{$translator('settings.title')}</h2>
          <p>{$translator('settings.subtitle')}</p>
        </div>
        <button type="button" class="ghost-button" on:click={onClose}>{$translator('common.close')}</button>
      </header>

      <div class="settings-body">
        <nav class="settings-nav">
          <button type="button" class:active={tab === 'appearance'} on:click={() => (tab = 'appearance')}>{$translator('settings.tab.appearance')}</button>
          <button type="button" class:active={tab === 'editor'} on:click={() => (tab = 'editor')}>{$translator('settings.tab.editor')}</button>
          <button type="button" class:active={tab === 'preview'} on:click={() => (tab = 'preview')}>{$translator('settings.tab.preview')}</button>
          <button type="button" class:active={tab === 'files'} on:click={() => (tab = 'files')}>{$translator('settings.tab.files')}</button>
          <button type="button" class:active={tab === 'shortcuts'} on:click={() => (tab = 'shortcuts')}>{$translator('settings.tab.shortcuts')}</button>
        </nav>

        <div class="settings-panel">
          {#if tab === 'appearance'}
            <label>
              <span>{$translator('settings.language')}</span>
              <select bind:value={draft.language} on:change={(event) => update('language', event.currentTarget.value as AppLanguage)}>
                {#each supportedLanguages as language}
                  <option value={language.id}>{language.label}</option>
                {/each}
              </select>
            </label>
            <label>
              <span>{$translator('settings.theme')}</span>
              <select bind:value={draft.theme} on:change={(event) => update('theme', event.currentTarget.value as ThemeMode)}>
                <option value="system">{$translator('settings.theme.system')}</option>
                <option value="light">{$translator('settings.theme.light')}</option>
                <option value="dark">{$translator('settings.theme.dark')}</option>
              </select>
            </label>
            <label>
              <span>{$translator('settings.accentColor')}</span>
              <input type="color" bind:value={draft.accentColor} on:input={(event) => update('accentColor', event.currentTarget.value)} />
            </label>
            <label>
              <span>{$translator('settings.editorFont')}</span>
              <input maxlength="200" bind:value={draft.editorFontFamily} on:input={(event) => update('editorFontFamily', event.currentTarget.value)} />
            </label>
            <label>
              <span>{$translator('settings.previewFont')}</span>
              <input maxlength="200" bind:value={draft.previewFontFamily} on:input={(event) => update('previewFontFamily', event.currentTarget.value)} />
            </label>
            <label>
              <span>{$translator('settings.cornerRadius')}</span>
              <input type="number" min="0" max="16" bind:value={draft.cornerRadius} on:input={(event) => update('cornerRadius', Number(event.currentTarget.value))} />
            </label>
          {:else if tab === 'editor'}
            <label class="switch-row">
              <span>{$translator('settings.showLineNumbers')}</span>
              <input type="checkbox" bind:checked={draft.showLineNumbers} on:change={(event) => update('showLineNumbers', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>{$translator('settings.wordWrap')}</span>
              <input type="checkbox" bind:checked={draft.wordWrap} on:change={(event) => update('wordWrap', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>{$translator('settings.markdownToolbar')}</span>
              <input type="checkbox" bind:checked={draft.markdownToolbarEnabled} on:change={(event) => update('markdownToolbarEnabled', event.currentTarget.checked)} />
            </label>
            <label>
              <span>{$translator('settings.editorFontSize')}</span>
              <input type="number" min="12" max="24" bind:value={draft.editorFontSize} on:input={(event) => update('editorFontSize', Number(event.currentTarget.value))} />
            </label>
            <label>
              <span>{$translator('settings.lineHeight')}</span>
              <input type="range" min="1.2" max="2" step="0.05" bind:value={draft.lineHeight} on:input={(event) => update('lineHeight', Number(event.currentTarget.value))} />
              <em>{draft.lineHeight.toFixed(2)}</em>
            </label>
            <label>
              <span>{$translator('settings.tabSize')}</span>
              <input type="number" min="2" max="8" bind:value={draft.tabSize} on:input={(event) => update('tabSize', Number(event.currentTarget.value))} />
            </label>
            <label class="switch-row">
              <span>{$translator('settings.insertSpaces')}</span>
              <input type="checkbox" bind:checked={draft.insertSpaces} on:change={(event) => update('insertSpaces', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>{$translator('settings.autosave')}</span>
              <input type="checkbox" bind:checked={draft.autosaveEnabled} on:change={(event) => update('autosaveEnabled', event.currentTarget.checked)} />
            </label>
            <label>
              <span>{$translator('settings.autosaveInterval')}</span>
              <input type="number" min="1000" max="3600000" step="500" bind:value={draft.autosaveIntervalMs} on:input={(event) => update('autosaveIntervalMs', Number(event.currentTarget.value))} />
            </label>
          {:else if tab === 'preview'}
            <label class="switch-row">
              <span>{$translator('settings.livePreview')}</span>
              <input type="checkbox" bind:checked={draft.livePreviewEnabled} on:change={(event) => update('livePreviewEnabled', event.currentTarget.checked)} />
            </label>
            <label>
              <span>{$translator('settings.previewDelay')}</span>
              <input type="number" min="100" max="10000" step="50" bind:value={draft.previewDebounceMs} on:input={(event) => update('previewDebounceMs', Number(event.currentTarget.value))} />
            </label>
            <label class="switch-row">
              <span>{$translator('settings.syncScroll')}</span>
              <input type="checkbox" bind:checked={draft.syncScroll} on:change={(event) => update('syncScroll', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>{$translator('settings.allowLocalImages')}</span>
              <input type="checkbox" bind:checked={draft.allowLocalImages} on:change={(event) => update('allowLocalImages', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>{$translator('settings.confirmExternalLinks')}</span>
              <input type="checkbox" bind:checked={draft.confirmExternalLinks} on:change={(event) => update('confirmExternalLinks', event.currentTarget.checked)} />
            </label>
            <label>
              <span>{$translator('settings.previewFontSize')}</span>
              <input type="number" min="12" max="28" bind:value={draft.previewFontSize} on:input={(event) => update('previewFontSize', Number(event.currentTarget.value))} />
            </label>
          {:else if tab === 'files'}
            <label class="switch-row">
              <span>{$translator('settings.showSidebar')}</span>
              <input type="checkbox" bind:checked={draft.showSidebar} on:change={(event) => update('showSidebar', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>{$translator('settings.showStatusBar')}</span>
              <input type="checkbox" bind:checked={draft.showStatusBar} on:change={(event) => update('showStatusBar', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>{$translator('settings.restoreSession')}</span>
              <input type="checkbox" bind:checked={draft.restoreLastSession} on:change={(event) => update('restoreLastSession', event.currentTarget.checked)} />
            </label>
            <label>
              <span>{$translator('settings.recentFileCount')}</span>
              <input type="number" min="3" max="50" bind:value={draft.recentFilesLimit} on:input={(event) => update('recentFilesLimit', Number(event.currentTarget.value))} />
            </label>
          {:else}
            <div class="shortcut-list">
              {#each shortcutItems as shortcut}
                <div>
                  <kbd>{shortcut.keys}</kbd>
                  <span>{shortcut.action}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <footer class="dialog-footer">
        <button type="button" class="ghost-button" disabled={operationBusy} on:click={resetSettings}>{$translator('settings.reset')}</button>
        <button type="button" class="primary-button" disabled={operationBusy} on:click={saveSettings}>{$translator('settings.save')}</button>
      </footer>
  </ModalShell>
{/if}
