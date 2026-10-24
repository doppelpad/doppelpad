<script lang="ts">
  // Which virtual controller the config plays on (or none), and how the physical controller's
  // own input passes: hidden, passed through, muted where mapped, pressed again after a layer
  // change.
  import { Gamepad2, Keyboard, TriangleAlert } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import { platform } from '../../lib/state/window.svelte';
  import type { Output, VirtualPadType } from '../../lib/types';
  import Toggle from '../ui/Toggle.svelte';
  import { outputConfig } from '../../lib/model/output';

  const out = $derived(
    outputConfig(
      app.profile?.output,
      app.catalog?.defaults.output ?? { virtual_pad: 'xbox360', hide_physical: true, passthrough: true, mute_mapped_inputs: true },
    ),
  );

  const drivers = $derived(app.live?.drivers);

  function set(patch: Partial<Output>) {
    app.edit((p) => ({ ...p, output: { ...(p.output ?? {}), ...patch } }));
  }

  const PADS: { id: VirtualPadType; icon: typeof Gamepad2 }[] = [
    { id: 'xbox360', icon: Gamepad2 },
    { id: 'dual_shock4', icon: Gamepad2 },
    { id: 'none', icon: Keyboard },
  ];
</script>

<div class="section-title">{t('output.virtual_pad')}</div>
<div class="cards">
  {#each PADS as p (p.id)}
    <button class="card" class:active={out.virtual_pad === p.id} disabled={app.readOnly} onclick={() => set({ virtual_pad: p.id })}>
      <p.icon size={26} strokeWidth={1.5} />
      <span>{t(`output.${p.id}`)}</span>
    </button>
  {/each}
</div>
{#if out.virtual_pad !== 'none' && drivers && !drivers.virtual_pads.available}
  <div class="warn">
    <TriangleAlert size={15} />
    {platform === 'macos' ? t('output.no_virtual_pads_macos') : t('output.driver_missing', { driver: 'ViGEmBus' })}
  </div>
{/if}

<div class="options">
  <Toggle
    checked={out.hide_physical}
    disabled={app.readOnly}
    label={t('output.hide_physical')}
    onchange={(v) => set({ hide_physical: v })}
  />
  {#if out.hide_physical && drivers && !drivers.device_hiding.available}
    <div class="warn">
      <TriangleAlert size={15} />
      {t('output.hide_missing')}
    </div>
  {/if}
  <Toggle checked={out.passthrough} disabled={app.readOnly} label={t('output.passthrough')} onchange={(v) => set({ passthrough: v })} />
  <Toggle
    checked={out.mute_mapped_inputs}
    disabled={app.readOnly}
    label={t('output.mute_mapped')}
    onchange={(v) => set({ mute_mapped_inputs: v })}
  />
  <Toggle
    checked={app.profile?.layer_change?.repress ?? true}
    disabled={app.readOnly}
    label={t('output.repress')}
    onchange={(v) => app.edit((p) => ({ ...p, layer_change: v ? undefined : { repress: false } }))}
  />
  <p class="hint">{t('output.repress_hint')}</p>
</div>

<style>
  .cards {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 18px 10px;
    color: var(--text-dim);
    text-align: center;
  }
  .card:hover:not(:disabled) {
    border-color: var(--line-strong);
    color: var(--text);
  }
  .card.active {
    border-color: var(--accent);
    color: var(--text);
    background: var(--accent-soft);
  }
  .card.active :global(svg) {
    color: var(--accent);
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
    border-radius: var(--radius-s);
    padding: 8px 10px;
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 10px;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }
</style>
