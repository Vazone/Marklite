<script lang="ts">
  import { onDestroy, tick } from 'svelte';

  export let busy = false;
  export let role: 'dialog' | 'alertdialog' = 'dialog';
  export let ariaLabel: string | undefined = undefined;
  export let labelledBy: string | undefined = undefined;
  export let describedBy: string | undefined = undefined;
  export let dialogClass = '';
  export let backdropClass = '';
  export let initialFocus: HTMLElement | null = null;
  export let onClose: () => void;
  export let onBackdropClick: (() => void) | undefined = undefined;
  export let onKeydown: ((event: KeyboardEvent) => void) | undefined = undefined;

  let dialog: HTMLElement;
  const restoreFocusTo = document.activeElement instanceof HTMLElement ? document.activeElement : null;

  void tick().then(() => focusInitial());

  onDestroy(() => {
    if (restoreFocusTo?.isConnected) restoreFocusTo.focus();
  });

  function focusableElements(): HTMLElement[] {
    if (!dialog) return [];
    return Array.from(
      dialog.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex="0"]'
      )
    );
  }

  function focusInitial(): void {
    if (busy) {
      dialog?.focus();
      return;
    }
    const target = initialFocus?.isConnected ? initialFocus : focusableElements()[0];
    (target ?? dialog)?.focus();
  }

  function closeIfAllowed(): void {
    if (!busy) (onBackdropClick ?? onClose)();
  }

  function handleKeydown(event: KeyboardEvent): void {
    event.stopPropagation();
    onKeydown?.(event);
    if (event.defaultPrevented) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      if (!busy) onClose();
      return;
    }
    if (event.key !== 'Tab') return;
    const focusable = focusableElements();
    if (!focusable.length) {
      event.preventDefault();
      dialog.focus();
      return;
    }
    const first = focusable[0];
    const last = focusable.at(-1)!;
    if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<div class={`modal-backdrop ${backdropClass}`.trim()} role="presentation" on:click={closeIfAllowed}>
  <section
    bind:this={dialog}
    class={dialogClass}
    {role}
    aria-modal="true"
    aria-label={ariaLabel}
    aria-labelledby={labelledBy}
    aria-describedby={describedBy}
    aria-busy={busy}
    tabindex="-1"
    on:click|stopPropagation
    on:keydown={handleKeydown}
  >
    <slot />
  </section>
</div>
