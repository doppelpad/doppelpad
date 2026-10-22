<script lang="ts">
  // The controller's light bar: for Main and, optionally, per layer — a mode, its colours and
  // how fast it animates. DualShock 4 and DualSense show it.
  import { Plus } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { layerIds, setLight } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { Light, LightMode } from '../../lib/types';
  import RemoveButton from '../ui/RemoveButton.svelte';
  import Slider from '../ui/Slider.svelte';
  import Toggle from '../ui/Toggle.svelte';

  const MODES: LightMode[] = ['solid', 'breath', 'rainbow', 'shimmer', 'heartbeat', 'strobe', 'off'];
  const def = $derived(app.catalog?.defaults.light);
  const disabled = $derived(app.readOnly);
  const layers = $derived(app.profile ? layerIds(app.profile).filter((l) => l !== 0) : []);

  let layer = $state(0);
  const own = $derived<Light | undefined>(
    layer === 0 ? app.profile?.lightbar : app.profile?.layers?.find((l) => l.id === layer)?.lightbar,
  );
  const light = $derived({ ...def, ...own } as Required<Light>);

  function set(patch: Partial<Light> | undefined) {
    app.edit((p) => setLight(p, layer, patch === undefined ? undefined : { ...own, ...patch }));
  }

  function setColor(i: number, c: string) {
    set({ colors: light.colors.map((x, j) => (j === i ? c : x)) });
  }
</script>

<div class="section-title">{t('light.title')}</div>
<p class="hint">{t('light.hint')}</p>

{#if layers.length > 0}
  <label class="row">
    <span>{t('light.layer')}</span>
    <select value={layer} onchange={(e) => (layer = Number(e.currentTarget.value))}>
      <option value={0}>{app.layerLabel(0)}</option>
      {#each layers as l (l)}<option value={l}>{app.layerLabel(l)}</option>{/each}
    </select>
  </label>
{/if}

<Toggle
  checked={!!own}
  {disabled}
  label={t(layer === 0 ? 'light.own_main' : 'light.own_layer')}
  onchange={(v) => set(v ? { ...def } : undefined)}
/>

{#if own}
  <div class="box card">
    <label class="row">
      <span>{t('light.mode')}</span>
      <select value={light.mode} {disabled} onchange={(e) => set({ mode: e.currentTarget.value as LightMode })}>
        {#each MODES as m (m)}<option value={m}>{t(`light.modes.${m}`)}</option>{/each}
      </select>
    </label>
    {#if light.mode !== 'off' && light.mode !== 'rainbow'}
      <div class="colors">
        {#each light.colors as c, i (i)}
          <span class="color">
            <input type="color" value={c} {disabled} onchange={(e) => setColor(i, e.currentTarget.value)} />
            {#if !disabled && light.colors.length > 1}
              <RemoveButton onclick={() => set({ colors: light.colors.filter((_, j) => j !== i) })} />
            {/if}
          </span>
        {/each}
        {#if !disabled && light.colors.length < 10}
          <button class="icon-btn" title={t('light.add_color')} onclick={() => set({ colors: [...light.colors, light.colors.at(-1) ?? '#ffffff'] })}><Plus size={14} /></button>
        {/if}
      </div>
      {#if light.colors.length > 1}
        <Toggle checked={light.cycle} {disabled} label={t('light.cycle')} onchange={(v) => set({ cycle: v || undefined })} />
      {/if}
    {/if}
    {#if light.mode !== 'off' && (light.mode !== 'solid' || light.cycle)}
      <Slider
        label={t('light.period')}
        value={light.period_ms}
        def={def?.period_ms}
        min={100}
        max={10000}
        step={100}
        unit={t('common.ms')}
        {disabled}
        onchange={(v) => set({ period_ms: Math.round(v) })}
      />
    {/if}
  </div>
{/if}

<style>
  .section-title {
    margin-top: 10px;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    max-width: 360px;
  }
  .box {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px 16px;
  }
  .colors {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .color {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .color input {
    width: 34px;
  }
</style>
