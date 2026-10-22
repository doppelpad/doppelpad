<script lang="ts">
  // A pill with a label (the children) and, when it can be removed, a small × at its end.
  import type { Snippet } from 'svelte';
  import RemoveButton from './RemoveButton.svelte';

  let {
    size = 'm',
    active = false,
    onremove,
    removeLabel,
    children,
  }: {
    size?: 'm' | 's';
    /** Highlighted, e.g. while its parameters are edited. */
    active?: boolean;
    /** Shows the × when set. */
    onremove?: () => void;
    removeLabel?: string;
    children: Snippet;
  } = $props();
</script>

<span class="chip {size}" class:active>
  {@render children()}
  {#if onremove}<RemoveButton onclick={onremove} label={removeLabel} />{/if}
</span>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border-radius: var(--radius-pill);
    background: var(--bg-3);
    border: 1px solid var(--line-strong);
    font-weight: 500;
  }
  .m {
    padding: 5px 8px 5px 10px;
  }
  .s {
    padding: 3px 6px 3px 10px;
    font-size: var(--fs-s);
  }
  .active {
    border-color: var(--accent);
  }
</style>
