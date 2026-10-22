<script lang="ts">
  // How mappings push one virtual stick from buttons and keys: where the deflection starts,
  // how fast it grows and where it stops, with a preview of the first two seconds, and the
  // output curve of the whole virtual stick, and an optional jitter on top.
  import { t } from '../../lib/i18n/index.svelte';
  import { jitterConfig, setVirtualStick, virtualRamp, virtualStickConfig, type CurvePoint, type Side } from '../../lib/model/analog';
  import { app } from '../../lib/state/app.svelte';
  import type { Jitter, VirtualStick } from '../../lib/types';
  import Segmented from '../ui/Segmented.svelte';
  import Slider from '../ui/Slider.svelte';
  import { PCT, sliderHint, sliderUnit, type SliderSpec } from '../ui/sliderSpec';
  import Toggle from '../ui/Toggle.svelte';
  import CurveEditor from './CurveEditor.svelte';
  import EditorColumns from './EditorColumns.svelte';
  import AnalogLayerNote from './AnalogLayerNote.svelte';

  let { side }: { side: Side } = $props();

  /** The format's ramp: start at about a fifth, then grow and speed up. */
  const RAMP: Partial<VirtualStick> = { initial: 0.21, speed: 1, acceleration: 0.5 };
  type VirtualField = 'initial' | 'max' | 'speed' | 'acceleration' | 'multiplier_x' | 'multiplier_y';
  /** The plain sliders, in order. */
  const SLIDERS: [VirtualField, SliderSpec][] = [
    ['initial', { min: 0, max: 1, step: 0.01, ...PCT, hint: true }],
    ['max', { min: 0.05, max: 1, step: 0.01, ...PCT, hint: true }],
    ['speed', { min: 0, max: 4, step: 0.05, unit: '/s', hint: true }],
    ['acceleration', { min: 0, max: 4, step: 0.05, unit: '/s²' }],
    ['multiplier_x', { min: 0.1, max: 5, step: 0.1, unit: '×' }],
    ['multiplier_y', { min: 0.1, max: 5, step: 0.1, unit: '×', hint: 'analog.virtual.multiplier_hint' }],
  ];

  const def = $derived(app.catalog?.defaults.virtual_stick);
  const stickDef = $derived(app.catalog?.defaults.stick);
  const cfg = $derived(app.profile && def ? virtualStickConfig(app.profile, side, def, app.layer) : null);
  const ref = $derived(app.layer === 0 || !app.profile || !def ? def : virtualStickConfig(app.profile, side, def));
  const resetLabel = $derived(app.layer === 0 ? undefined : t('analog.layer.reset_field'));
  const disabled = $derived(app.readOnly);
  const instant = $derived(!!cfg && cfg.speed === 0 && cfg.acceleration === 0 && cfg.initial >= cfg.max);

  function set(patch: Partial<VirtualStick>) {
    if (def) app.edit((p) => setVirtualStick(p, side, patch, def, app.layer));
  }

  const PATTERNS = ['circle', 'square', 'cross'] as const;
  const jit = $derived(def ? jitterConfig(cfg?.jitter, def.jitter) : null);
  const refJit = $derived(def ? jitterConfig(ref?.jitter, def.jitter) : null);

  function setJitter(patch: Partial<Jitter>) {
    if (jit) set({ jitter: { ...jit, ...patch } });
  }

  // Preview: deflection over the first two seconds of holding a button (horizontal axis).
  const W = 240;
  const H = 90;
  const points = $derived.by(() => {
    if (!cfg) return '';
    const pts: string[] = [];
    for (let i = 0; i <= 48; i++) {
      const s = (i / 48) * 2;
      pts.push(`${((s / 2) * W).toFixed(1)},${((1 - virtualRamp(cfg, s, cfg.multiplier_x)) * H).toFixed(1)}`);
    }
    return pts.join(' ');
  });
  /** The curve preview draws only the curve (no stick deadzone or sensitivity). */
  const curveCfg = $derived(
    cfg && stickDef ? { ...stickDef, deadzone: 0, outer: 1, anti_deadzone: 0, sensitivity: 1, curve: cfg.curve } : null,
  );
</script>

{#if cfg && def && ref}
  <section class="virtual">
    <div class="section-title">{t('analog.virtual.title')}</div>
    <p class="hint">{t('analog.virtual.hint')}</p>
    <AnalogLayerNote kind="virtual_sticks" sides={[side]} />
    <EditorColumns>
      {#snippet controls()}
        <Segmented
          variant="chips"
          label={t('analog.virtual.title')}
          options={[
            { value: 'instant', label: t('analog.virtual.preset_instant') },
            { value: 'ramp', label: t('analog.virtual.preset_ramp') },
          ]}
          value={instant ? 'instant' : 'ramp'}
          {disabled}
          onchange={(v) => set(v === 'instant' ? { initial: 1, max: 1, speed: 0, acceleration: 0 } : RAMP)}
        />
        {#each SLIDERS as [f, s] (f)}
          <Slider
            label={t(`analog.virtual.${f}`)}
            value={cfg[f]}
            def={ref[f]}
            {resetLabel}
            min={s.min}
            max={s.max}
            step={s.step}
            scale={s.scale}
            unit={sliderUnit(s)}
            hint={sliderHint('analog.virtual', f, s)}
            {disabled}
            onchange={(v) => set({ [f]: v })}
          />
        {/each}
        <div class="jitter">
          <Toggle checked={!!jit?.enabled} {disabled} label={t('analog.virtual.jitter')} onchange={(v) => setJitter({ enabled: v })} />
          <p class="hint">{t('analog.virtual.jitter_hint')}</p>
          {#if jit?.enabled && refJit}
            <Segmented
              variant="chips"
              label={t('analog.virtual.jitter')}
              options={PATTERNS.map((pt) => ({ value: pt, label: t(`analog.virtual.pattern.${pt}`) }))}
              value={jit.pattern}
              {disabled}
              onchange={(pt) => setJitter({ pattern: pt })}
            />
            <Slider
              label={t('analog.virtual.jitter_min')}
              value={jit.min}
              def={refJit.min}
              {resetLabel}
              min={0.001}
              max={0.1}
              step={0.001}
              scale={100}
              unit="%"
              {disabled}
              onchange={(v) => setJitter({ min: v, max: Math.max(v, jit.max) })}
            />
            <Slider
              label={t('analog.virtual.jitter_max')}
              value={jit.max}
              def={refJit.max}
              {resetLabel}
              min={0.001}
              max={0.1}
              step={0.001}
              scale={100}
              unit="%"
              {disabled}
              onchange={(v) => setJitter({ max: v, min: Math.min(v, jit.min) })}
            />
            <Slider
              label={t('analog.virtual.delay_min')}
              value={jit.delay_min_ms}
              def={refJit.delay_min_ms}
              {resetLabel}
              min={10}
              max={100}
              step={1}
              unit={t('common.ms')}
              {disabled}
              onchange={(v) => setJitter({ delay_min_ms: v, delay_max_ms: Math.max(v, jit.delay_max_ms) })}
            />
            <Slider
              label={t('analog.virtual.delay_max')}
              value={jit.delay_max_ms}
              def={refJit.delay_max_ms}
              {resetLabel}
              min={10}
              max={100}
              step={1}
              unit={t('common.ms')}
              hint={t('analog.virtual.delay_hint')}
              {disabled}
              onchange={(v) => setJitter({ delay_max_ms: v, delay_min_ms: Math.min(v, jit.delay_min_ms) })}
            />
          {/if}
        </div>
      {/snippet}
      {#snippet previews()}
        <svg viewBox="-4 -4 {W + 8} {H + 22}" role="img" aria-label={t('analog.virtual.preview')}>
          <rect x="0" y="0" width={W} height={H} class="frame" />
          <line x1="0" x2={W} y1={(1 - cfg.max) * H} y2={(1 - cfg.max) * H} class="cap" />
          <polyline {points} class="ramp" />
          <text x="0" y={H + 14}>0 s</text>
          <text x={W / 2} y={H + 14} text-anchor="middle">1 s</text>
          <text x={W} y={H + 14} text-anchor="end">2 s</text>
        </svg>
        <p class="live-hint">{t('analog.virtual.preview')}</p>
        {#if curveCfg}
          <div class="section-title">{t('analog.virtual.curve')}</div>
          <CurveEditor cfg={curveCfg} live={null} {disabled} onchange={(curve: CurvePoint[]) => set({ curve })} />
          <p class="live-hint">{t('analog.virtual.curve_hint')}</p>
        {/if}
      {/snippet}
    </EditorColumns>
  </section>
{/if}

<style>
  .virtual {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 18px;
    border-top: 1px solid var(--line);
  }
  .hint {
    margin: -4px 0 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .jitter {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-top: 4px;
  }
  svg {
    width: 100%;
    max-width: 340px;
    height: auto;
    align-self: center;
  }
  svg text {
    font-size: 10px;
    fill: var(--text-faint);
  }
  .frame {
    fill: var(--bg-1);
    stroke: var(--line);
  }
  .cap {
    stroke: var(--text-faint);
    stroke-dasharray: 3 3;
  }
  .ramp {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
  }
  .live-hint {
    margin: 0 0 6px;
    text-align: center;
    color: var(--text-faint);
    font-size: 11.5px;
  }
</style>
