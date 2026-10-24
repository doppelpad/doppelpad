<script lang="ts">
  // The one tooltip bubble of the window (see lib/tooltip.svelte.ts): below the element it
  // belongs to, or above when there is no room, and always inside the window. Scrolling,
  // a key, a click or the window losing the focus hide it.
  import { hideTooltip, tooltip } from '../../lib/tooltip.svelte';

  let box: HTMLDivElement | undefined = $state();
  let pos = $state<{ left: number; top: number } | null>(null);

  const GAP = 6;
  const EDGE = 6;

  $effect(() => {
    const t = tooltip.current;
    if (!t || !box) {
      pos = null;
      return;
    }
    const w = box.offsetWidth;
    const h = box.offsetHeight;
    const a = t.anchor;
    const left = Math.max(EDGE, Math.min(a.left + a.width / 2 - w / 2, window.innerWidth - w - EDGE));
    const below = a.bottom + GAP;
    const top = below + h + EDGE > window.innerHeight && a.top - GAP - h >= EDGE ? a.top - GAP - h : below;
    pos = { left, top };
  });

  const hide = () => hideTooltip();
</script>

<svelte:window onkeydowncapture={hide} onblur={hide} onresize={hide} onwheelcapture={hide} />
<svelte:document onscrollcapture={hide} />

{#if tooltip.current}
  {#key tooltip.current.id}
    <div
      class="tooltip"
      role="tooltip"
      bind:this={box}
      style:left="{pos?.left ?? 0}px"
      style:top="{pos?.top ?? 0}px"
      style:visibility={pos ? 'visible' : 'hidden'}
    >
      {tooltip.current.text}
    </div>
  {/key}
{/if}

<style>
  .tooltip {
    position: fixed;
    z-index: var(--z-tooltip);
    max-width: 320px;
    padding: 5px 9px;
    border-radius: var(--radius-s);
    background: var(--bg-3);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    color: var(--text);
    font-size: var(--fs-s);
    line-height: 1.35;
    white-space: pre-line;
    pointer-events: none;
    animation: appear 0.1s ease-out;
  }
  @keyframes appear {
    from {
      opacity: 0;
    }
  }
</style>
