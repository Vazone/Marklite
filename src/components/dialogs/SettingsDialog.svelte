<script lang="ts">
  import { onMount } from 'svelte';
  import type { AppSettings, ThemeMode } from '../../lib/tauriApi';
  import { shortcuts } from '../../lib/shortcuts';

  export let open = false;
  export let settings: AppSettings;
  export let onSave: (settings: AppSettings) => void = () => {};
  export let onReset: () => void = () => {};
  export let onClose: () => void = () => {};

  let draft: AppSettings = { ...settings };
  let tab: 'appearance' | 'editor' | 'preview' | 'files' | 'shortcuts' = 'appearance';

  $: if (open) {
    draft = { ...settings };
  }

  onMount(() => {
    const handler = (event: KeyboardEvent) => {
      if (open && event.key === 'Escape') {
        onClose();
      }
    };

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  });

  function update<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    draft = { ...draft, [key]: value };
  }

  function saveSettings() {
    onSave(draft);
  }
</script>

{#if open}
  <div class="modal-backdrop" role="presentation" on:click={onClose}>
    <section
      class="settings-dialog"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <header class="dialog-header">
        <div>
          <h2>设置</h2>
          <p>外观、编辑器、预览和文件偏好</p>
        </div>
        <button type="button" class="ghost-button" on:click={onClose}>关闭</button>
      </header>

      <div class="settings-body">
        <nav class="settings-nav">
          <button type="button" class:active={tab === 'appearance'} on:click={() => (tab = 'appearance')}>外观</button>
          <button type="button" class:active={tab === 'editor'} on:click={() => (tab = 'editor')}>编辑器</button>
          <button type="button" class:active={tab === 'preview'} on:click={() => (tab = 'preview')}>预览</button>
          <button type="button" class:active={tab === 'files'} on:click={() => (tab = 'files')}>文件</button>
          <button type="button" class:active={tab === 'shortcuts'} on:click={() => (tab = 'shortcuts')}>快捷键</button>
        </nav>

        <div class="settings-panel">
          {#if tab === 'appearance'}
            <label>
              <span>主题</span>
              <select bind:value={draft.theme} on:change={(event) => update('theme', event.currentTarget.value as ThemeMode)}>
                <option value="system">跟随系统</option>
                <option value="light">浅色</option>
                <option value="dark">深色</option>
              </select>
            </label>
            <label>
              <span>强调色</span>
              <input type="color" bind:value={draft.accentColor} on:input={(event) => update('accentColor', event.currentTarget.value)} />
            </label>
            <label>
              <span>编辑器字体</span>
              <input bind:value={draft.editorFontFamily} on:input={(event) => update('editorFontFamily', event.currentTarget.value)} />
            </label>
            <label>
              <span>预览字体</span>
              <input bind:value={draft.previewFontFamily} on:input={(event) => update('previewFontFamily', event.currentTarget.value)} />
            </label>
            <label>
              <span>界面缩放</span>
              <input type="range" min="0.9" max="1.2" step="0.05" bind:value={draft.interfaceScale} on:input={(event) => update('interfaceScale', Number(event.currentTarget.value))} />
              <em>{draft.interfaceScale.toFixed(2)}x</em>
            </label>
            <label>
              <span>圆角</span>
              <input type="number" min="0" max="16" bind:value={draft.cornerRadius} on:input={(event) => update('cornerRadius', Number(event.currentTarget.value))} />
            </label>
          {:else if tab === 'editor'}
            <label class="switch-row">
              <span>显示行号</span>
              <input type="checkbox" bind:checked={draft.showLineNumbers} on:change={(event) => update('showLineNumbers', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>自动换行</span>
              <input type="checkbox" bind:checked={draft.wordWrap} on:change={(event) => update('wordWrap', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>Markdown 工具栏</span>
              <input type="checkbox" bind:checked={draft.markdownToolbarEnabled} on:change={(event) => update('markdownToolbarEnabled', event.currentTarget.checked)} />
            </label>
            <label>
              <span>编辑器字号</span>
              <input type="number" min="12" max="24" bind:value={draft.editorFontSize} on:input={(event) => update('editorFontSize', Number(event.currentTarget.value))} />
            </label>
            <label>
              <span>行高</span>
              <input type="range" min="1.2" max="2" step="0.05" bind:value={draft.lineHeight} on:input={(event) => update('lineHeight', Number(event.currentTarget.value))} />
              <em>{draft.lineHeight.toFixed(2)}</em>
            </label>
            <label>
              <span>Tab 宽度</span>
              <input type="number" min="2" max="8" bind:value={draft.tabSize} on:input={(event) => update('tabSize', Number(event.currentTarget.value))} />
            </label>
            <label class="switch-row">
              <span>使用空格替代 Tab</span>
              <input type="checkbox" bind:checked={draft.insertSpaces} on:change={(event) => update('insertSpaces', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>自动保存</span>
              <input type="checkbox" bind:checked={draft.autosaveEnabled} on:change={(event) => update('autosaveEnabled', event.currentTarget.checked)} />
            </label>
            <label>
              <span>自动保存间隔</span>
              <input type="number" min="1000" step="500" bind:value={draft.autosaveIntervalMs} on:input={(event) => update('autosaveIntervalMs', Number(event.currentTarget.value))} />
            </label>
          {:else if tab === 'preview'}
            <label class="switch-row">
              <span>实时预览</span>
              <input type="checkbox" bind:checked={draft.livePreviewEnabled} on:change={(event) => update('livePreviewEnabled', event.currentTarget.checked)} />
            </label>
            <label>
              <span>预览延迟</span>
              <input type="number" min="100" step="50" bind:value={draft.previewDebounceMs} on:input={(event) => update('previewDebounceMs', Number(event.currentTarget.value))} />
            </label>
            <label class="switch-row">
              <span>滚动同步</span>
              <input type="checkbox" bind:checked={draft.syncScroll} on:change={(event) => update('syncScroll', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>允许本地图片</span>
              <input type="checkbox" bind:checked={draft.allowLocalImages} on:change={(event) => update('allowLocalImages', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>外链打开前确认</span>
              <input type="checkbox" bind:checked={draft.confirmExternalLinks} on:change={(event) => update('confirmExternalLinks', event.currentTarget.checked)} />
            </label>
            <label>
              <span>预览字号</span>
              <input type="number" min="12" max="28" bind:value={draft.previewFontSize} on:input={(event) => update('previewFontSize', Number(event.currentTarget.value))} />
            </label>
          {:else if tab === 'files'}
            <label class="switch-row">
              <span>显示侧边栏</span>
              <input type="checkbox" bind:checked={draft.showSidebar} on:change={(event) => update('showSidebar', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>显示状态栏</span>
              <input type="checkbox" bind:checked={draft.showStatusBar} on:change={(event) => update('showStatusBar', event.currentTarget.checked)} />
            </label>
            <label class="switch-row">
              <span>启动时恢复上次会话</span>
              <input type="checkbox" bind:checked={draft.restoreLastSession} on:change={(event) => update('restoreLastSession', event.currentTarget.checked)} />
            </label>
            <label>
              <span>最近文件数量</span>
              <input type="number" min="3" max="50" bind:value={draft.recentFilesLimit} on:input={(event) => update('recentFilesLimit', Number(event.currentTarget.value))} />
            </label>
          {:else}
            <div class="shortcut-list">
              {#each shortcuts as shortcut}
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
        <button type="button" class="ghost-button" on:click={onReset}>恢复默认</button>
        <button type="button" class="primary-button" on:click={saveSettings}>保存设置</button>
      </footer>
    </section>
  </div>
{/if}
