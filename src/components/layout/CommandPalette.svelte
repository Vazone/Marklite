<script lang="ts">
  import { Search } from 'lucide-svelte';
  import type { CommandItem } from '../../lib/commands';
  import ModalShell from '../dialogs/ModalShell.svelte';
  import { translator } from '../../lib/i18n';

  export let open = false;
  export let commands: CommandItem[] = [];
  export let onClose: () => void;

  let query = '';
  let selectedIndex = 0;
  let input: HTMLInputElement;

  $: filtered = commands.filter((command) =>
    `${command.title} ${command.category} ${command.shortcut ?? ''}`.toLowerCase().includes(query.toLowerCase())
  );

  $: if (filtered.length === 0) selectedIndex = 0;
  else if (selectedIndex >= filtered.length) selectedIndex = filtered.length - 1;

  $: if (!open) {
    query = '';
    selectedIndex = 0;
  }

  function run(command: CommandItem) {
    command.action();
    onClose();
  }

  function handlePaletteKeydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (filtered.length > 0) selectedIndex = Math.min(filtered.length - 1, selectedIndex + 1);
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (filtered.length > 0) selectedIndex = Math.max(0, selectedIndex - 1);
      return;
    }
    if (event.key === 'Enter' && filtered[selectedIndex]) {
      event.preventDefault();
      run(filtered[selectedIndex]);
    }
  }
</script>

{#if open}
  <ModalShell
    {onClose}
    ariaLabel={$translator('palette.title')}
    dialogClass="command-palette"
    backdropClass="palette-backdrop"
    initialFocus={input}
    onKeydown={handlePaletteKeydown}
  >
    <div class="palette-input">
      <Search size={18} />
      <input bind:this={input} bind:value={query} placeholder={$translator('palette.placeholder')} />
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
        <p class="muted">{$translator('palette.empty')}</p>
      {/each}
    </div>
  </ModalShell>
{/if}
