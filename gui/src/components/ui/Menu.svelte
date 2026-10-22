<script lang="ts" module>
  export interface MenuItem {
    label: string;
    onclick?: () => void;
    danger?: boolean;
    disabled?: boolean;
    checked?: boolean;
    separator?: boolean;
    /** CSS color for a dot in front of the label. */
    color?: string;
    /** Dim text at the end of the row, e.g. a battery level. */
    detail?: string;
    /** Colors the detail: low or critical. */
    tone?: 'warning' | 'danger';
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import MenuList from './MenuList.svelte';

  let {
    items,
    trigger,
    align = 'left',
    title = '',
    iconOnly = false,
  }: {
    items: MenuItem[];
    trigger: Snippet;
    align?: 'left' | 'right';
    /** The trigger's tooltip. */
    title?: string;
    /** The trigger shows only an icon: the title is its accessible name too. */
    iconOnly?: boolean;
  } = $props();

  let open = $state(false);
  let up = $state(false);
  let root: HTMLElement;
  let popup: HTMLElement | undefined = $state();

  // Open upward when the popup would run past the bottom of the window and there is more room above.
  $effect(() => {
    if (!popup) return;
    const r = root.getBoundingClientRect();
    const below = window.innerHeight - r.bottom;
    up = popup.offsetHeight + 4 > below && r.top > below;
  });

  function onWindowClick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={(e) => e.key === 'Escape' && (open = false)} />

<div class="menu" bind:this={root}>
  <button
    type="button"
    class="trigger"
    {title}
    aria-label={(iconOnly && title) || undefined}
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    {@render trigger()}
  </button>
  {#if open}
    <div class="popup" class:right={align === 'right'} class:up bind:this={popup}>
      <MenuList {items} onpick={() => (open = false)} />
    </div>
  {/if}
</div>

<style>
  .menu {
    position: relative;
    display: inline-flex;
  }
  .trigger {
    display: inline-flex;
    align-items: center;
  }
  .popup {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: var(--z-menu);
  }
  .popup.right {
    left: auto;
    right: 0;
  }
  .popup.up {
    top: auto;
    bottom: calc(100% + 4px);
  }
</style>
