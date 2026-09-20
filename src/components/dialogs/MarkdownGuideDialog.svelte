<script lang="ts">
  import { Check, Copy, Search } from 'lucide-svelte';
  import ModalShell from './ModalShell.svelte';
  import {
    filterMarkdownGuide,
    guideSupportLabel,
    markdownGuideGroupsFor,
    type MarkdownGuideGroup
  } from '../../lib/markdownGuide';
  import { currentLanguage, translator } from '../../lib/i18n';

  export let open = false;
  export let onClose: () => void;

  let group: MarkdownGuideGroup = 'basic';
  let query = '';
  let copiedTopicId: string | null = null;
  let copyError = false;

  $: groups = markdownGuideGroupsFor($currentLanguage);
  $: topics = filterMarkdownGuide(group, query, $currentLanguage);

  async function copyExample(id: string, example: string) {
    try {
      await navigator.clipboard.writeText(example);
      copyError = false;
      copiedTopicId = id;
      window.setTimeout(() => {
        if (copiedTopicId === id) copiedTopicId = null;
      }, 1600);
    } catch {
      copiedTopicId = null;
      copyError = true;
    }
  }
</script>

{#if open}
  <ModalShell dialogClass="markdown-guide-dialog" ariaLabel={$translator('guide.title')} {onClose}>
    <header class="dialog-header">
      <div>
        <h2>{$translator('guide.title')}</h2>
        <p>{$translator('guide.subtitle')}</p>
      </div>
      <button type="button" class="ghost-button" on:click={onClose}>{$translator('common.close')}</button>
    </header>

    <div class="markdown-guide-body">
      <aside class="markdown-guide-nav" aria-label={$translator('guide.groups')}>
        {#each groups as item}
          <button
            type="button"
            class:active={group === item.id}
            aria-pressed={group === item.id}
            on:click={() => (group = item.id)}
          >
            <strong>{item.label}</strong>
            <span>{item.description}</span>
          </button>
        {/each}
      </aside>

      <section class="markdown-guide-content">
        <label class="markdown-guide-search">
          <Search size={16} />
          <input bind:value={query} placeholder={$translator('guide.search')} aria-label={$translator('guide.searchAria')} />
        </label>
        {#if copyError}
          <p class="markdown-guide-copy-error" role="status">{$translator('guide.clipboardError')}</p>
        {/if}

        <div class="markdown-guide-topics" aria-live="polite">
          {#each topics as topic (topic.id)}
            <article class="markdown-guide-topic">
              <header>
                <h3>{topic.title}</h3>
                <span class={`guide-support ${topic.support}`}>{guideSupportLabel(topic.support, $currentLanguage)}</span>
              </header>
              <p>{topic.summary}</p>
              <div class="markdown-guide-example">
                <pre><code>{topic.example}</code></pre>
                <button
                  type="button"
                  aria-label={$translator('guide.copyExample', { title: topic.title })}
                  on:click={() => void copyExample(topic.id, topic.example)}
                >
                  {#if copiedTopicId === topic.id}<Check size={15} /> {$translator('common.copied')}{:else}<Copy size={15} /> {$translator('common.copy')}{/if}
                </button>
              </div>
              {#if topic.note}<small>{topic.note}</small>{/if}
            </article>
          {:else}
            <div class="markdown-guide-empty">{$translator('guide.empty')}</div>
          {/each}
        </div>
      </section>
    </div>
  </ModalShell>
{/if}
