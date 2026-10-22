<script lang="ts">
  // The menu that opens at the pointer on a right click, or below a focused control on the menu
  // key or Shift+F10 (`app.openContextMenu`). It stays inside the window, takes the focus, and
  // closes on a click elsewhere, Escape, scrolling, resizing or when the window loses the
  // focus; without a choice the focus goes back where it came from.
  import { app } from '../../lib/state/app.svelte';
  import MenuList from './MenuList.svelte';

  let box: HTMLDivElement | undefined = $state();
  let list: HTMLElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });

  $effect(() => {
    const m = app.contextMenu;
    if (!m || !box) return;
    const w = box.offsetWidth;
    const h = box.offsetHeight;
    const left = Math.max(4, Math.min(m.x, window.innerWidth - w - 4));
    const top = m.y + h + 4 > window.innerHeight ? Math.max(4, m.y - h) : m.y;
    pos = { left, top };
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
</script>

<svelte:window onpointerdowncapture={onpointerdown} onkeydowncapture={onkeydown} onblur={dismiss} onresize={dismiss} />
<svelte:document onscrollcapture={dismiss} />

{#if app.contextMenu}
  <div class="context" style="left: {pos.left}px; top: {pos.top}px" bind:this={box}>
    <MenuList items={app.contextMenu.items} onpick={() => close(true)} bind:el={list} />
  </div>
{/if}

<style>
  .context {
    position: fixed;
    z-index: var(--z-menu);
  }
</style>
