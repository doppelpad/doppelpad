<script lang="ts">
  import { Gamepad2, RotateCcw, Keyboard, Mouse, Combine, Cable, Joystick, Plus, Ellipsis, ChevronDown, ChartPie } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { layerColor } from '../lib/theme/layers';
  import { insertPreset, makePreset } from '../lib/model/presets';
  import { addLayer, clearLayer, copyLayer, layerIds, pasteLayer, removeLayer, renameLayer } from '../lib/model/profile';
  import { TOOLBAR_VIEWS, VIEWS, type ToolbarView } from '../lib/model/views';
  import { app } from '../lib/state/app.svelte';
  import type { PresetEntry } from '../lib/types';
  import LayerDialog from './LayerDialog.svelte';
  import LayerTip from './LayerTip.svelte';
  import TransformDialog from './TransformDialog.svelte';
  import LayerMapDialog from './LayerMapDialog.svelte';
  import type { MenuItem } from '../lib/menu';
  import Menu from './ui/Menu.svelte';
  import { tip as tooltip } from '../lib/tooltip.svelte';

  const ICONS: Record<ToolbarView, typeof Gamepad2> = {
    front: Gamepad2,
    back: RotateCcw,
    keyboard: Keyboard,
    mouse: Mouse,
    analog: Joystick,
    shortcuts: Combine,
    radial: ChartPie,
    output: Cable,
  };
  /** Tooltips that say more than the label. */
  const TITLES: Partial<Record<ToolbarView, string>> = { analog: 'analog.hint', radial: 'radial.tile_hint' };

  type Tile = { id: ToolbarView; icon: typeof Gamepad2; label: string; title?: string };
  const tileOf = (id: ToolbarView): Tile => ({ id, icon: ICONS[id], label: t(`views.${id}`), title: TITLES[id] && t(TITLES[id]) });
  /** Tiles before the layers, and after them. */
  const views = $derived(TOOLBAR_VIEWS.filter((v) => VIEWS[v].tile === 'before').map(tileOf));
  const extras = $derived(TOOLBAR_VIEWS.filter((v) => VIEWS[v].tile === 'after').map(tileOf));

  const layers = $derived(app.profile ? layerIds(app.profile) : [0]);
  /** The layout transforms dialog is open. */
  let transformOpen = $state(false);
  /** The layer map is open. */
  let mapOpen = $state(false);

  /** The layer chip whose tooltip shows, and where (below the chip). */
  let tip = $state<{ layer: number; x: number; y: number } | null>(null);
  let tipTimer: ReturnType<typeof setTimeout> | undefined;
  const tipId = $props.id();

  function showTip(layer: number, chip: HTMLElement, delay: number) {
    clearTimeout(tipTimer);
    tipTimer = setTimeout(() => {
      const r = chip.getBoundingClientRect();
      tip = { layer, x: Math.max(4, Math.min(r.left, window.innerWidth - 344)), y: r.bottom + 6 };
    }, delay);
  }

  function hideTip() {
    clearTimeout(tipTimer);
    tip = null;
  }

  // Chip widths come from an invisible copy of every chip, so the strip knows what fits before showing it.
  const GAP = 4;
  const BAR_GAP = 14; // column gap of .toolbar
  let barRect: DOMRectReadOnly | undefined = $state();
  let viewsWidth = $state(0);
  /** The extras' width with their labels and as icons only (from invisible copies). */
  let extraFull = $state(0);
  let extraIcons = $state(0);
  let stripWidth = $state(0);
  let toolsWidth = $state(0);
  let moreWidth = $state(0);
  let chipWidths: Record<number, number> = $state({});

  const chipsWidth = (ids: number[]) => ids.reduce((s, id) => s + (chipWidths[id] ?? 0) + GAP, 0);
  /** Main and the current layer always get a chip. */
  const pinned = $derived(layers.filter((id) => id === 0 || id === app.layer));
  /** Narrowest the strip gets: every chip, or the pinned ones plus "+N", and the tools. */
  const minStrip = $derived(Math.min(chipsWidth(layers), chipsWidth(pinned) + moreWidth + GAP) + toolsWidth);
  /** The extras show only their icons when, with their labels, not every layer would get a chip. */
  const compact = $derived((barRect?.width ?? 0) - viewsWidth - extraFull - 2 * BAR_GAP < chipsWidth(layers) + toolsWidth);
  const extraWidth = $derived(compact ? extraIcons : extraFull);
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

  function setView(v: ToolbarView) {
    app.view = v;
    app.select(null);
  }

  function onAddLayer() {
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
    app.copiedLayer = copyLayer(app.snapshot()!, app.layer);
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
    const preset = makePreset(app.snapshot()!, app.layer, name);
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

  const layerMenu = $derived<MenuItem[]>([
    { label: t('layers.settings') + '…', onclick: () => (layerDialog = app.layer), disabled: app.layer === 0 },
    { label: t('layers.rename') + '…', onclick: onRename, disabled: app.layer === 0 },
    { label: t('layers.copy'), onclick: onCopy },
    { label: t('layers.paste'), onclick: onPaste, disabled: !app.copiedLayer },
    { label: t('layers.clear'), onclick: () => app.edit((p) => clearLayer(p, app.layer)) },
    { label: t('transforms.menu') + '…', onclick: () => (transformOpen = true), disabled: !app.profile || app.readOnly },
    { label: t('layer_map.title') + '…', onclick: () => (mapOpen = true), disabled: !app.profile },
    { separator: true },
    { label: t('layers.save_preset') + '…', onclick: onSavePreset, disabled: !app.profile },
    ...presets.map((p) => ({
      label: t('layers.insert_preset', { name: p.name }),
      onclick: () => onInsertPreset(p),
      disabled: !app.profile || app.readOnly,
    })),
    { separator: true },
    { label: t('layers.remove'), danger: true, onclick: onRemove, disabled: app.layer === 0 },
  ]);
</script>

{#snippet tile(v: Tile, iconOnly = false)}
  <button
    class="tile"
    class:active={app.view === v.id}
    use:tooltip={iconOnly ? (v.title ?? v.label) : v.title}
    onclick={() => setView(v.id)}
  >
    <v.icon size={18} />
    {#if !iconOnly}<span>{v.label}</span>{/if}
  </button>
{/snippet}

<div class="toolbar" class:stacked bind:contentRect={barRect}>
  <div class="views" bind:offsetWidth={viewsWidth}>
    {#each views as v (v.id)}{@render tile(v)}{/each}
  </div>

  <div class="layers" bind:clientWidth={stripWidth}>
    {#each shown as id (id)}
      <button
        class="layer"
        class:active={app.layer === id}
        class:flagged={app.flags.layers.has(id)}
        style:--c={layerColor(id)}
        aria-describedby={tip?.layer === id ? tipId : undefined}
        onmouseenter={(e) => showTip(id, e.currentTarget, 400)}
        onmouseleave={hideTip}
        onfocus={(e) => showTip(id, e.currentTarget, 0)}
        onblur={hideTip}
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
      <button class="icon-btn" use:tooltip={t('layers.add')} onclick={onAddLayer} disabled={!app.profile || app.readOnly}>
        <Plus size={16} />
      </button>
      <Menu items={layerMenu} title={t('layers.options')} iconOnly>
        {#snippet trigger()}<span class="icon-btn"><Ellipsis size={16} /></span>{/snippet}
      </Menu>
    </div>

    <div class="measure" aria-hidden="true">
      {#each layers as id (id)}
        <span class="layer" class:flagged={app.flags.layers.has(id)} bind:offsetWidth={chipWidths[id]}>
          <span class="dot"></span><span class="name">{app.layerLabel(id)}</span>
        </span>
      {/each}
      <span class="layer more" bind:offsetWidth={moreWidth}>+{layers.length}<ChevronDown size={14} /></span>
    </div>
  </div>

  <div class="extra">
    {#each extras as v (v.id)}{@render tile(v, compact)}{/each}
  </div>

  <div class="measure" aria-hidden="true">
    <span class="extra" bind:offsetWidth={extraFull}>
      {#each extras as v (v.id)}<span class="tile"><v.icon size={18} /><span>{v.label}</span></span>{/each}
    </span>
    <span class="extra" bind:offsetWidth={extraIcons}>
      {#each extras as v (v.id)}<span class="tile"><v.icon size={18} /></span>{/each}
    </span>
  </div>
</div>

{#if tip}<LayerTip layer={tip.layer} x={tip.x} y={tip.y} id={tipId} />{/if}

{#if transformOpen}<TransformDialog onclose={() => (transformOpen = false)} />{/if}
{#if mapOpen}<LayerMapDialog onclose={() => (mapOpen = false)} />{/if}

{#if layerDialog !== null}
  <LayerDialog layer={layerDialog} onclose={() => (layerDialog = null)} />
{/if}

<style>
  .toolbar {
    position: relative;
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
    white-space: nowrap;
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
    border-color: color-mix(in srgb, var(--accent) 30%, transparent);
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
  .measure .extra,
  .measure .tile {
    flex: none;
    margin: 0;
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
  /* A warning about the layer (nothing changes to it; the diagnostics bar says so). */
  .layer.flagged::after {
    content: '';
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--warning);
  }
</style>
