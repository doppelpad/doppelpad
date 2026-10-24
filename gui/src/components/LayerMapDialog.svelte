<script lang="ts">
  // The layer map: Main and the shift layers on a circle, an arrow for each mapping that changes
  // layer, labelled with its control, activator and kind. Layers nothing changes to and arrows
  // with a warning (a switch nothing leaves again) are marked. Clicking a layer shows it;
  // clicking an arrow selects the mapping.
  import { t } from '../lib/i18n/index.svelte';
  import { comboLabel } from '../lib/model/labels';
  import { layerGraph, type MapEdge } from '../lib/model/layers';
  import { comboKey, layerIds } from '../lib/model/profile';
  import { app } from '../lib/state/app.svelte';
  import { layerColor } from '../lib/theme/layers';
  import { tip } from '../lib/tooltip.svelte';
  import Dialog from './ui/Dialog.svelte';

  let { onclose }: { onclose: () => void } = $props();

  /** Node radius and the space around the circle, in the map's units (the circle has radius 1). */
  const R = 0.2;
  const graph = $derived(app.profile ? layerGraph(app.profile, layerIds(app.profile)) : { nodes: [], edges: [] });
  const at = $derived(new Map(graph.nodes.map((n) => [n.id, n])));

  const MODE_MARK = { hold: '⇣', toggle: '⇄', switch: '→' } as const;

  /** The arrow's path (a gentle curve between the two circles' edges) and where its label sits. */
  function arrow(e: MapEdge): { d: string; lx: number; ly: number } {
    const a = at.get(e.from)!;
    const b = at.get(e.to)!;
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const len = Math.hypot(dx, dy) || 1;
    const [ux, uy] = [dx / len, dy / len];
    // Control point: the middle, pushed sideways by the bend.
    const cx = (a.x + b.x) / 2 - uy * e.bend * len;
    const cy = (a.y + b.y) / 2 + ux * e.bend * len;
    const start = toward(a.x, a.y, cx, cy, R);
    const end = toward(b.x, b.y, cx, cy, R + 0.04);
    return {
      d: `M ${start[0]} ${start[1]} Q ${cx} ${cy} ${end[0]} ${end[1]}`,
      lx: 0.25 * start[0] + 0.5 * cx + 0.25 * end[0],
      ly: 0.25 * start[1] + 0.5 * cy + 0.25 * end[1],
    };
  }

  /** The point `r` away from (x, y) toward (tx, ty). */
  function toward(x: number, y: number, tx: number, ty: number, r: number): [number, number] {
    const l = Math.hypot(tx - x, ty - y) || 1;
    return [x + ((tx - x) / l) * r, y + ((ty - y) / l) * r];
  }

  const flaggedEdge = (e: MapEdge) => app.flags.triggers.get(e.from)?.has(comboKey(e.trigger)) ?? false;

  function showLayer(id: number) {
    app.layer = id;
    onclose();
  }

  function showMapping(e: MapEdge) {
    if (app.view !== 'front' && app.view !== 'back' && app.view !== 'shortcuts') app.view = 'front';
    app.layer = e.from;
    app.activator = e.activator;
    app.select(e.trigger);
    onclose();
  }
</script>

<Dialog title={t('layer_map.title')} {onclose} width={640}>
  {#if graph.nodes.length < 2}
    <p class="hint">{t('layer_map.empty')}</p>
  {:else}
    <svg viewBox="-1.55 -1.35 3.1 2.75" role="img" aria-label={t('layer_map.title')}>
      <defs>
        <marker id="layer-map-arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
          <path d="M 0 0 L 10 5 L 0 10 z" class="head" />
        </marker>
      </defs>
      {#each graph.edges as e (e.index)}
        {@const a = arrow(e)}
        <g class="edge" class:warn={flaggedEdge(e)}>
          <path d={a.d} marker-end="url(#layer-map-arrow)" />
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <text
            x={a.lx}
            y={a.ly}
            role="button"
            tabindex="0"
            use:tip={`${t(`binding.jump_${e.mode}`)}${e.delayMs ? ` · ${e.delayMs} ms` : ''}`}
            onclick={() => showMapping(e)}
            onkeydown={(ev) => (ev.key === 'Enter' || ev.key === ' ') && showMapping(e)}
          >
            {MODE_MARK[e.mode]}
            {comboLabel(e.trigger, app.family)}{e.activator !== 'single' ? ` · ${t(`activators.${e.activator}`)}` : ''}
          </text>
        </g>
      {/each}
      {#each graph.nodes as n (n.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <g
          class="node"
          class:current={n.id === app.layer}
          class:warn={app.flags.layers.has(n.id)}
          style:--c={layerColor(n.id)}
          role="button"
          tabindex="0"
          onclick={() => showLayer(n.id)}
          onkeydown={(ev) => (ev.key === 'Enter' || ev.key === ' ') && showLayer(n.id)}
        >
          <circle cx={n.x} cy={n.y} r={R} />
          <text x={n.x} y={n.y}>{app.layerLabel(n.id)}</text>
        </g>
      {/each}
    </svg>
    <p class="hint">{t('layer_map.hint')}</p>
  {/if}
</Dialog>

<style>
  svg {
    width: 100%;
    height: auto;
    display: block;
  }
  .edge path {
    fill: none;
    stroke: var(--line-strong);
    stroke-width: 0.012;
  }
  .edge.warn path {
    stroke: var(--warning);
    stroke-dasharray: 0.04 0.03;
  }
  .head {
    fill: var(--text-dim);
  }
  .edge text {
    font-size: 0.075px;
    fill: var(--text-dim);
    text-anchor: middle;
    dominant-baseline: middle;
    cursor: pointer;
    paint-order: stroke;
    stroke: var(--bg-1);
    stroke-width: 0.03px;
  }
  .edge text:hover,
  .edge text:focus-visible {
    fill: var(--text);
    outline: none;
  }
  .edge.warn text {
    fill: var(--warning);
  }
  .node {
    cursor: pointer;
  }
  .node circle {
    fill: color-mix(in srgb, var(--c) 18%, var(--bg-1));
    stroke: var(--c);
    stroke-width: 0.012;
  }
  .node.current circle {
    stroke-width: 0.03;
  }
  .node.warn circle {
    stroke: var(--warning);
    stroke-dasharray: 0.04 0.03;
  }
  .node:focus-visible {
    outline: none;
  }
  .node:focus-visible circle,
  .node:hover circle {
    fill: color-mix(in srgb, var(--c) 32%, var(--bg-1));
  }
  .node text {
    font-size: 0.07px;
    fill: var(--text);
    text-anchor: middle;
    dominant-baseline: middle;
    pointer-events: none;
  }
</style>
