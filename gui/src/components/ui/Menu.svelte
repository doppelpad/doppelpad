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
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    items,
    trigger,
    align = 'left',
    title = '',
  }: { items: MenuItem[]; trigger: Snippet; align?: 'left' | 'right'; title?: string } = $props();

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
  <button type="button" class="trigger" {title} onclick={() => (open = !open)}>
    {@render trigger()}
  </button>
  {#if open}
    <div class="popup" class:right={align === 'right'} class:up role="menu" bind:this={popup}>
      {#each items as item, i (i)}
        {#if item.separator}
          <div class="sep"></div>
        {:else}
          <button
            type="button"
            role="menuitem"
            class="item"
            class:danger={item.danger}
            disabled={item.disabled}
            onclick={() => {
              open = false;
              item.onclick?.();
            }}
          >
            {#if item.checked !== undefined}<span class="check">{item.checked ? '✓' : ''}</span>{/if}
            {#if item.color}<span class="dot" style:background={item.color}></span>{/if}
            {item.label}
          </button>
        {/if}
      {/each}
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
    min-width: 200px;
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow);
    padding: 4px;
    z-index: 50;
  }
  .popup.right {
    left: auto;
    right: 0;
  }
  .popup.up {
    top: auto;
    bottom: calc(100% + 4px);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    text-align: left;
    padding: 7px 10px;
    border-radius: var(--radius-s);
    white-space: nowrap;
  }
  .item:hover:not(:disabled) {
    background: var(--bg-3);
  }
  .item:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .item.danger {
    color: var(--danger);
  }
  .check {
    width: 14px;
    color: var(--accent);
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .sep {
    height: 1px;
    background: var(--line);
    margin: 4px 2px;
  }
</style>
