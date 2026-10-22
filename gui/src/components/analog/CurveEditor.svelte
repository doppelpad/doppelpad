<script lang="ts">
  // Response graph of a stick: stick deflection → output, with deadzone, outer, curve,
  // anti-deadzone and sensitivity applied like the engine does. The curve's points sit on
  // the graph and can be dragged; double-click adds or removes points.
  import { t } from '../../lib/i18n/index.svelte';
  import {
    CURVE_PRESETS,
    MAX_CURVE_POINTS,
    addCurvePoint,
    curvePreset,
    curveToGraph,
    graphToCurve,
    moveCurvePoint,
    removeCurvePoint,
    stickResponse,
    type CurvePoint,
  } from '../../lib/model/analog';
  import type { StickConfig } from '../../lib/types';

  let {
    cfg,
    live,
    disabled = false,
    onchange,
  }: {
    cfg: Required<StickConfig>;
    /** Current stick deflection, if a controller is connected. */
    live: number | null;
    disabled?: boolean;
    onchange: (points: CurvePoint[]) => void;
  } = $props();

  // Plot area in SVG units.
  const S = 240;
  const L = 30;
  const T = 8;
  const B = 26;
  const R = 10;
  const px = (x: number) => L + x * S;
  const py = (y: number) => T + (1 - y) * S;

  let svg: SVGSVGElement | undefined = $state();
  let dragging = $state<number | null>(null);

  const points = $derived(cfg.curve as CurvePoint[]);
  const outer = $derived(Math.max(cfg.outer, cfg.deadzone + 0.01));
  const preset = $derived(curvePreset(points));

  const line = $derived.by(() => {
    const xs = new Set<number>();
    for (let i = 0; i <= 120; i++) xs.add(i / 120);
    // Exact corners at the deadzone (anti-deadzone jump), the outer edge and the points.
    xs.add(cfg.deadzone);
    xs.add(Math.min(1, cfg.deadzone + 1e-4));
    xs.add(Math.min(1, outer));
    for (const p of points) xs.add(curveToGraph(p, cfg)[0]);
    return [...xs]
      .sort((a, b) => a - b)
      .map((x) => `${px(x).toFixed(2)},${py(stickResponse(x, cfg)).toFixed(2)}`)
      .join(' ');
  });

  const handles = $derived(points.map((p) => curveToGraph(p, cfg)));

  /** The graph point under the pointer (null while the graph is not laid out). */
  function toGraph(e: MouseEvent): CurvePoint | null {
    const ctm = svg?.getScreenCTM();
    if (!ctm) return null;
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
    return [(p.x - L) / S, 1 - (p.y - T) / S];
  }

  function onHandleDown(e: PointerEvent, i: number) {
    if (disabled || e.button !== 0) return;
    e.stopPropagation();
    dragging = i;
    svg?.setPointerCapture(e.pointerId);
  }

  function onMove(e: PointerEvent) {
    const at = dragging === null ? null : toGraph(e);
    if (dragging === null || !at) return;
    onchange(moveCurvePoint(points, dragging, graphToCurve(at, cfg)));
  }

  function onUp() {
    dragging = null;
  }

  function onBackgroundDblClick(e: MouseEvent) {
    const at = toGraph(e);
    if (disabled || points.length >= MAX_CURVE_POINTS || !at) return;
    const [x, y] = at;
    if (x <= cfg.deadzone || x >= outer || y < 0 || y > 1) return;
    onchange(addCurvePoint(points, graphToCurve([x, y], cfg)));
  }

  function onHandleKey(e: KeyboardEvent, i: number) {
    if (disabled) return;
    const [t0, v0] = points[i];
    const d = e.shiftKey ? 0.1 : 0.01;
    const moves: Record<string, CurvePoint> = {
      ArrowLeft: [t0 - d, v0],
      ArrowRight: [t0 + d, v0],
      ArrowUp: [t0, v0 + d],
      ArrowDown: [t0, v0 - d],
    };
    if (moves[e.key]) {
      e.preventDefault();
      onchange(moveCurvePoint(points, i, moves[e.key]));
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      onchange(removeCurvePoint(points, i));
    } else if ((e.key === 'Insert' || e.key === '+') && points.length < MAX_CURVE_POINTS) {
      // Halfway to the next point, or to the curve's end.
      e.preventDefault();
      const [t1, v1] = points[i + 1] ?? [1, 1];
      onchange(addCurvePoint(points, [(t0 + t1) / 2, (v0 + v1) / 2]));
    }
  }

  const GRID = [0.25, 0.5, 0.75];
</script>

<div class="curve">
  <svg
    bind:this={svg}
    role="application"
    aria-label={t('analog.stick.curve')}
    viewBox="0 0 {L + S + R} {T + S + B}"
    class:disabled
    class:dragging={dragging !== null}
    onpointermove={onMove}
    onpointerup={onUp}
    onpointercancel={onUp}
    ondblclick={onBackgroundDblClick}
  >
    <rect class="plot" x={px(0)} y={py(1)} width={S} height={S} />
    {#if cfg.deadzone > 0}
      <rect class="dz" x={px(0)} y={py(1)} width={cfg.deadzone * S} height={S} />
    {/if}
    {#if outer < 1}
      <rect class="sat" x={px(outer)} y={py(1)} width={(1 - outer) * S} height={S} />
    {/if}
    {#each GRID as g (g)}
      <line class="grid" x1={px(g)} y1={py(0)} x2={px(g)} y2={py(1)} />
      <line class="grid" x1={px(0)} y1={py(g)} x2={px(1)} y2={py(g)} />
    {/each}
    <line class="diag" x1={px(0)} y1={py(0)} x2={px(1)} y2={py(1)} />
    <polyline class="resp" points={line} />

    {#if live !== null}
      {@const lx = Math.min(1, live)}
      <line class="live" x1={px(lx)} y1={py(0)} x2={px(lx)} y2={py(1)} />
      <circle class="live-dot" cx={px(lx)} cy={py(stickResponse(lx, cfg))} r="4.5" />
    {/if}

    {#each handles as h, i (i)}
      <circle
        class="handle"
        class:active={dragging === i}
        cx={px(h[0])}
        cy={py(h[1])}
        r="6.5"
        role="button"
        tabindex={disabled ? -1 : 0}
        aria-label={t('analog.stick.point', { n: i + 1 })}
        onpointerdown={(e) => onHandleDown(e, i)}
        ondblclick={(e) => {
          e.stopPropagation();
          if (!disabled) onchange(removeCurvePoint(points, i));
        }}
        onkeydown={(e) => onHandleKey(e, i)}
      />
    {/each}

    <text class="tick" x={px(0) - 6} y={py(0) + 4} text-anchor="end">0</text>
    <text class="tick" x={px(0) - 6} y={py(1) + 4} text-anchor="end">1</text>
    <text class="tick" x={px(1)} y={py(0) + 15} text-anchor="middle">1</text>
    <text class="axis" x={px(0.5)} y={py(0) + 18} text-anchor="middle">{t('analog.stick.axis_in')} →</text>
    <text class="axis" transform="translate({px(0) - 10} {py(0.5)}) rotate(-90)" text-anchor="middle">
      {t('analog.stick.axis_out')} →
    </text>
  </svg>

  <div class="presets">
    {#each CURVE_PRESETS as p (p.id)}
      <button class="chip" class:active={preset === p.id} {disabled} onclick={() => onchange(p.points)}>
        {t(`analog.stick.presets.${p.id}`)}
      </button>
    {/each}
    {#if !preset}<span class="chip custom active">{t('analog.stick.custom')}</span>{/if}
  </div>
  {#if !disabled}<p class="hint">{t('analog.stick.curve_hint')}</p>{/if}
</div>

<style>
  .curve {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  svg {
    width: 100%;
    max-width: 320px;
    touch-action: none;
  }
  .plot {
    fill: var(--bg-0);
    stroke: var(--line-strong);
    stroke-width: 1;
  }
  .dz {
    fill: var(--text-faint);
    fill-opacity: 0.16;
  }
  .sat {
    fill: var(--accent);
    fill-opacity: 0.08;
  }
  .grid {
    stroke: var(--line);
    stroke-width: 0.75;
  }
  .diag {
    stroke: var(--line-strong);
    stroke-width: 1;
    stroke-dasharray: 3 4;
  }
  .resp {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2.25;
    stroke-linejoin: round;
  }
  .live {
    stroke: var(--text-dim);
    stroke-width: 1;
    stroke-dasharray: 2 3;
  }
  .live-dot {
    fill: var(--text);
    stroke: var(--bg-0);
    stroke-width: 1.5;
  }
  .handle {
    fill: var(--bg-1);
    stroke: var(--accent);
    stroke-width: 2.5;
    cursor: grab;
    outline: none;
  }
  .handle:hover,
  .handle:focus-visible,
  .handle.active {
    fill: var(--accent);
  }
  .dragging,
  .dragging .handle {
    cursor: grabbing;
  }
  .disabled .handle {
    cursor: default;
  }
  .tick,
  .axis {
    fill: var(--text-faint);
    font-size: 10.5px;
  }
  .axis {
    fill: var(--text-dim);
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .chip {
    padding: 4px 10px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--line);
    background: var(--bg-2);
    font-size: var(--fs-s);
    color: var(--text-dim);
  }
  .chip:hover:not(:disabled):not(.custom) {
    border-color: var(--line-strong);
    color: var(--text);
  }
  .chip.active {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .chip.custom {
    cursor: default;
  }
  .hint {
    margin: 0;
    color: var(--text-faint);
    font-size: 11.5px;
  }
</style>
