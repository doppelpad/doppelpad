<script lang="ts">
  // Shaping of one physical stick: deadzones, sensitivity, rotation, inversion, the
  // response curve and where its direction controls start (the low zone), with a live
  // preview. The medium and high zones are edited in the binding panel's zone editor.
  import { t } from '../../lib/i18n/index.svelte';
  import { orient, setStick, stickConfig, type CurvePoint, type Side } from '../../lib/model/analog';
  import { stickAxes } from '../../lib/model/devices';
  import { setStickLevel, stickThresholds, zoneDefaults as zoneDefaultsOf } from '../../lib/model/zones';
  import { app } from '../../lib/state/app.svelte';
  import type { StickConfig } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';
  import { PCT, sliderHint, sliderUnit, type SliderSpec } from '../ui/sliderSpec';
  import Toggle from '../ui/Toggle.svelte';
  import CurveEditor from './CurveEditor.svelte';
  import EditorColumns from './EditorColumns.svelte';
  import AnalogLayerNote from './AnalogLayerNote.svelte';
  import StickRing from './StickRing.svelte';

  let { side }: { side: Side } = $props();

  const def = $derived(app.catalog?.defaults.stick);
  /** The settings as they are on the current layer, and Main's (what a layer resets to). */
  const cfg = $derived(app.profile && def ? stickConfig(app.profile, side, def, app.layer) : null);
  const main = $derived(app.profile && def ? stickConfig(app.profile, side, def) : null);
  const ref = $derived(app.layer === 0 ? def : main);
  const resetLabel = $derived(app.layer === 0 ? undefined : t('analog.layer.reset_field'));
  type StickField = 'deadzone' | 'outer' | 'anti_deadzone' | 'sensitivity' | 'rotation' | 'direction_width';
  /** The plain sliders, in order; the direction threshold comes before the direction width. */
  const SLIDERS: [StickField, SliderSpec][] = [
    ['deadzone', { min: 0, max: 0.9, step: 0.01, ...PCT, hint: true }],
    ['outer', { min: 0.1, max: 1, step: 0.01, ...PCT, hint: true }],
    ['anti_deadzone', { min: 0, max: 0.9, step: 0.01, ...PCT, hint: true }],
    ['sensitivity', { min: 0.1, max: 3, step: 0.05, unit: '×' }],
    ['rotation', { min: -180, max: 180, step: 1, unit: '°', hint: true }],
    ['direction_width', { min: 5, max: 40, step: 0.5, unit: '°', hint: true }],
  ];
  const disabled = $derived(app.readOnly);

  const zoneDefaults = $derived(zoneDefaultsOf(app.catalog?.defaults));
  /** Effective low, medium and high thresholds per axis. */
  const zones = $derived(stickThresholds(cfg?.zones, zoneDefaults));
  /** A low zone with separate horizontal and vertical radii is edited in the zone editor. */
  const ellipticLow = $derived(zones.x[0] !== zones.y[0]);

  /** The physical stick of the controller in use (y up). */
  const raw = $derived(app.primary ? stickAxes(app.primary.state, side) : null);
  const live = $derived(raw && cfg ? Math.hypot(...orient(raw, cfg)) : null);

  function set(patch: Partial<StickConfig>) {
    if (def) app.edit((p) => setStick(p, side, patch, def, app.layer));
  }
</script>

{#if cfg && def && ref}
  <AnalogLayerNote kind="sticks" sides={[side]} />
  <EditorColumns>
    {#snippet controls()}
      {#snippet threshold()}
        <Slider
          label={t('analog.stick.direction_threshold')}
          value={zones.x[0]}
          def={stickThresholds(ref.zones, zoneDefaults).x[0]}
          {resetLabel}
          min={0.05}
          max={0.95}
          step={0.01}
          scale={100}
          unit="%"
          hint={t(ellipticLow ? 'analog.stick.direction_threshold_elliptic' : 'analog.stick.direction_threshold_hint')}
          disabled={disabled || ellipticLow}
          onchange={(v) => set({ zones: setStickLevel(cfg.zones, 'low', 'both', v, zoneDefaults) })}
        />
      {/snippet}
      {#each SLIDERS as [f, s] (f)}
        {#if f === 'direction_width'}{@render threshold()}{/if}
        <Slider
          label={t(`analog.stick.${f}`)}
          value={cfg[f]}
          def={ref[f]}
          {resetLabel}
          min={s.min}
          max={s.max}
          step={s.step}
          scale={s.scale}
          unit={sliderUnit(s)}
          hint={sliderHint('analog.stick', f, s)}
          {disabled}
          onchange={(v) => set({ [f]: v })}
        />
      {/each}
      <div class="toggles">
        <Toggle checked={cfg.invert_x} {disabled} label={t('analog.stick.invert_x')} onchange={(v) => set({ invert_x: v })} />
        <Toggle checked={cfg.invert_y} {disabled} label={t('analog.stick.invert_y')} onchange={(v) => set({ invert_y: v })} />
        <Toggle checked={cfg.diagonals} {disabled} label={t('analog.stick.diagonals')} onchange={(v) => set({ diagonals: v })} />
      </div>
      <p class="hint">{t('analog.stick.diagonals_hint')}</p>
    {/snippet}

    {#snippet previews()}
      <StickRing {cfg} {raw} {zones} />
      <p class="live-hint">{raw ? t('analog.live_hint') : t('analog.no_controller')}</p>
      <div class="section-title">{t('analog.stick.curve')}</div>
      <CurveEditor {cfg} {live} {disabled} onchange={(curve: CurvePoint[]) => set({ curve })} />
    {/snippet}
  </EditorColumns>
{/if}

<style>
  .toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 20px;
    padding-top: 4px;
  }
  .live-hint {
    margin: 0 0 6px;
    text-align: center;
    color: var(--text-faint);
    font-size: 11.5px;
  }
</style>
