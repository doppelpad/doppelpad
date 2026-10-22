<script lang="ts">
  // One on-screen notification. The app (osd.rs) sends each message (see lib/overlay.ts) and
  // shows and hides the window.
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { BatteryLow, BatteryWarning, Bell, Gamepad2, Hash, Layers, Power, PowerOff, Unplug } from '@lucide/svelte';
  import { setLanguage, t } from '../../lib/i18n/index.svelte';
  import { overlayContent } from '../../lib/overlay';
  import { layerColor } from '../../lib/theme/layers';
  import type { OsdMessage } from '../../lib/types';
  import { defaultLayerName } from '../../lib/model/labels';

  let msg = $state<OsdMessage | null>(null);

  function show(m: OsdMessage | null) {
    if (m) setLanguage(m.lang);
    msg = m;
  }

  onMount(() => overlayContent<OsdMessage>(show));

  const layerName = (layer: number, name: string | null) => name ?? defaultLayerName(layer);

  /** Icon, headline, second line and accent colour of the message. */
  const view = $derived.by(() => {
    if (!msg) return null;
    switch (msg.kind) {
      case 'remap_on':
        return { icon: Power, title: t('osd.remap_on'), detail: '', color: 'var(--ok)' };
      case 'remap_off':
        return { icon: PowerOff, title: t('osd.remap_off'), detail: '', color: 'var(--text-faint)' };
      case 'slot':
        return { icon: Hash, title: t('osd.slot', { n: msg.slot }), detail: msg.config ?? t('osd.slot_empty'), color: 'var(--accent)' };
      case 'layer':
        return { icon: Layers, title: layerName(msg.layer, msg.name), detail: t('osd.layer'), color: layerColor(msg.layer) };
      case 'connected':
        return { icon: Gamepad2, title: msg.name, detail: t('osd.connected'), color: 'var(--ok)' };
      case 'disconnected':
        return { icon: Unplug, title: msg.name, detail: t('osd.disconnected'), color: 'var(--warning)' };
      case 'battery_low':
        return { icon: BatteryLow, title: msg.name, detail: t('osd.battery_low', { level: msg.level }), color: 'var(--warning)' };
      case 'battery_critical':
        return { icon: BatteryWarning, title: msg.name, detail: t('osd.battery_critical', { level: msg.level }), color: 'var(--danger)' };
      case 'test':
        return { icon: Bell, title: 'Doppelpad', detail: t('osd.test'), color: 'var(--accent)' };
    }
  });

  const right = $derived(msg?.corner.endsWith('right') ?? true);
</script>

{#if msg && view}
  {#key msg.id}
    <div
      class="osd"
      style:opacity={msg.opacity / 100}
      style:zoom={msg.scale / 100}
      style:--tone={view.color}
      in:fly={{ x: right ? 40 : -40, duration: 180 }}
    >
      <span class="icon"><view.icon size={22} /></span>
      <span class="text">
        <span class="title">{view.title}</span>
        {#if view.detail}<span class="detail">{view.detail}</span>{/if}
      </span>
    </div>
  {/key}
{/if}

<style>
  .osd {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 18px;
    background: rgba(22, 26, 33, 0.96);
    border: 1px solid var(--line-strong);
    border-left: 4px solid var(--tone);
    border-radius: var(--radius-m);
    overflow: hidden;
  }
  .icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--tone) 18%, transparent);
    color: var(--tone);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title,
  .detail {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .title {
    font-size: 16px;
    font-weight: 600;
  }
  .detail {
    color: var(--text-dim);
    font-size: 13px;
  }
</style>
