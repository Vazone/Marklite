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
    <section
      class="about-dialog"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <header class="dialog-header">
        <div>
          <h2>MarkLite</h2>
          <p>Rust + Tauri + Svelte + CodeMirror</p>
        </div>
        <button type="button" class="ghost-button" on:click={onClose}>关闭</button>
      </header>
      <p>
        MarkLite 是一个面向 Windows 的轻量 Markdown 编辑器，支持文件读写、实时预览、多标签、最近文件、文档大纲和设置持久化。
      </p>
      <div class="about-meta">
        <span>作者：Vazone</span>
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
      <div class="about-diagnostics" aria-label="启动诊断">
        <p>启动诊断仅保存在本机，不包含文档正文或完整路径，也不会自动上传。</p>
        <div>
          <button type="button" class="ghost-button" on:click={onExportDiagnostics}>
            <Download size={15} />
            导出启动诊断
          </button>
          <button type="button" class="ghost-button" on:click={onClearDiagnostics}>
            <Trash2 size={15} />
            清除启动诊断
          </button>
        </div>
      </div>
    </section>
  </div>
{/if}
