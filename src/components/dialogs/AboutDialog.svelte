<script lang="ts">
  export let open = false;
  export let onClose: () => void = () => {};
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
      <div class="about-stack">
        <span>Tauri 2</span>
        <span>pulldown-cmark</span>
        <span>CodeMirror 6</span>
        <span>Svelte 5</span>
      </div>
    </section>
  </div>
{/if}
