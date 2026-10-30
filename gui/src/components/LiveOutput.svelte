<script lang="ts">
  // What Doppelpad sends right now, to test a config without a game: the virtual controller's
  // pressed buttons, sticks and triggers, drawn on the controller in use (where the player's
  // buttons are), the keys and mouse buttons held, and the layer. Remapping pauses while this
  // window has the focus unless the user keeps it running, so the panel offers that switch.
  import { X } from '@lucide/svelte';
  import { DEVICE_LAYOUTS } from '../assets/devices/layouts';
  import { t } from '../lib/i18n/index.svelte';
  import { comboLabel } from '../lib/model/labels';
  import { pressedChips } from '../lib/model/chips';
  import { SIDE_NAMES, SIDES, controlLabel } from '../lib/model/controls';
  import { controlsFromPad, drawingsFor, triggerAxis } from '../lib/model/devices';
  import { clamp } from '../lib/model/util';
  import { app } from '../lib/state/app.svelte';
  import type { PadFamily } from '../lib/types';
  import DeviceDrawing from './DeviceDrawing.svelte';
  import Toggle from './ui/Toggle.svelte';
  import { tip } from '../lib/tooltip.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const session = $derived(app.slotSet.session);
  const output = $derived(session?.output ?? null);
  /** What the virtual controller sends now. */
  const virtual = $derived(session?.virtual_pad ? (output?.pad ?? null) : null);
  /** The family that draws the virtual controller when no controller is read. */
  const virtualFamily = $derived<PadFamily>(session?.virtual_pad === 'dual_shock4' ? 'dual_shock4' : 'xbox');
  /** The controller the session reads: the drawing is that one, so the output shows where the player's buttons are. */
  const reading = $derived(app.live?.devices.find((d) => d.id === session?.device) ?? app.primary);
  const layout = $derived(
    virtual ? DEVICE_LAYOUTS[drawingsFor(reading ?? { family: virtualFamily, vendor_id: 0, product_id: 0 }).front] : null,
  );
  const pressed = $derived(
    virtual && layout ? pressedChips(layout.controls, controlsFromPad(virtual, app.catalog?.pad_buttons ?? [])) : new Set<string>(),
  );
  /** The triggers are named as on the controller the drawing shows. */
  const family = $derived(reading?.family ?? virtualFamily);

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
    <DeviceDrawing {layout} {pressed} axes={virtual.axes} />
    <p class="sent-as">{t('live_output.sent_as', { pad: t(`output.${session.virtual_pad}`) })}</p>
    <div class="triggers">
      {#each SIDE_NAMES as side (side)}
        <div class="bar" use:tip={controlLabel(SIDES[side].trigger, family)}>
          <span style="width: {Math.round(clamp(triggerAxis(virtual, side), 0, 1) * 100)}%"></span>
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
    z-index: var(--z-float);
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
