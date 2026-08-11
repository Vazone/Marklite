<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { AppSettings } from '../../lib/tauriApi';
  import { api, openExternalLink, toAppError } from '../../lib/tauriApi';
  import { executeMarkdownTarget } from '../../lib/markdownNavigation';
  import { uiActions } from '../../app/stores/uiStore';
  import type { EditorScrollPosition } from '../../app/stores/documentStore';

  export let html = '';
  export let settings: AppSettings;
  export let documentPath: string | null = null;
  export let onOpenDocument: (path: string, fragment: string | null) => void = () => {};

  let previewHost: HTMLElement;
  let resourceRevision = 0;
  let resourceKey = '';
  let scheduledResourceKey: string | null = null;
  let preparedHtml = '';

  const IMAGE_LOAD_CONCURRENCY = 4;

  $: resourceKey = `${documentPath ?? ''}\u0000${settings.allowLocalImages}\u0000${html}`;

  $: if (previewHost && resourceKey !== scheduledResourceKey) {
    scheduledResourceKey = resourceKey;
    schedulePreviewPreparation(html, documentPath, settings.allowLocalImages);
  }

  onDestroy(() => {
    resourceRevision += 1;
  });

  export function syncToEditorScroll(position: EditorScrollPosition | undefined) {
    if (!settings.syncScroll || !previewHost || !position) return;

    const maxScroll = Math.max(0, previewHost.scrollHeight - previewHost.clientHeight);
    previewHost.scrollTop = maxScroll * Math.min(1, Math.max(0, position.ratio));
  }

  export function scrollToFragment(fragment: string) {
    if (!previewHost) return;
    if (!fragment) {
      previewHost.scrollTop = 0;
      return;
    }
    const target = Array.from(previewHost.querySelectorAll<HTMLElement>('[id]')).find(
      (element) => element.id === fragment
    );
    if (target) {
      previewHost.scrollTop = Math.max(0, target.offsetTop - previewHost.offsetTop - 12);
    } else {
      uiActions.toast(`未找到预览锚点：#${fragment}`, 'error');
    }
  }

  async function handleClick(event: MouseEvent) {
    const target = event.target as HTMLElement | null;
    const anchor = target?.closest('a');
    if (!anchor) return;

    const href = anchor.getAttribute('href');
    if (!href) return;
    event.preventDefault();

    try {
      const resolved = await api.resolveMarkdownTarget(documentPath, href);
      await executeMarkdownTarget(resolved, settings.confirmExternalLinks, {
        scrollToFragment,
        openDocument: onOpenDocument,
        confirmExternal: (url) => window.confirm(`打开外部链接？\n${url}`),
        openExternal: openExternalLink
      });
    } catch (error) {
      uiActions.toast(toAppError(error).message, 'error');
    }
  }

  function schedulePreviewPreparation(
    currentHtml: string,
    currentDocumentPath: string | null,
    allowLocalImages: boolean
  ) {
    resourceRevision += 1;
    void preparePreview(currentHtml, currentDocumentPath, allowLocalImages, resourceRevision);
  }

  async function preparePreview(
    currentHtml: string,
    currentDocumentPath: string | null,
    allowLocalImages: boolean,
    revision: number
  ) {
    const template = createPreviewTemplate(currentHtml);
    assignHeadingIds(template.content);
    const images = Array.from(template.content.querySelectorAll<HTMLImageElement>('img[src]'));

    if (images.length === 0) {
      commitPreparedPreview(template.innerHTML, revision);
      return;
    }

    if (!allowLocalImages) {
      for (const image of images) {
        replaceWithImagePlaceholder(image, '本地图片已禁用，可在设置中启用');
      }
      commitPreparedPreview(template.innerHTML, revision);
      return;
    }

    const loadingTemplate = template.cloneNode(true) as HTMLTemplateElement;
    for (const image of loadingTemplate.content.querySelectorAll<HTMLImageElement>('img[src]')) {
      replaceWithImagePlaceholder(image, '正在加载本地图片');
    }
    commitPreparedPreview(loadingTemplate.innerHTML, revision);

    await prepareLocalImages(images, currentDocumentPath, revision);
    commitPreparedPreview(template.innerHTML, revision);
  }

  function createPreviewTemplate(currentHtml: string): HTMLTemplateElement {
    const template = document.createElement('template');
    template.innerHTML = currentHtml;
    return template;
  }

  async function prepareLocalImages(
    images: HTMLImageElement[],
    currentDocumentPath: string | null,
    revision: number
  ) {
    let nextIndex = 0;
    const workerCount = Math.min(IMAGE_LOAD_CONCURRENCY, images.length);
    const workers = Array.from({ length: workerCount }, async () => {
      while (revision === resourceRevision) {
        const index = nextIndex;
        nextIndex += 1;
        if (index >= images.length) return;

        const image = images[index];
        const source = image.getAttribute('src');
        if (!source) {
          replaceWithImagePlaceholder(image, '图片地址为空');
          continue;
        }

        try {
          const loaded = await api.loadLocalImage(currentDocumentPath, source);
          if (revision !== resourceRevision) return;
          image.src = loaded.dataUrl;
          image.title = loaded.path;
        } catch (error) {
          if (revision !== resourceRevision) return;
          replaceWithImagePlaceholder(image, toAppError(error).message);
        }
      }
    });
    await Promise.all(workers);
  }

  function commitPreparedPreview(nextHtml: string, revision: number) {
    if (revision !== resourceRevision) return;
    preparedHtml = nextHtml;
  }

  function assignHeadingIds(root: ParentNode) {
    const seen = new Map<string, number>();
    for (const heading of root.querySelectorAll<HTMLElement>('h1, h2, h3, h4, h5, h6')) {
      if (heading.id) continue;
      const base = slugify(heading.textContent ?? '') || 'section';
      const count = (seen.get(base) ?? 0) + 1;
      seen.set(base, count);
      heading.id = count === 1 ? base : `${base}-${count}`;
    }
  }

  function slugify(value: string): string {
    return value
      .normalize('NFKC')
      .trim()
      .toLocaleLowerCase()
      .replace(/[^\p{L}\p{N}]+/gu, '-')
      .replace(/^-+|-+$/g, '');
  }

  function replaceWithImagePlaceholder(image: HTMLImageElement, message: string) {
    const placeholder = document.createElement('span');
    placeholder.className = 'local-image-placeholder';
    placeholder.setAttribute('role', 'img');
    placeholder.textContent = `${image.alt || '图片'}：${message}`;
    image.replaceWith(placeholder);
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
    {@html preparedHtml}
  {:else}
    <div class="preview-empty">
      <h2>预览将在这里显示</h2>
      <p>开始输入 Markdown 后会自动刷新。</p>
    </div>
  {/if}
</section>
