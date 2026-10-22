<script lang="ts">
  import { Gamepad2, RotateCcw, Keyboard, Mouse, Combine, Cable, Joystick, Plus, Ellipsis, ChevronDown } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { layerColor } from '../lib/theme/layers';
  import { insertPreset, makePreset } from '../lib/model/presets';
  import { addLayer, clearLayer, copyLayer, layerIds, pasteLayer, removeLayer, renameLayer } from '../lib/model/profile';
  import { app, type View } from '../lib/state/app.svelte';
  import type { PresetEntry, Profile } from '../lib/types';
  import LayerDialog from './LayerDialog.svelte';
  import Menu from './ui/Menu.svelte';

  const views: { id: View; icon: typeof Gamepad2; label: string }[] = $derived([
    { id: 'front', icon: Gamepad2, label: t('views.front') },
    { id: 'back', icon: RotateCcw, label: t('views.back') },
    { id: 'keyboard', icon: Keyboard, label: t('views.keyboard') },
    { id: 'mouse', icon: Mouse, label: t('views.mouse') },
  ]);

  const layers = $derived(app.profile ? layerIds(app.profile) : [0]);

  // Chip widths come from an invisible copy of every chip, so the strip knows what fits before showing it.
  const GAP = 4;
  const BAR_GAP = 14; // column gap of .toolbar
  let barRect: DOMRectReadOnly | undefined = $state();
  let viewsWidth = $state(0);
  let extraWidth = $state(0);
  let stripWidth = $state(0);
  let toolsWidth = $state(0);
  let moreWidth = $state(0);
  let chipWidths: Record<number, number> = $state({});

  const chipsWidth = (ids: number[]) => ids.reduce((s, id) => s + (chipWidths[id] ?? 0) + GAP, 0);
  /** Main and the current layer always get a chip. */
  const pinned = $derived(layers.filter((id) => id === 0 || id === app.layer));
  /** Narrowest the strip gets: every chip, or the pinned ones plus "+N", and the tools. */
  const minStrip = $derived(Math.min(chipsWidth(layers), chipsWidth(pinned) + moreWidth + GAP) + toolsWidth);
  /** Too narrow for views, layers and extras in one row, so the layers get a row of their own. */
  const stacked = $derived((barRect?.width ?? 0) - viewsWidth - extraWidth - 2 * BAR_GAP < minStrip);

  /** Layers shown as chips: all if they fit, else the pinned ones plus the others in order while they fit. */
  const shown = $derived.by(() => {
    if (chipsWidth(layers) + toolsWidth <= stripWidth) return layers;
    const keep = new Set(pinned);
    let used = chipsWidth(pinned) + moreWidth + GAP + toolsWidth;
    for (const id of layers) {
      if (keep.has(id)) continue;
      used += (chipWidths[id] ?? 0) + GAP;
      if (used > stripWidth) break;
      keep.add(id);
    }
    return layers.filter((id) => keep.has(id));
  });
  const overflow = $derived(layers.filter((id) => !shown.includes(id)));
  const overflowMenu = $derived(
    overflow.map((id) => ({ label: app.layerLabel(id), color: layerColor(id), onclick: () => (app.layer = id) })),
  );

  function setView(v: View) {
    app.view = v;
    app.select(null);
  }

  async function onAddLayer() {
    if (!app.profile) return;
    const { profile, id } = addLayer(app.profile);
    app.update(profile);
    app.layer = id;
  }

  async function onRename() {
    if (!app.profile || app.layer === 0) return;
    const name = await app.ask(t('layers.name_prompt'), app.layerLabel(app.layer));
    if (name) app.edit((p) => renameLayer(p, app.layer, name));
  }

  function onCopy() {
    if (!app.profile) return;
    app.copiedLayer = copyLayer($state.snapshot(app.profile) as Profile, app.layer);
  }

  function onPaste() {
    const copied = app.copiedLayer;
    if (copied) app.edit((p) => pasteLayer(p, copied, app.layer));
  }

  let presets = $state<PresetEntry[]>([]);
  const loadPresets = () =>
    api
      .presetList()
      .then((p) => (presets = p))
      .catch(() => {});
  loadPresets();

  async function onSavePreset() {
    if (!app.profile) return;
    const name = await app.ask(t('layers.preset_name'), app.layerLabel(app.layer));
    if (!name) return;
    const preset = makePreset($state.snapshot(app.profile) as Profile, app.layer, name);
    await app.attempt(api.presetSave(preset).then(loadPresets), t('layers.preset_saved', { name }));
  }

  async function onInsertPreset(entry: PresetEntry) {
    if (!app.profile) return;
    await app.attempt(
      api.presetLoad(entry.path).then((preset) => {
        if (!app.profile) return;
        const { profile, layer, dropped } = insertPreset(app.profile, preset);
        app.update(profile);
        app.layer = layer;
        if (dropped > 0) app.toast(t('layers.preset_dropped', { n: dropped }), 'info');
      }),
    );
  }

  function onRemove() {
    if (!app.profile || app.layer === 0) return;
    app.edit((p) => removeLayer(p, app.layer));
    app.layer = 0;
  }

  let layerDialog = $state<number | null>(null);

  const layerMenu = $derived([
    { label: t('layers.settings') + '…', onclick: () => (layerDialog = app.layer), disabled: app.layer === 0 },
    { label: t('layers.rename') + '…', onclick: onRename, disabled: app.layer === 0 },
    { label: t('layers.copy'), onclick: onCopy },
    { label: t('layers.paste'), onclick: onPaste, disabled: !app.copiedLayer },
    { label: t('layers.clear'), onclick: () => app.edit((p) => clearLayer(p, app.layer)) },
    { separator: true, label: '' },
    { label: t('layers.save_preset') + '…', onclick: onSavePreset, disabled: !app.profile },
    ...presets.map((p) => ({
      label: t('layers.insert_preset', { name: p.name }),
      onclick: () => onInsertPreset(p),
      disabled: !app.profile || app.readOnly,
    })),
    { separator: true, label: '' },
    { label: t('layers.remove'), danger: true, onclick: onRemove, disabled: app.layer === 0 },
  ]);
</script>

<div class="toolbar" class:stacked bind:contentRect={barRect}>
  <div class="views" bind:offsetWidth={viewsWidth}>
    {#each views as v (v.id)}
      <button class="tile" class:active={app.view === v.id} title={v.label} onclick={() => setView(v.id)}>
        <v.icon size={18} />
        <span>{v.label}</span>
      </button>
    {/each}
  </div>

  <div class="layers" bind:clientWidth={stripWidth}>
    {#each shown as id (id)}
      <button
        class="layer"
        class:active={app.layer === id}
        style:--c={layerColor(id)}
        title={app.layerLabel(id)}
        onclick={() => {
          app.layer = id;
        }}
      >
        <span class="dot"></span><span class="name">{app.layerLabel(id)}</span>
      </button>
    {/each}
    {#if overflow.length}
      <Menu items={overflowMenu} title={t('layers.more', { n: overflow.length })}>
        {#snippet trigger()}<span class="layer more">+{overflow.length}<ChevronDown size={14} /></span>{/snippet}
      </Menu>
    {/if}
    <div class="tools" bind:offsetWidth={toolsWidth}>
      <button class="icon-btn" title={t('layers.add')} onclick={onAddLayer} disabled={!app.profile || app.readOnly}>
        <Plus size={16} />
      </button>
      <Menu items={layerMenu}>
        {#snippet trigger()}<span class="icon-btn"><Ellipsis size={16} /></span>{/snippet}
      </Menu>
    </div>

    <div class="measure" aria-hidden="true">
      {#each layers as id (id)}
        <span class="layer" bind:offsetWidth={chipWidths[id]}>
          <span class="dot"></span><span class="name">{app.layerLabel(id)}</span>
        </span>
      {/each}
      <span class="layer more" bind:offsetWidth={moreWidth}>+{layers.length}<ChevronDown size={14} /></span>
    </div>
  </div>

  <div class="extra" bind:offsetWidth={extraWidth}>
    <button class="tile" class:active={app.view === 'analog'} title={t('analog.hint')} onclick={() => setView('analog')}>
      <Joystick size={18} /><span>{t('views.analog')}</span>
    </button>
    <button class="tile" class:active={app.view === 'shortcuts'} onclick={() => setView('shortcuts')}>
      <Combine size={18} /><span>{t('views.shortcuts')}</span>
    </button>
    <button class="tile" class:active={app.view === 'output'} onclick={() => setView('output')}>
      <Cable size={18} /><span>{t('views.output')}</span>
    </button>
  </div>
</div>

{#if layerDialog !== null}
  <LayerDialog layer={layerDialog} onclose={() => (layerDialog = null)} />
{/if}

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px 14px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .views,
  .extra {
    display: flex;
    gap: 4px;
  }
  .extra {
    margin-left: auto;
  }
  .tile {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
    border: 1px solid transparent;
  }
  .tile span {
    font-size: var(--fs-s);
  }
  .tile:hover {
    background: var(--bg-2);
    color: var(--text);
  }
  .tile.active {
    color: var(--accent);
    background: var(--accent-soft);
    border-color: rgba(20, 184, 166, 0.3);
  }
  .layers {
    position: relative;
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 4px;
    justify-content: safe center;
  }
  .stacked .layers {
    order: 1;
    flex-basis: 100%;
  }
  .tools {
    flex: none;
    display: flex;
    gap: 4px;
  }
  .measure {
    position: absolute;
    top: 0;
    left: 0;
    width: 0;
    height: 0;
    overflow: hidden;
    display: flex;
    visibility: hidden;
    pointer-events: none;
  }
  .layer {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--line);
    color: var(--text-dim);
    font-size: var(--fs-s);
    white-space: nowrap;
  }
  .layer.more {
    gap: 2px;
    padding: 5px 8px 5px 12px;
  }
  .name {
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .layer .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
  }
  .layer:hover {
    border-color: var(--line-strong);
    color: var(--text);
  }
  .layer.active {
    border-color: var(--c);
    color: var(--text);
    background: color-mix(in srgb, var(--c) 14%, transparent);
  }
</style>
