<script lang="ts" module>
  import type { Component } from 'svelte';

  export interface MenuItem {
    label: string;
    onclick?: () => void;
    danger?: boolean;
    disabled?: boolean;
    checked?: boolean;
    separator?: boolean;
    /** A caption over the items after it (the label), not an item. */
    heading?: boolean;
    /** CSS color for a dot in front of the label. */
    color?: string;
    /** An icon in front of the label. */
    icon?: Component<{ size?: number }>;
    /** Dim text at the end of the row, e.g. a battery level. */
    detail?: string;
    /** Colors the detail: low or critical. */
    tone?: 'warning' | 'danger';
  }
</script>

<script lang="ts">
  // A button that opens a list of items below it (above when there is no room). The list is
  // fixed to the window, so a scrolling panel does not cut it off, and scrolls when it is
  // longer than the room it has. It closes on a choice, a click elsewhere, Escape or Tab,
  // scrolling, resizing or when the window loses the focus.
  import type { Snippet } from 'svelte';
  import { tip } from '../../lib/tooltip.svelte';
  import MenuList from './MenuList.svelte';

  let {
    items,
    trigger,
    align = 'left',
    title = '',
    label,
    iconOnly = false,
    disabled = false,
    wide = false,
    class: cls = '',
    onopen,
  }: {
    items: MenuItem[];
    trigger: Snippet;
    align?: 'left' | 'right';
    /** The trigger's tooltip. */
    title?: string;
    /** The trigger's accessible name, when neither its text nor the title says it. */
    label?: string;
    /** The trigger shows only an icon: the title is its accessible name too. */
    iconOnly?: boolean;
    disabled?: boolean;
    /** The trigger fills the width it is given. */
    wide?: boolean;
    /** Classes of the trigger button. */
    class?: string;
    /** Called as the list opens (e.g. to load what it lists). */
    onopen?: () => void;
  } = $props();

  let open = $state(false);
  let root: HTMLElement;
  let popup: HTMLElement | undefined = $state();
  let list: HTMLElement | undefined = $state();
  let pos = $state<{ left: number; top: number; minWidth: number; maxHeight: number } | null>(null);

  /** Room kept free between the list and the window's edges. */
  const EDGE = 8;

  $effect(() => {
    if (!open || !popup || !list) return;
    const r = root.getBoundingClientRect();
    const below = window.innerHeight - r.bottom - EDGE;
    const above = r.top - EDGE;
    const h = popup.offsetHeight;
    const up = h + 4 > below && above > below;
    const maxHeight = Math.max(120, (up ? above : below) - 4);
    const w = Math.max(popup.offsetWidth, r.width);
    const left = Math.max(EDGE, Math.min(align === 'right' ? r.right - w : r.left, window.innerWidth - w - EDGE));
    const top = up ? r.top - 4 - Math.min(h, maxHeight) : r.bottom + 4;
    pos = { left, top, minWidth: r.width, maxHeight };
    // The checked item, else the first one, gets the focus (and is scrolled into view).
    const start =
      list.querySelector<HTMLElement>('.item.checked:not(:disabled)') ?? list.querySelector<HTMLElement>('.item:not(:disabled)');
    (start ?? list).focus({ preventScroll: true });
    start?.scrollIntoView({ block: 'nearest' });
  });

  function show() {
    if (disabled) return;
    pos = null;
    open = true;
    onopen?.();
  }

  function close(focusTrigger: boolean) {
    open = false;
    if (focusTrigger) root.querySelector<HTMLElement>('.trigger')?.focus();
  }

  function onpointerdown(e: PointerEvent) {
    if (open && !root.contains(e.target as Node)) close(false);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape' || e.key === 'Tab') {
      // Before a dialog sees the Escape, so only the list closes.
      e.preventDefault();
      e.stopPropagation();
      close(true);
    }
  }

  function onTriggerKey(e: KeyboardEvent) {
    if (!open && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault();
      show();
    }
  }

  function onscroll(e: Event) {
    if (open && !popup?.contains(e.target as Node)) close(false);
  }

  const dismiss = () => open && close(false);
</script>

<svelte:window onpointerdowncapture={onpointerdown} onkeydowncapture={onkeydown} onblur={dismiss} onresize={dismiss} />
<svelte:document onscrollcapture={onscroll} />

<div class="menu" class:wide bind:this={root}>
  <button
    type="button"
    class="trigger {cls}"
    class:open
    use:tip={open ? '' : title}
    aria-label={label ?? ((iconOnly && title) || undefined)}
    aria-haspopup="menu"
    aria-expanded={open}
    {disabled}
    onclick={() => (open ? close(false) : show())}
    onkeydown={onTriggerKey}
  >
    {@render trigger()}
  </button>
  {#if open}
    <div
      class="popup"
      bind:this={popup}
      style:left="{pos?.left ?? 0}px"
      style:top="{pos?.top ?? 0}px"
      style:min-width="{pos?.minWidth ?? 0}px"
      style:--menu-max-height={pos ? `${pos.maxHeight}px` : 'none'}
      style:visibility={pos ? 'visible' : 'hidden'}
    >
      <MenuList {items} onpick={() => close(true)} bind:el={list} />
    </div>
  {/if}
</div>

<style>
  .menu {
    position: relative;
    display: inline-flex;
    min-width: 0;
  }
  .menu.wide {
    display: flex;
  }
  .trigger {
    display: inline-flex;
    align-items: center;
    min-width: 0;
  }
  .wide .trigger {
    flex: 1;
  }
  .trigger:disabled {
    cursor: default;
  }
  .popup {
    position: fixed;
    z-index: var(--z-menu);
  }
</style>
