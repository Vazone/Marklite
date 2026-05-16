<script lang="ts">
  import type { AppSettings } from '../../lib/tauriApi';
  import { openExternalLink } from '../../lib/tauriApi';
  import { uiActions } from '../../app/stores/uiStore';
  import type { EditorScrollPosition } from '../../app/stores/documentStore';

  export let html = '';
  export let settings: AppSettings;

  let previewHost: HTMLElement;

  export function syncToEditorScroll(position: EditorScrollPosition | undefined) {
    if (!settings.syncScroll || !previewHost || !position) return;

    const maxScroll = Math.max(0, previewHost.scrollHeight - previewHost.clientHeight);
    previewHost.scrollTop = maxScroll * Math.min(1, Math.max(0, position.ratio));
  }

  async function handleClick(event: MouseEvent) {
    const target = event.target as HTMLElement | null;
    const anchor = target?.closest('a');
    if (!anchor) return;

    const href = anchor.getAttribute('href');
    if (!href || href.startsWith('#')) return;

    event.preventDefault();

    try {
      if (settings.confirmExternalLinks && !window.confirm(`打开外部链接？\n${href}`)) {
        return;
      }

      await openExternalLink(href);
    } catch (error) {
      uiActions.toast(String(error), 'error');
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<section
  bind:this={previewHost}
  class="preview markdown-preview"
  style:font-family={settings.previewFontFamily}
  style:font-size={`${settings.previewFontSize}px`}
  style:line-height={settings.lineHeight}
  on:click={handleClick}
>
  {#if html}
    {@html html}
  {:else}
    <div class="preview-empty">
      <h2>预览将在这里显示</h2>
      <p>开始输入 Markdown 后会自动刷新。</p>
    </div>
  {/if}
</section>
