<script lang="ts">
  // The menu that opens at the pointer on a right click, or below a focused control on the menu
  // key or Shift+F10 (`app.openContextMenu`). It stays inside the window, takes the focus, and
  // closes on a click elsewhere, Escape, scrolling, resizing or when the window loses the
  // focus; without a choice the focus goes back where it came from.
  import { listen } from '../../lib/dom';
  import { placePopup, pointBox, windowBox } from '../../lib/place';
  import { app } from '../../lib/state/app.svelte';
  import MenuList from './MenuList.svelte';

  let box: HTMLDivElement | undefined = $state();
  let list: HTMLElement | undefined = $state();
  let pos = $state<{ left: number; top: number; maxHeight: number } | null>(null);

  $effect(() => {
    const m = app.contextMenu;
    if (!m || !box) {
      pos = null;
      return;
    }
    const size = { width: box.offsetWidth, height: box.offsetHeight };
    const place = placePopup(pointBox(m.x, m.y), size, windowBox(), { gap: 0, edge: 4, minHeight: 120 });
    pos = { left: place.left, top: place.top, maxHeight: place.maxHeight };
    (list?.querySelector<HTMLButtonElement>('.item:not(:disabled)') ?? list)?.focus();
  });

  function close(giveBack: boolean) {
    const from = app.contextMenu?.from;
    app.contextMenu = null;
    if (giveBack) from?.focus();
  }

  function onpointerdown(e: PointerEvent) {
    if (app.contextMenu && box && !box.contains(e.target as Node)) close(false);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!app.contextMenu) return;
    if (e.key === 'Escape' || e.key === 'Tab') {
      e.preventDefault();
      e.stopPropagation();
      close(true);
    }
  }

  const dismiss = () => app.contextMenu && close(false);

  // Listening only while it is open.
  $effect(() => {
    if (!app.contextMenu) return;
    const off = [
      listen(window, 'pointerdown', onpointerdown, true),
      listen(window, 'keydown', onkeydown, true),
      listen(window, 'blur', dismiss),
      listen(window, 'resize', dismiss),
      listen(document, 'scroll', dismiss, true),
    ];
    return () => off.forEach((stop) => stop());
  });
</script>

{#if app.contextMenu}
  <div
    class="context"
    style:left="{pos?.left ?? 0}px"
    style:top="{pos?.top ?? 0}px"
    style:--menu-max-height={pos ? `${pos.maxHeight}px` : 'none'}
    style:visibility={pos ? 'visible' : 'hidden'}
    bind:this={box}
  >
    <MenuList items={app.contextMenu.items} onpick={() => close(true)} bind:el={list} />
  </div>
{/if}

<style>
  .context {
    position: fixed;
    z-index: var(--z-menu);
  }
</style>
