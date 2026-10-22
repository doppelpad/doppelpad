<script lang="ts">
  // The small round × that removes a chip or a list entry.
  import { X } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';

  let { onclick, label }: { onclick: () => void; /** Tooltip (default "Remove"). */ label?: string } = $props();
</script>

<button class="remove" title={label ?? t('common.remove')} {onclick}><X size={11} /></button>

<style>
  .remove {
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    color: var(--text-dim);
  }
  .remove:hover {
    background: var(--danger);
    color: var(--on-danger);
  }
</style>
