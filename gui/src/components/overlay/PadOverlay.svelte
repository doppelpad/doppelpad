<script lang="ts">
  // The controller in use with its pressed inputs, sent by the app (pad_overlay.rs, see
  // lib/overlay.ts). It sits in the chosen corner of its window.
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { DEVICE_LAYOUTS } from '../../assets/devices/layouts';
  import { controlsFromPad, drawingsFor } from '../../lib/model/devices';
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
  const STICKS: Record<string, string> = { LS: 'LeftStick', RS: 'RightStick' };

  // Light up the pressed parts and move the stick caps.
  $effect(() => {
    if (!drawing || !content || !layout) return;
    const held = [...pressed];
    for (const g of drawing.querySelectorAll<SVGGElement>('[data-control]')) {
      const c = g.getAttribute('data-control') ?? '';
      const stick = STICKS[c];
      g.classList.toggle('pressed', pressed.has(c) || (!!stick && held.some((p) => p.startsWith(stick))));
    }
    const a = content.device.state.axes;
    for (const g of drawing.querySelectorAll<SVGGElement>('[data-stick]')) {
      const [x, y] = g.getAttribute('data-stick') === 'Left' ? [a[0], a[1]] : [a[2], a[3]];
      g.querySelector('.stick-cap')?.setAttribute('transform', `translate(${(x * 14).toFixed(1)} ${(-y * 14).toFixed(1)})`);
    }
  });
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
    max-width: 100%;
    max-height: 100%;
    width: 100%;
    padding: 12px;
    border-radius: var(--radius-l);
    background: rgba(15, 17, 21, 0.82);
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
</style>
