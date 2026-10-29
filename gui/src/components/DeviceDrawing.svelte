<script lang="ts">
  // A controller drawing that lights the parts held and moves the stick caps: the controller
  // views, the live output and the controller overlay. The parts are looked up once per
  // drawing, not on every live state; `marks` adds the editor's own classes (mapped, selected).
  import type { DeviceLayout } from '../assets/devices/layouts';
  import { stickCapTransform } from '../lib/model/devices';
  import type { ControlName } from '../lib/types';

  let {
    layout,
    pressed,
    axes,
    marks,
    el = $bindable(),
    onclick,
    oncontextmenu,
  }: {
    layout: DeviceLayout;
    /** The drawn controls to light (see `pressedChips`). */
    pressed: ReadonlySet<ControlName>;
    /** The sticks' deflection, as in `PadState.axes`. */
    axes: readonly number[];
    /** More classes per drawn control, e.g. `mapped`. */
    marks?: ReadonlyMap<ControlName, Record<string, boolean>>;
    el?: HTMLDivElement;
    onclick?: (e: MouseEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
  } = $props();

  /** The drawing's parts and stick caps, once it is in the page. */
  let parts = $state.raw<SVGGElement[]>([]);
  let sticks = $state.raw<SVGGElement[]>([]);
  $effect(() => {
    void layout.svg;
    if (!el) return;
    parts = [...el.querySelectorAll<SVGGElement>('[data-control]')];
    sticks = [...el.querySelectorAll<SVGGElement>('[data-stick]')];
  });

  /** The lit parts as one string: a new live state with the same parts held changes nothing. */
  const lit = $derived([...pressed].sort().join('\n'));
  $effect(() => {
    const on = new Set(lit.split('\n'));
    for (const g of parts) g.classList.toggle('pressed', on.has(g.dataset.control ?? ''));
  });

  $effect(() => {
    for (const g of parts) {
      const m = marks?.get(g.dataset.control ?? '');
      for (const cls of ['mapped', 'selected']) g.classList.toggle(cls, !!m?.[cls]);
    }
  });

  $effect(() => {
    for (const g of sticks) {
      g.querySelector('.stick-cap')?.setAttribute('transform', stickCapTransform(axes, g.dataset.stick === 'Left' ? 'left' : 'right'));
    }
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="drawing" bind:this={el} {onclick} {oncontextmenu}>
  {@html layout.svg}
</div>

<style>
  .drawing :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }
  .drawing :global([data-control]) {
    color: var(--line-strong);
  }
  .drawing :global([data-control].pressed) {
    color: var(--accent-hover);
  }
  .drawing :global([data-control].pressed .hit) {
    fill-opacity: 0.4;
  }
  .drawing :global(.body) {
    color: var(--drawing-body, var(--text-faint));
  }
</style>
