<script lang="ts">
  // The controller in use with its pressed inputs, sent by the app (pad_overlay.rs, see
  // lib/overlay.ts). It sits in the chosen corner of its window.
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { DEVICE_LAYOUTS } from '../../assets/devices/layouts';
  import type { Side } from '../../lib/model/analog';
  import { SIDES, SIDE_NAMES } from '../../lib/model/controls';
  import { controlsFromPad, drawingsFor, moveStickCaps, triggerAxis } from '../../lib/model/devices';
  import { overlayContent } from '../../lib/overlay';
  import type { Catalog, Corner, LiveDevice } from '../../lib/types';

  type Content = { device: LiveDevice; opacity: number; corner: Corner };
  let content = $state<Content | null>(null);
  let buttons = $state<string[]>([]);
  let drawing: HTMLDivElement | undefined = $state();

  onMount(() => {
    invoke<Catalog>('catalog').then((c) => (buttons = c.pad_buttons));
    return overlayContent<Content>((c) => (content = c));
  });

  const layout = $derived(content ? DEVICE_LAYOUTS[drawingsFor(content.device).front] : null);
  const pressed = $derived(content ? controlsFromPad(content.device, buttons) : new Set<string>());
  const STICKS: Record<string, string> = { LS: SIDES.left.stick, RS: SIDES.right.stick };

  // Light up the pressed parts and move the stick caps.
  $effect(() => {
    if (!drawing || !content || !layout) return;
    const held = [...pressed];
    for (const g of drawing.querySelectorAll<SVGGElement>('[data-control]')) {
      const c = g.getAttribute('data-control') ?? '';
      const stick = STICKS[c];
      g.classList.toggle('pressed', pressed.has(c) || (!!stick && held.some((p) => p.startsWith(stick))));
    }
    moveStickCaps(drawing, content.device.state.axes);
  });

  // The front drawing hides the triggers behind the bumpers, so a bar above each bumper
  // shows how far its trigger is pulled.
  const BAR = { h: 12, gap: 14 };
  type Bar = { name: 'LT' | 'RT'; side: Side; x: number; y: number; w: number; left: boolean };
  let bars = $state<Bar[]>([]);
  $effect(() => {
    if (!drawing || !layout || layout.controls.some((lc) => lc.control === 'LT')) {
      bars = [];
      return;
    }
    bars = (['LB', 'RB'] as const).flatMap((bumper, i) => {
      const g = drawing!.querySelector<SVGGElement>(`[data-control="${bumper}"]`);
      if (!g) return [];
      const b = g.getBBox();
      const side = SIDE_NAMES[i];
      return [{ name: SIDES[side].trigger, side, x: b.x, y: b.y - BAR.gap - BAR.h, w: b.width, left: !i }];
    });
  });
  const pull = (side: Side) => Math.min(1, Math.max(0, content ? triggerAxis(content.device, side) : 0));
</script>

{#if content && layout}
  {@const [, , vw, vh] = layout.viewBox}
  <div
    class="frame"
    class:right={content.corner.endsWith('right')}
    class:bottom={content.corner.startsWith('bottom')}
    style:opacity={content.opacity / 100}
  >
    <div class="card" style="aspect-ratio: {vw} / {vh}">
      <div class="drawing" bind:this={drawing}>{@html layout.svg}</div>
      {#if bars.length}
        <svg class="triggers" viewBox={layout.viewBox.join(' ')} aria-hidden="true">
          {#each bars as bar (bar.name)}
            {@const w = bar.w * pull(bar.side)}
            <g class:pressed={pressed.has(bar.name)}>
              <rect class="track" x={bar.x} y={bar.y} width={bar.w} height={BAR.h} rx={BAR.h / 2} />
              {#if w > 0}
                <!-- fills from the outer end inwards -->
                <rect class="fill" x={bar.left ? bar.x : bar.x + bar.w - w} y={bar.y} width={w} height={BAR.h} rx={Math.min(BAR.h, w) / 2} />
              {/if}
            </g>
          {/each}
        </svg>
      {/if}
    </div>
  </div>
{/if}

<style>
  .frame {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: flex-start;
    justify-content: flex-start;
    padding: 6px;
  }
  .frame.right {
    justify-content: flex-end;
  }
  .frame.bottom {
    align-items: flex-end;
  }
  .card {
    position: relative;
    max-width: 100%;
    max-height: 100%;
    width: 100%;
    padding: 12px;
    border-radius: var(--radius-l);
    background: color-mix(in srgb, var(--bg-0) 82%, transparent);
    border: 1px solid var(--line-strong);
  }
  .drawing,
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
    fill-opacity: 0.45;
  }
  .drawing :global(.body) {
    color: var(--text-faint);
  }
  .triggers {
    position: absolute;
    inset: 12px;
    width: calc(100% - 24px);
    height: calc(100% - 24px);
  }
  .track {
    fill: none;
    stroke: var(--line-strong);
    stroke-width: 3;
  }
  .fill {
    fill: color-mix(in srgb, var(--accent) 55%, transparent);
  }
  .pressed .track {
    stroke: var(--accent-hover);
  }
  .pressed .fill {
    fill: var(--accent-hover);
  }
</style>
