<script lang="ts">
  // The config's crosshairs: shape, colours, size, position, magnifier, and the layers that
  // show each one. Mappings show them with the "Show crosshair N" command.
  import { Plus } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import {
    CROSSHAIR_SHAPES,
    MAX_CROSSHAIRS,
    addCrosshair,
    crosshairConfig,
    removeCrosshair,
    updateCrosshair,
  } from '../../lib/model/crosshair';
  import { layerIds } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { Crosshair, CrosshairShape as Shape } from '../../lib/types';
  import ItemHeader from '../ui/ItemHeader.svelte';
  import Slider from '../ui/Slider.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import CrosshairShape from './CrosshairShape.svelte';

  const crosshairs = $derived(app.profile?.crosshairs ?? []);
  const def = $derived(app.catalog?.defaults.crosshair);
  const magDef = $derived(app.catalog?.defaults.magnifier ?? { zoom: 2, size: 0.2 });
  const layers = $derived(app.profile ? layerIds(app.profile) : [0]);
  const disabled = $derived(app.readOnly);

  const full = (c: Crosshair) => crosshairConfig(c, def);

  function set(i: number, patch: Partial<Crosshair>) {
    app.edit((p) => updateCrosshair(p, i, patch));
  }

  function toggleLayer(i: number, c: Crosshair, id: number, on: boolean) {
    const now = c.layers ?? [];
    const next = on ? [...now, id].sort((a, b) => a - b) : now.filter((l) => l !== id);
    set(i, { layers: next.length ? next : undefined });
  }

  function offset(i: number, c: Crosshair, axis: 0 | 1, raw: string) {
    const n = Math.round(Number(raw));
    if (!Number.isFinite(n)) return;
    const o: [number, number] = [...(c.offset ?? [0, 0])] as [number, number];
    o[axis] = Math.max(-10000, Math.min(10000, n));
    set(i, { offset: o[0] || o[1] ? o : undefined });
  }
</script>

<div class="section-title">{t('crosshairs.title')}</div>
<p class="hint">{t('crosshairs.hint')}</p>

{#each crosshairs as c, i (i)}
  {@const f = full(c)}
  <section class="xh card">
    <ItemHeader
      index={i}
      name={c.name}
      placeholder={t('crosshairs.name')}
      {disabled}
      removeLabel={t('crosshairs.remove')}
      onrename={(name) => set(i, { name })}
      onremove={() => app.edit((p) => removeCrosshair(p, i))}
    >
      {#snippet preview()}<span class="preview"><CrosshairShape crosshair={f} px={56} /></span>{/snippet}
    </ItemHeader>

    <div class="shapes">
      {#each CROSSHAIR_SHAPES as s (s)}
        <button
          class="shape"
          class:active={f.shape === s}
          title={t(`crosshairs.shapes.${s}`)}
          {disabled}
          onclick={() => set(i, { shape: s as Shape })}
        >
          <CrosshairShape crosshair={{ ...f, shape: s, size: 22, outline: 0, opacity: 1, color: 'currentColor' }} px={26} />
        </button>
      {/each}
    </div>

    <div class="row">
      <label class="color">
        <span>{t('crosshairs.color')}</span>
        <input type="color" value={f.color} {disabled} onchange={(e) => set(i, { color: e.currentTarget.value })} />
      </label>
      <label class="color">
        <span>{t('crosshairs.outline_color')}</span>
        <input type="color" value={f.outline_color} {disabled} onchange={(e) => set(i, { outline_color: e.currentTarget.value })} />
      </label>
      <label class="color">
        <span>{t('crosshairs.offset')}</span>
        <input type="number" value={f.offset[0]} {disabled} onchange={(e) => offset(i, c, 0, e.currentTarget.value)} />
        <input type="number" value={f.offset[1]} aria-label={t('crosshairs.offset_y')} {disabled} onchange={(e) => offset(i, c, 1, e.currentTarget.value)} />
        <span class="unit">px</span>
      </label>
    </div>

    <div class="fields">
      <Slider label={t('crosshairs.size')} value={f.size} def={def?.size} min={10} max={200} step={1} unit="px" {disabled} onchange={(v) => set(i, { size: Math.round(v) })} />
      <Slider label={t('crosshairs.thickness')} value={f.thickness} def={def?.thickness} min={10} max={100} step={5} unit="%" {disabled} onchange={(v) => set(i, { thickness: Math.round(v) })} />
      <Slider label={t('crosshairs.opacity')} value={f.opacity} def={def?.opacity} min={0.1} max={1} step={0.05} scale={100} unit="%" {disabled} onchange={(v) => set(i, { opacity: v })} />
      <Slider label={t('crosshairs.outline')} value={f.outline} def={def?.outline} min={0} max={10} step={1} unit="px" {disabled} onchange={(v) => set(i, { outline: Math.round(v) })} />
    </div>

    <div class="toggles">
      <Toggle checked={f.visible} {disabled} label={t('crosshairs.visible')} onchange={(v) => set(i, { visible: v ? undefined : false })} />
      <Toggle checked={!!c.magnifier} {disabled} label={t('crosshairs.magnifier')} onchange={(v) => set(i, { magnifier: v ? { ...magDef } : undefined })} />
    </div>
    {#if c.magnifier}
      {@const m = c.magnifier}
      <div class="fields">
        <Slider label={t('crosshairs.zoom')} value={m.zoom} def={magDef.zoom} min={1} max={16} step={0.5} unit="×" {disabled} onchange={(v) => set(i, { magnifier: { ...m, zoom: v } })} />
        <Slider label={t('crosshairs.mag_size')} value={m.size} def={magDef.size} min={0.1} max={1} step={0.05} scale={100} unit="%" hint={t('crosshairs.mag_size_hint')} {disabled} onchange={(v) => set(i, { magnifier: { ...m, size: v } })} />
      </div>
    {/if}

    <div class="layers">
      <span class="hl">{t('crosshairs.layers')}</span>
      {#each layers as id (id)}
        <label class="check">
          <input type="checkbox" checked={f.layers.includes(id)} {disabled} onchange={(e) => toggleLayer(i, c, id, e.currentTarget.checked)} />
          <span>{app.layerLabel(id)}</span>
        </label>
      {/each}
    </div>
  </section>
{/each}

{#if !disabled && crosshairs.length < MAX_CROSSHAIRS}
  <button class="btn add" onclick={() => app.edit(addCrosshair)}>
    <Plus size={14} />
    {t('crosshairs.add')}
  </button>
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
  .xh {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px 16px 16px;
  }
  .preview {
    display: grid;
    place-items: center;
    width: 64px;
    height: 64px;
    border-radius: var(--radius-s);
    background: repeating-conic-gradient(var(--line-strong) 0% 25%, var(--bg-3) 0% 50%) 50% / 16px 16px;
    flex: none;
  }
  .shapes {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .shape {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .shape.active {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 20px;
  }
  .color {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .color input[type='color'] {
    width: 34px;
  }
  .color input[type='number'] {
    width: 64px;
  }
  .unit {
    color: var(--text-faint);
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 14px 24px;
  }
  .toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 20px;
  }
  .layers {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }
  .hl {
    color: var(--text-dim);
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .add {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
