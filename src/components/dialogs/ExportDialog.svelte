<script lang="ts">
  import { tick } from 'svelte';
  import {
    defaultExportOptions
  } from '../../lib/documentExport';
  import type {
    ExportFormat,
    ExportMarginPreset,
    ExportOptions,
    ExportOrientation,
    ExportPaperSize
  } from '../../lib/tauriApi';

  export let open = false;
  export let busy = false;
  export let documentTitle = '';
  export let onExport: (format: ExportFormat, options: ExportOptions) => void = () => {};
  export let onClose: () => void = () => {};

  let dialog: HTMLElement;
  let format: ExportFormat = 'html';
  let options: ExportOptions = { ...defaultExportOptions };
  let wasOpen = false;

  $: if (open && !wasOpen) {
    wasOpen = true;
    void tick().then(() => dialog?.querySelector<HTMLElement>('select, input, button')?.focus());
  } else if (!open) {
    wasOpen = false;
  }

  function closeIfAllowed(): void {
    if (!busy) onClose();
  }

  function update<K extends keyof ExportOptions>(key: K, value: ExportOptions[K]): void {
    options = { ...options, [key]: value };
  }

  function handleSubmit(event: SubmitEvent): void {
    event.preventDefault();
    if (!busy) onExport(format, { ...options });
  }

  function handleKeydown(event: KeyboardEvent): void {
    event.stopPropagation();
    if (event.key === 'Escape' && !busy) {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== 'Tab' || !dialog) return;
    const focusable = Array.from(
      dialog.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex="0"]'
      )
    );
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable.at(-1)!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

{#if open}
  <div class="modal-backdrop" role="presentation" on:click={closeIfAllowed}>
    <div
      bind:this={dialog}
      class="export-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="export-dialog-title"
      aria-busy={busy}
      tabindex="-1"
      on:click|stopPropagation
      on:keydown={handleKeydown}
    >
      <header class="dialog-header">
        <div>
          <h2 id="export-dialog-title">导出为</h2>
          <p title={documentTitle}>{documentTitle || '当前文档'}</p>
        </div>
        <button type="button" class="ghost-button" disabled={busy} on:click={onClose}>关闭</button>
      </header>

      <form on:submit={handleSubmit}>
        <div class="export-dialog-body">
          <label>
            <span>文件格式</span>
            <select bind:value={format} disabled={busy}>
              <option value="html">HTML 网页</option>
              <option value="pdf">PDF 文档</option>
              <option value="docx">Word 文档（DOCX）</option>
            </select>
          </label>

          {#if format !== 'html'}
            <div class="export-grid">
              <label>
                <span>纸张</span>
                <select
                  value={options.paperSize}
                  disabled={busy}
                  on:change={(event) => update('paperSize', event.currentTarget.value as ExportPaperSize)}
                >
                  <option value="a4">A4</option>
                  <option value="letter">Letter</option>
                </select>
              </label>
              <label>
                <span>方向</span>
                <select
                  value={options.orientation}
                  disabled={busy}
                  on:change={(event) => update('orientation', event.currentTarget.value as ExportOrientation)}
                >
                  <option value="portrait">纵向</option>
                  <option value="landscape">横向</option>
                </select>
              </label>
              <label>
                <span>页边距</span>
                <select
                  value={options.margin}
                  disabled={busy}
                  on:change={(event) => update('margin', event.currentTarget.value as ExportMarginPreset)}
                >
                  <option value="narrow">窄</option>
                  <option value="normal">普通</option>
                  <option value="wide">宽</option>
                </select>
              </label>
            </div>
          {/if}

          <label class="switch-row">
            <span>包含文档标题</span>
            <input
              type="checkbox"
              checked={options.includeTitle}
              disabled={busy}
              on:change={(event) => update('includeTitle', event.currentTarget.checked)}
            />
          </label>
          <label class="switch-row">
            <span>嵌入本地图片（仅本次导出）</span>
            <input
              type="checkbox"
              checked={options.includeLocalImages}
              disabled={busy}
              on:change={(event) => update('includeLocalImages', event.currentTarget.checked)}
            />
          </label>
          <p class="export-note">
            不会下载网络图片。本地图片只会在你明确勾选后读取；缺失或不支持的内容会以警告方式降级。
          </p>
        </div>

        <footer class="dialog-footer">
          <button type="button" class="ghost-button" disabled={busy} on:click={onClose}>取消</button>
          <button type="submit" class="primary-button" disabled={busy}>
            {busy ? '正在导出…' : '选择位置并导出'}
          </button>
        </footer>
      </form>
    </div>
  </div>
{/if}

<style>
  .export-dialog {
    width: min(560px, calc(100vw - 32px));
    overflow: hidden;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-lg);
    background: var(--panel-bg);
    box-shadow: var(--shadow-strong);
  }

  .export-dialog-body {
    display: grid;
    gap: 16px;
    padding: 22px;
  }

  label {
    display: grid;
    gap: 7px;
  }

  label > span {
    color: var(--text-primary);
    font-size: 0.88rem;
    font-weight: 600;
  }

  select {
    width: 100%;
    min-height: 38px;
    padding: 0 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--input-bg);
    color: var(--text-primary);
  }

  .export-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }

  .switch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .export-note {
    margin: 0;
    color: var(--text-secondary);
    font-size: 0.82rem;
    line-height: 1.55;
  }

  @media (max-width: 560px) {
    .export-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
