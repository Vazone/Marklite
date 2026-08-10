<script lang="ts">
  import { onMount } from 'svelte';
  import { Search } from 'lucide-svelte';
  import type { CommandItem } from '../../lib/commands';

  export let open = false;
  export let commands: CommandItem[] = [];
  export let onClose: () => void = () => {};

  let query = '';
  let selectedIndex = 0;
  let input: HTMLInputElement;

  $: filtered = commands.filter((command) =>
    `${command.title} ${command.category} ${command.shortcut ?? ''}`.toLowerCase().includes(query.toLowerCase())
  );

  $: if (selectedIndex >= filtered.length) selectedIndex = 0;

  onMount(() => {
    const handler = (event: KeyboardEvent) => {
      if (!open) return;

      if (event.key === 'Escape') {
        event.preventDefault();
        onClose();
      }

      if (event.key === 'ArrowDown') {
        event.preventDefault();
        selectedIndex = Math.min(filtered.length - 1, selectedIndex + 1);
      }

      if (event.key === 'ArrowUp') {
        event.preventDefault();
        selectedIndex = Math.max(0, selectedIndex - 1);
      }

      if (event.key === 'Enter' && filtered[selectedIndex]) {
        event.preventDefault();
        run(filtered[selectedIndex]);
      }
    };

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  });

  $: if (open) {
    window.setTimeout(() => input?.focus(), 0);
  } else {
    query = '';
    selectedIndex = 0;
  }

  function run(command: CommandItem) {
    command.action();
    onClose();
  }
</script>

{#if open}
  <div class="modal-backdrop palette-backdrop" role="presentation" on:click={onClose}>
    <section
      class="command-palette"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      on:click|stopPropagation
      on:keydown|stopPropagation
    >
      <div class="palette-input">
        <Search size={18} />
        <input bind:this={input} bind:value={query} placeholder="输入命令名称" />
      </div>
      <div class="palette-results">
        {#each filtered as command, index}
          <button type="button" class:active={index === selectedIndex} on:click={() => run(command)}>
            <span>
              <strong>{command.title}</strong>
              <small>{command.category}</small>
            </span>
            {#if command.shortcut}
              <kbd>{command.shortcut}</kbd>
            {/if}
          </button>
        {:else}
          <p class="muted">没有匹配的命令。</p>
        {/each}
      </div>
    </section>
  </div>
{/if}
