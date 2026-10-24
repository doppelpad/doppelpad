<script lang="ts">
  // The head of a numbered item card (a crosshair, a radial menu): its preview, number, name
  // field and delete button.
  import { Trash2 } from '@lucide/svelte';
  import type { Snippet } from 'svelte';
  import { tip } from '../../lib/tooltip.svelte';

  let {
    preview,
    index,
    name,
    placeholder,
    disabled,
    removeLabel,
    onrename,
    onremove,
  }: {
    preview: Snippet;
    index: number;
    name: string | undefined;
    placeholder: string;
    disabled: boolean;
    removeLabel: string;
    /** The new name, `undefined` when cleared. */
    onrename: (name: string | undefined) => void;
    onremove: () => void;
  } = $props();
</script>

<header>
  {@render preview()}
  <span class="num">{index + 1}</span>
  <input
    class="name"
    value={name ?? ''}
    {placeholder}
    aria-label={placeholder}
    {disabled}
    onchange={(e) => onrename(e.currentTarget.value.trim() || undefined)}
  />
  {#if !disabled}
    <button class="icon-btn danger del" use:tip={removeLabel} onclick={onremove}><Trash2 size={15} /></button>
  {/if}
</header>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .num {
    color: var(--text-faint);
    font-weight: 600;
  }
  .name {
    flex: 1;
    min-width: 0;
  }
  .del {
    margin-left: auto;
  }
</style>
