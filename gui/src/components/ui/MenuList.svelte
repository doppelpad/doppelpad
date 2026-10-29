<script lang="ts">
  // The rows of a menu, shared by the menus behind a button, the drop-down lists and the
  // context menu: items, separators, ticks, icons, colour dots and details. Up and down (Home,
  // End) move between the rows that can be chosen, typing jumps to the next row starting with
  // what was typed; choosing one calls `onpick` (which closes the menu) before the item. A list
  // longer than `--menu-max-height` scrolls.
  import type { MenuItem } from '../../lib/menu';

  let { items, onpick, el = $bindable() }: { items: MenuItem[]; onpick: () => void; el?: HTMLElement } = $props();

  /** One tickable item gives every item the tick column, so the labels line up. */
  const ticks = $derived(items.some((i) => i.checked !== undefined));

  /** What was typed for the type-ahead, and when the last key came. */
  let typed = '';
  let typedAt = 0;

  function rows(): HTMLButtonElement[] {
    return el ? [...el.querySelectorAll<HTMLButtonElement>('.item:not(:disabled)')] : [];
  }

  function focusRow(row: HTMLButtonElement | undefined) {
    row?.focus({ preventScroll: true });
    row?.scrollIntoView({ block: 'nearest' });
  }

  function onkeydown(e: KeyboardEvent) {
    const all = rows();
    const i = all.indexOf(document.activeElement as HTMLButtonElement);
    const to = { ArrowDown: i + 1, ArrowUp: i < 0 ? -1 : i - 1, Home: 0, End: -1, PageDown: i + 8, PageUp: Math.max(0, i - 8) }[e.key];
    if (to !== undefined && all.length > 0) {
      e.preventDefault();
      const n = all.length;
      focusRow(e.key.startsWith('Page') ? all[Math.min(to, n - 1)] : all.at(to % n));
      return;
    }
    if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey && e.key !== ' ') {
      e.preventDefault();
      const now = performance.now();
      typed = now - typedAt < 700 ? typed + e.key.toLowerCase() : e.key.toLowerCase();
      typedAt = now;
      // A repeated letter steps through the rows that start with it.
      const repeat = typed.length > 1 && [...typed].every((c) => c === typed[0]);
      const prefix = repeat ? typed[0]! : typed;
      const from = repeat || typed.length === 1 ? i + 1 : Math.max(i, 0);
      const order = [...all.slice(from), ...all.slice(0, from)];
      focusRow(order.find((r) => (r.dataset.label ?? '').toLowerCase().startsWith(prefix)));
    }
  }
</script>

<div class="list" role="menu" tabindex="-1" bind:this={el} {onkeydown}>
  {#each items as item, i (i)}
    {#if item.separator}
      <div class="sep"></div>
    {:else if item.heading}
      <div class="heading" role="presentation">{item.label}</div>
    {:else}
      <button
        type="button"
        role={item.checked === undefined ? 'menuitem' : 'menuitemradio'}
        aria-checked={item.checked}
        class="item"
        class:danger={item.danger}
        class:checked={item.checked}
        data-label={item.label}
        disabled={item.disabled}
        onclick={() => {
          onpick();
          item.onclick?.();
        }}
      >
        {#if ticks}<span class="check">{item.checked ? '✓' : ''}</span>{/if}
        {#if item.color}<span class="dot" style:background={item.color}></span>{/if}
        {#if item.icon}<span class="icon"><item.icon size={15} /></span>{/if}
        <span class="label">{item.label}</span>
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
    max-height: var(--menu-max-height, none);
    overflow-y: auto;
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
  .item:hover:not(:disabled),
  .item:focus-visible {
    background: var(--bg-3);
    outline: none;
  }
  .item:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .item.danger {
    color: var(--danger);
  }
  .check {
    flex: none;
    width: 14px;
    color: var(--accent);
  }
  .icon {
    flex: none;
    display: inline-flex;
    color: var(--text-dim);
  }
  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
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
  .heading {
    padding: 8px 10px 3px;
    font-size: var(--fs-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .sep + .heading,
  .heading:first-child {
    padding-top: 4px;
  }
</style>
