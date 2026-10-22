<script lang="ts">
  // Side panel for a whole stick: what it does on the current layer (mouse, WASD, arrow keys,
  // flick stick) and, for a route, its settings; a mouse route also gets response presets
  // and a graph with the live stick deflection.
  import { t } from '../../lib/i18n/index.svelte';
  import { type Side } from '../../lib/model/analog';
  import { MOUSE_PRESETS, mousePresetOf, mouseResponse, updateRoute, type MousePreset } from '../../lib/model/routes';
  import { STICK_GROUPS, stickGroupOf, stickRouteIndex } from '../../lib/model/stickgroups';
  import { SIDES } from '../../lib/model/controls';
  import { stickAxes } from '../../lib/model/devices';
  import { app } from '../../lib/state/app.svelte';
  import RouteCard from '../analog/RouteCard.svelte';
  import PanelHeader from './PanelHeader.svelte';
  import Segmented from '../ui/Segmented.svelte';
  import { routeConfig } from '../../lib/model/routes';

  let { side }: { side: Side } = $props();

  const profile = $derived(app.profile);
  const group = $derived(profile ? stickGroupOf(profile, app.layer, side) : null);
  const index = $derived(profile ? stickRouteIndex(profile, app.layer, side) : -1);
  const route = $derived(index >= 0 ? profile?.analog?.[index] : undefined);
  /** Main's route from this stick, which a shift layer without its own keeps using. */
  const inherited = $derived(!!profile && app.layer !== 0 && index < 0 && stickRouteIndex(profile, 0, side) >= 0);
  const def = $derived(app.catalog?.defaults.analog);
  const r = $derived(route && def ? routeConfig(route, def) : null);
  const preset = $derived(r ? mousePresetOf(r) : null);
  const PRESETS = Object.keys(MOUSE_PRESETS) as MousePreset[];

  /** The graph: cursor speed (0..1) over stick deflection (0..1), 60 samples. */
  const W = 200;
  const H = 90;
  const curve = $derived(
    r
      ? Array.from({ length: 61 }, (_, i) => {
          const x = i / 60;
          return `${(x * W).toFixed(1)},${(H - mouseResponse(x, r) * H).toFixed(1)}`;
        }).join(' ')
      : '',
  );
  /** The stick's deflection right now, for the dot on the graph. */
  const deflection = $derived.by(() => {
    if (!app.primary) return null;
    const [x, y] = stickAxes(app.primary, side);
    return Math.min(Math.hypot(x, y), 1);
  });

  function applyPreset(preset: MousePreset) {
    if (index >= 0) app.edit((p) => updateRoute(p, index, MOUSE_PRESETS[preset]));
  }
</script>

<div class="panel">
  <PanelHeader title={t(`analog.routes.sources.${SIDES[side].source}`)} layer={app.layer} onclose={() => app.select(null)} />

  <div class="section-title">{t('binding.whole_stick')}</div>
  <Segmented
    variant="fill"
    label={t('binding.whole_stick')}
    options={STICK_GROUPS.map((g) => ({ value: g, label: t(`binding.stick_groups.${g}`) }))}
    value={group}
    disabled={app.readOnly}
    onchange={(g) => g && app.setStickGroup(side, g)}
  />

  {#if group === 'wasd' || group === 'arrows'}
    <p class="muted">{t(`binding.whole_stick_keys.${group}`)}</p>
  {:else if !route}
    <p class="muted">{t(inherited ? 'binding.whole_stick_inherited' : 'binding.whole_stick_none')}</p>
  {/if}

  {#if route && r && group === 'mouse'}
    <div class="section-title">{t('binding.response')}</div>
    <Segmented
      variant="fill"
      label={t('binding.response')}
      options={PRESETS.map((p) => ({ value: p, label: t(`binding.presets.${p}`), title: t(`binding.presets.${p}_hint`) }))}
      value={preset}
      disabled={app.readOnly}
      onchange={(p) => p && applyPreset(p)}
    />
    <figure class="graph">
      <div class="plot">
        <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label={t('binding.response_graph')}>
          <rect class="dz" x="0" y="0" width={r.deadzone * W} height={H} />
          <rect class="full" x={r.outer * W} y="0" width={(1 - r.outer) * W} height={H} />
          <line class="grid" x1="0" y1={H / 2} x2={W} y2={H / 2} />
          <line class="grid" x1={W / 2} y1="0" x2={W / 2} y2={H} />
          <polyline points={curve} />
        </svg>
        <span class="ylabel">↑ {t('binding.graph_y')}</span>
        {#if deflection !== null}
          <span class="dot" style:left="{deflection * 100}%" style:top="{(1 - mouseResponse(deflection, r)) * 100}%"></span>
        {/if}
      </div>
      <figcaption>
        <span>{t('binding.graph_x')}</span>
        {#if !preset}<span>{t('binding.presets.custom')}</span>{/if}
      </figcaption>
    </figure>
  {/if}

  {#if route}
    <RouteCard {index} {route} fixedSource />
  {/if}

  {#if (group || route) && !app.readOnly}
    <button class="clear" onclick={() => app.setStickGroup(side, null)}>{t('binding.whole_stick_clear')}</button>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 14px 20px;
  }
  .section-title {
    margin-top: 4px;
  }
  .muted {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .graph {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .plot {
    position: relative;
    height: 96px;
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    overflow: hidden;
  }
  .plot svg {
    display: block;
    width: 100%;
    height: 100%;
  }
  .ylabel {
    position: absolute;
    top: 4px;
    left: 6px;
    font-size: var(--fs-xs);
    color: var(--text-faint);
    pointer-events: none;
  }
  .dot {
    position: absolute;
    width: 7px;
    height: 7px;
    margin: -3.5px 0 0 -3.5px;
    border-radius: 50%;
    background: var(--text);
  }
  .graph .dz,
  .graph .full {
    fill: var(--bg-2);
  }
  .graph .grid {
    stroke: var(--line);
    stroke-dasharray: 2 3;
    vector-effect: non-scaling-stroke;
  }
  .graph polyline {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
  figcaption {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-xs);
    color: var(--text-faint);
  }
  .clear {
    align-self: flex-start;
    font-size: var(--fs-s);
    color: var(--text-dim);
  }
  .clear:hover {
    color: var(--danger);
  }
</style>
