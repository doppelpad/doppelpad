<script lang="ts">
  // A button that opens a list of items below it (above when there is no room). The list is
  // fixed to the window, so a scrolling panel does not cut it off, and scrolls when it is
  // longer than the room it has. It closes on a choice, a click elsewhere, Escape or Tab,
  // scrolling, resizing or when the window loses the focus.
  import type { Snippet } from 'svelte';
  import { listen } from '../../lib/dom';
  import type { MenuItem } from '../../lib/menu';
  import { stepper } from '../../lib/padnav';
  import { placePopup, windowBox } from '../../lib/place';
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
    step,
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
    /** Steps through the choices while the controller drives the app (a drop-down list). */
    step?: (step: 1 | -1) => void;
  } = $props();

  let open = $state(false);
  let root: HTMLElement;
  let popup: HTMLElement | undefined = $state();
  let list: HTMLElement | undefined = $state();
  let pos = $state<{ left: number; top: number; minWidth: number; maxHeight: number } | null>(null);

  $effect(() => {
    if (!open || !popup || !list) return;
    const r = root.getBoundingClientRect();
    const size = { width: Math.max(popup.offsetWidth, r.width), height: popup.offsetHeight };
    const place = placePopup(r, size, windowBox(), { align: align === 'right' ? 'end' : 'start', minHeight: 120 });
    pos = { left: place.left, top: place.top, minWidth: r.width, maxHeight: place.maxHeight };
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

  // Listening only while open: a list of games has a menu per row.
  $effect(() => {
    if (!open) return;
    const off = [
      listen(window, 'pointerdown', onpointerdown, true),
      listen(window, 'keydown', onkeydown, true),
      listen(window, 'blur', dismiss),
      listen(window, 'resize', dismiss),
      listen(document, 'scroll', onscroll, true),
    ];
    return () => off.forEach((stop) => stop());
  });
</script>

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
    use:stepper={step}
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
