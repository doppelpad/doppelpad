<script lang="ts">
  // The controller in use with its pressed inputs, sent by the app (pad_overlay.rs, see
  // lib/overlay.ts). It sits in the chosen corner of its window.
  import { api } from '../../lib/api';
  import { onMount } from 'svelte';
  import { DEVICE_LAYOUTS } from '../../assets/devices/layouts';
  import type { Side } from '../../lib/model/analog';
  import { pressedChips } from '../../lib/model/chips';
  import { SIDES, SIDE_NAMES } from '../../lib/model/controls';
  import { controlsFromPad, drawingsFor, triggerAxis } from '../../lib/model/devices';
  import { clamp } from '../../lib/model/util';
  import { overlayContent } from '../../lib/overlay';
  import type { PadOverlayContent } from '../../lib/types';
  import DeviceDrawing from '../DeviceDrawing.svelte';
  import CornerFrame from './CornerFrame.svelte';

  let content = $state<PadOverlayContent | null>(null);
  let buttons = $state<string[]>([]);
  let drawing: HTMLDivElement | undefined = $state();

  onMount(() => {
    api.catalog().then((c) => (buttons = c.pad_buttons));
    return overlayContent<PadOverlayContent>((c) => (content = c));
  });

  const layout = $derived(content ? DEVICE_LAYOUTS[drawingsFor(content.device).front] : null);
  /** The controls held, and the drawn ones they light. */
  const held = $derived(content ? controlsFromPad(content.device.state, buttons) : new Set<string>());
  const pressed = $derived(layout ? pressedChips(layout.controls, held) : new Set<string>());

  // The front drawing hides the triggers behind the bumpers, so a bar above each bumper
  // shows how far its trigger is pulled.
  const BAR = { h: 12, gap: 14 };
  type Bar = { name: 'LT' | 'RT'; side: Side; x: number; y: number; w: number; left: boolean };
  let bars = $state<Bar[]>([]);
  $effect(() => {
    const d = drawing;
    if (!d || !layout || layout.controls.some((lc) => lc.control === 'LT')) {
      bars = [];
      return;
    }
    bars = (['LB', 'RB'] as const).flatMap((bumper, i) => {
      const g = d.querySelector<SVGGElement>(`[data-control="${bumper}"]`);
      if (!g) return [];
      const b = g.getBBox();
      const side = SIDE_NAMES[i];
      return [{ name: SIDES[side].trigger, side, x: b.x, y: b.y - BAR.gap - BAR.h, w: b.width, left: !i }];
    });
  });
  const pull = (side: Side) => clamp(content ? triggerAxis(content.device.state, side) : 0, 0, 1);
</script>

{#if content && layout}
  {@const [, , vw, vh] = layout.viewBox}
  <CornerFrame corner={content.corner} opacity={content.opacity}>
    <div class="card" style="aspect-ratio: {vw} / {vh}">
      <DeviceDrawing {layout} {pressed} axes={content.device.state.axes} bind:el={drawing} />
      {#if bars.length}
        <svg class="triggers" viewBox={layout.viewBox.join(' ')} aria-hidden="true">
          {#each bars as bar (bar.name)}
            {@const w = bar.w * pull(bar.side)}
            <g class:pressed={held.has(bar.name)}>
              <rect class="track" x={bar.x} y={bar.y} width={bar.w} height={BAR.h} rx={BAR.h / 2} />
              {#if w > 0}
                <!-- fills from the outer end inwards -->
                <rect
                  class="fill"
                  x={bar.left ? bar.x : bar.x + bar.w - w}
                  y={bar.y}
                  width={w}
                  height={BAR.h}
                  rx={Math.min(BAR.h, w) / 2}
                />
              {/if}
            </g>
          {/each}
        </svg>
      {/if}
    </div>
  </CornerFrame>
{/if}

<style>
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
  .card :global(.drawing) {
    width: 100%;
    height: 100%;
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
