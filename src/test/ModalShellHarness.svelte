<script lang="ts">
  import ModalShell from '../components/dialogs/ModalShell.svelte';

  export let busy = false;
  export let onClose: () => void;
</script>

<ModalShell {busy} {onClose} ariaLabel="测试模态框">
  <button type="button">第一项</button>
  <button type="button">最后一项</button>
</ModalShell>
