<script lang="ts">
  // What Doppelpad sends right now, to test a config without a game: the virtual controller's
  // pressed buttons, sticks and triggers, drawn on the controller in use (where the player's
  // buttons are), the keys and mouse buttons held, and the layer. Remapping pauses while this
  // window has the focus unless the user keeps it running, so the panel offers that switch.
  import { X } from '@lucide/svelte';
  import { DEVICE_LAYOUTS } from '../assets/devices/layouts';
  import { t } from '../lib/i18n/index.svelte';
  import { comboLabel } from '../lib/model/labels';
  import { controlsFromPad, drawingsFor, moveStickCaps, triggerAxis } from '../lib/model/devices';
  import { app } from '../lib/state/app.svelte';
  import type { LiveDevice } from '../lib/types';
  import Toggle from './ui/Toggle.svelte';
  import { tip } from '../lib/tooltip.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const session = $derived(app.slotSet.session);
  const output = $derived(session?.output ?? null);
  /** The virtual controller as a device, so the drawing helpers can show it. */
  const virtual = $derived<LiveDevice | null>(
    output?.pad && session?.virtual_pad
      ? {
          id: 'virtual',
          name: session.virtual_pad,
          family: session.virtual_pad === 'dual_shock4' ? 'dual_shock4' : 'xbox',
          backend: 'virtual',
          vendor_id: 0,
          product_id: 0,
          state: output.pad,
          gyro_calibration: null,
          battery: null,
          raw: null,
          mirror: false,
        }
      : null,
  );
  /** The controller the session reads: the drawing is that one, so the output shows where the player's buttons are. */
  const reading = $derived(app.live?.devices.find((d) => d.id === session?.device) ?? app.primary);
  const layout = $derived(virtual ? DEVICE_LAYOUTS[drawingsFor(reading ?? virtual).front] : null);
  const pressed = $derived(virtual ? controlsFromPad(virtual, app.catalog?.pad_buttons ?? []) : new Set<string>());

  let drawing: HTMLDivElement | undefined = $state();
  $effect(() => {
    if (!drawing || !virtual) return;
    const held = [...pressed];
    for (const g of drawing.querySelectorAll<SVGGElement>('[data-control]')) {
      const c = g.getAttribute('data-control') ?? '';
      const stick = c === 'LS' ? 'LeftStick' : c === 'RS' ? 'RightStick' : null;
      g.classList.toggle('pressed', pressed.has(c) || (!!stick && held.some((p) => p.startsWith(stick))));
    }
    moveStickCaps(drawing, virtual.state.axes);
  });

  /** Why nothing is sent right now, if so. */
  const stopped = $derived(
    !app.live?.remap_on
      ? t('live_output.remap_off')
      : app.live?.suspended
        ? t('live_output.paused')
        : !session
          ? t('live_output.no_session')
          : null,
  );
</script>

<div class="live-output">
  <div class="head">
    <strong>{t('live_output.title')}</strong>
    {#if session}<span class="layer">{app.layerLabel(session.layer)}</span>{/if}
    <button class="icon-btn" use:tip={t('common.close')} onclick={onclose}><X size={14} /></button>
  </div>
  {#if stopped}
    <p class="note">{stopped}</p>
  {/if}
  {#if app.live?.suspended && !app.settings.stay_live}
    <Toggle checked={false} label={t('settings.stay_live')} onchange={(v) => app.setSettings({ stay_live: v })} />
  {/if}
  {#if layout && virtual && session?.virtual_pad}
    <div class="drawing" bind:this={drawing}>{@html layout.svg}</div>
    <p class="sent-as">{t('live_output.sent_as', { pad: t(`output.${session.virtual_pad}`) })}</p>
    <div class="triggers">
      {#each ['left', 'right'] as const as side (side)}
        <div class="bar" use:tip={side === 'left' ? 'LT' : 'RT'}>
          <span style="width: {Math.round(Math.max(0, Math.min(1, triggerAxis(virtual, side))) * 100)}%"></span>
        </div>
      {/each}
    </div>
  {:else if session && !session.virtual_pad}
    <p class="hint">{t('live_output.no_pad')}</p>
  {/if}
  <div class="held">
    {#each output?.keys ?? [] as k (k)}<span class="chip">{comboLabel(`Key:${k}`, null)}</span>{/each}
    {#each output?.mouse_buttons ?? [] as b (b)}<span class="chip"
        >{comboLabel(`Mouse:${b.charAt(0).toUpperCase()}${b.slice(1)}`, null)}</span
      >{/each}
    {#if output && output.keys.length === 0 && output.mouse_buttons.length === 0}<span class="hint">{t('live_output.no_keys')}</span>{/if}
  </div>
</div>

<style>
  .live-output {
    position: absolute;
    top: 48px;
    right: 12px;
    z-index: 5;
    width: 260px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-m);
    box-shadow: var(--shadow);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .head strong {
    flex: 1;
  }
  .layer {
    font-size: var(--fs-xs);
    color: var(--text-dim);
  }
  .note {
    margin: 0;
    color: var(--warning);
    font-size: var(--fs-s);
  }
  .drawing,
  .drawing :global(svg) {
    width: 100%;
    height: auto;
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
  .sent-as {
    margin: -4px 0 0;
    text-align: center;
    color: var(--text-faint);
    font-size: var(--fs-xs);
  }
  .triggers {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--bg-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .held {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    min-height: 22px;
  }
  .chip {
    padding: 2px 6px;
    border-radius: var(--radius-s);
    background: color-mix(in srgb, var(--accent) 18%, var(--bg-1));
    font-size: var(--fs-s);
  }
</style>
