<script lang="ts">
  // Top view of a stick: deadzone, full-deflection ring and the zone thresholds (the low one
  // is where direction controls start), with the live stick position and what the engine
  // outputs for it.
  import { t } from '../../lib/i18n/index.svelte';
  import { orient, shapeStick } from '../../lib/model/analog';
  import type { Triple } from '../../lib/model/zones';
  import type { StickConfig } from '../../lib/types';

  let {
    cfg,
    raw,
    zones,
  }: {
    cfg: Required<StickConfig>;
    raw: [number, number] | null;
    /** Effective low, medium and high thresholds per axis. */
    zones: { x: Triple; y: Triple };
  } = $props();

  const outer = $derived(Math.max(cfg.outer, cfg.deadzone + 0.01));
  const out = $derived(raw ? shapeStick(orient(raw, cfg), cfg) : null);
  /** Where pushing the stick up ends up after inversion and rotation. */
  const up = $derived(orient([0, 1], cfg));
  const turned = $derived(Math.abs(up[0]) > 1e-3 || up[1] < 0.999);

  // Stick coordinates are y-up; SVG is y-down.
  const sx = (x: number) => x;
  const sy = (y: number) => -y;
</script>

<div class="ring">
  <svg viewBox="-1.12 -1.12 2.24 2.24" role="img" aria-label={t('analog.legend_stick')}>
    <circle class="bound" r="1" />
    <path
      class="saturation"
      d="M -1 0 A 1 1 0 1 0 1 0 A 1 1 0 1 0 -1 0 Z M {-outer} 0 A {outer} {outer} 0 1 1 {outer} 0 A {outer} {outer} 0 1 1 {-outer} 0 Z"
    />
    <circle class="outer" r={outer} />
    <line class="axis" x1="-1" y1="0" x2="1" y2="0" />
    <line class="axis" x1="0" y1="-1" x2="0" y2="1" />
    {#each [2, 1] as i (i)}
      <ellipse class="zone" rx={zones.x[i]} ry={zones.y[i]} />
    {/each}
    <ellipse class="threshold" rx={zones.x[0]} ry={zones.y[0]} />
    {#if cfg.deadzone > 0}<circle class="deadzone" r={cfg.deadzone} />{/if}
    {#if turned}
      <line class="up" x1="0" y1="0" x2={sx(up[0] * 1.04)} y2={sy(up[1] * 1.04)} />
      <circle class="up-dot" cx={sx(up[0] * 1.04)} cy={sy(up[1] * 1.04)} r="0.035" />
    {/if}
    {#if raw && out}
      <line class="out-line" x1="0" y1="0" x2={sx(out[0])} y2={sy(out[1])} />
      <circle class="raw" cx={sx(raw[0])} cy={sy(raw[1])} r="0.055" />
      <circle class="out" cx={sx(out[0])} cy={sy(out[1])} r="0.06" />
    {/if}
  </svg>
  <div class="legend">
    <span><i class="k raw"></i>{t('analog.legend_stick')}</span>
    <span><i class="k out"></i>{t('analog.legend_output')}</span>
    <span><i class="k threshold"></i>{t('analog.legend_directions')}</span>
  </div>
</div>

<style>
  .ring {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  svg {
    width: 100%;
    max-width: 200px;
    aspect-ratio: 1;
    overflow: visible;
  }
  .bound {
    fill: var(--bg-0);
    stroke: var(--line-strong);
    stroke-width: 0.012;
  }
  .saturation {
    fill: var(--accent);
    fill-opacity: 0.08;
    fill-rule: evenodd;
  }
  .outer {
    fill: none;
    stroke: var(--accent);
    stroke-width: 0.01;
    stroke-dasharray: 0.04 0.03;
  }
  .axis {
    stroke: var(--line);
    stroke-width: 0.008;
  }
  .threshold,
  .zone {
    fill: none;
    stroke: var(--layer-2);
    stroke-width: 0.01;
    stroke-dasharray: 0.015 0.025;
  }
  .zone {
    stroke-opacity: 0.35;
  }
  .deadzone {
    fill: var(--text-faint);
    fill-opacity: 0.22;
    stroke: var(--text-faint);
    stroke-width: 0.01;
  }
  .up {
    stroke: var(--warning);
    stroke-width: 0.012;
    stroke-opacity: 0.6;
  }
  .up-dot {
    fill: var(--warning);
  }
  .out-line {
    stroke: var(--accent);
    stroke-width: 0.012;
    stroke-opacity: 0.5;
  }
  .raw {
    fill: none;
    stroke: var(--text);
    stroke-width: 0.018;
  }
  .out {
    fill: var(--accent);
  }
  .legend {
    display: flex;
    gap: 12px;
    font-size: 11.5px;
    color: var(--text-faint);
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .k {
    display: inline-block;
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  .k.raw {
    border: 1.5px solid var(--text);
  }
  .k.out {
    background: var(--accent);
  }
  .k.threshold {
    border: 1.5px dotted var(--layer-2);
  }
</style>
