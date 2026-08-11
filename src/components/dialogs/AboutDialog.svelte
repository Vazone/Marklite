<script lang="ts">
  import { Download, ExternalLink, Trash2 } from 'lucide-svelte';
  import { openExternalLink } from '../../lib/tauriApi';

  export let open = false;
  export let onClose: () => void = () => {};
  export let onExportDiagnostics: () => void = () => {};
  export let onClearDiagnostics: () => void = () => {};

  const repositoryUrl = 'https://github.com/Vazone/Marklite';
</script>

{#if open}
  <div class="modal-backdrop" role="presentation" on:click={onClose}>
    <div
      class="about-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="about-dialog-title"
      tabindex="-1"
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <header class="dialog-header">
        <div>
          <h2 id="about-dialog-title">关于 MarkLite / About MarkLite</h2>
          <p>Rust + Tauri + Svelte + CodeMirror</p>
        </div>
        <button type="button" class="ghost-button" on:click={onClose}>关闭 / Close</button>
      </header>
      <div class="about-description">
        <p lang="zh-CN">
          MarkLite 是一个面向 Windows、macOS 和 Linux 的轻量 Markdown 编辑器，支持文件读写、实时预览、多标签、最近文件、文档大纲和设置持久化。
        </p>
        <p lang="en">
          MarkLite is a lightweight Markdown editor for Windows, macOS, and Linux, with file editing, live preview, tabs, recent files, document outlines, and persistent settings.
        </p>
      </div>
      <div class="about-meta">
        <span>作者 / Author: Vazone</span>
        <button type="button" on:click={() => void openExternalLink(repositoryUrl)}>
          <ExternalLink size={15} />
          <span>{repositoryUrl}</span>
        </button>
      </div>
      <div class="about-stack">
        <span>Tauri 2</span>
        <span>pulldown-cmark</span>
        <span>CodeMirror 6</span>
        <span>Svelte 5</span>
      </div>
      <div class="about-diagnostics" aria-label="启动诊断 / Startup diagnostics">
        <p>
          <span lang="zh-CN">启动诊断仅保存在本机，不包含文档正文或完整路径，也不会自动上传。</span>
          <span lang="en">Startup diagnostics remain on this device, exclude document content and full paths, and are never uploaded automatically.</span>
        </p>
        <div>
          <button type="button" class="ghost-button" on:click={onExportDiagnostics}>
            <Download size={15} />
            导出启动诊断 / Export diagnostics
          </button>
          <button type="button" class="ghost-button" on:click={onClearDiagnostics}>
            <Trash2 size={15} />
            清除启动诊断 / Clear diagnostics
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
