<script lang="ts">
  // The rows of a menu, shared by the menus behind a button and the context menu: items,
  // separators, ticks, colour dots and details. Up and down (Home, End) move between the rows
  // that can be chosen; choosing one calls `onpick` (which closes the menu) before the item.
  import type { MenuItem } from './Menu.svelte';

  let { items, onpick, el = $bindable() }: { items: MenuItem[]; onpick: () => void; el?: HTMLElement } = $props();

  /** One tickable item gives every item the tick column, so the labels line up. */
  const ticks = $derived(items.some((i) => i.checked !== undefined));

  function onkeydown(e: KeyboardEvent) {
    if (!el) return;
    const rows = [...el.querySelectorAll<HTMLButtonElement>('.item:not(:disabled)')];
    const i = rows.indexOf(document.activeElement as HTMLButtonElement);
    const to = { ArrowDown: i + 1, ArrowUp: i < 0 ? -1 : i - 1, Home: 0, End: -1 }[e.key];
    if (to === undefined || rows.length === 0) return;
    e.preventDefault();
    rows.at(to % rows.length)?.focus();
  }
</script>

<div class="list" role="menu" tabindex="-1" bind:this={el} {onkeydown}>
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
          onpick();
          item.onclick?.();
        }}
      >
        {#if ticks}<span class="check">{item.checked ? '✓' : ''}</span>{/if}
        {#if item.color}<span class="dot" style:background={item.color}></span>{/if}
        {item.label}
        {#if item.detail}
          <span class="detail" class:warning={item.tone === 'warning'} class:danger={item.tone === 'danger'}>{item.detail}</span>
        {/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .list {
    min-width: 200px;
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow);
    padding: 4px;
  }
  .list:focus {
    outline: none;
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
  .detail {
    margin-left: auto;
    padding-left: 18px;
    font-size: var(--fs-s);
    color: var(--text-dim);
  }
  .detail.warning {
    color: var(--warning);
  }
  .detail.danger {
    color: var(--danger);
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
