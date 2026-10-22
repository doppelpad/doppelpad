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
  import Toggle from '../ui/Toggle.svelte';
  import CurveEditor from './CurveEditor.svelte';
  import LayerNote from './LayerNote.svelte';
  import StickRing from './StickRing.svelte';

  let { side }: { side: Side } = $props();

  const def = $derived(app.catalog?.defaults.stick);
  /** The settings as they are on the current layer, and Main's (what a layer resets to). */
  const cfg = $derived(app.profile && def ? stickConfig(app.profile, side, def, app.layer) : null);
  const main = $derived(app.profile && def ? stickConfig(app.profile, side, def) : null);
  const ref = $derived(app.layer === 0 ? def : main);
  const resetLabel = $derived(app.layer === 0 ? undefined : t('analog.layer.reset_field'));
  const disabled = $derived(app.readOnly);

  const zoneDefaults = $derived(zoneDefaultsOf(app.catalog?.defaults));
  /** Effective low, medium and high thresholds per axis. */
  const zones = $derived(stickThresholds(cfg?.zones, zoneDefaults));
  /** A low zone with separate horizontal and vertical radii is edited in the zone editor. */
  const ellipticLow = $derived(zones.x[0] !== zones.y[0]);

  /** The physical stick of the controller in use (y up). */
  const raw = $derived(app.primary ? stickAxes(app.primary, side) : null);
  const live = $derived(raw && cfg ? Math.hypot(...orient(raw, cfg)) : null);

  function set(patch: Partial<StickConfig>) {
    if (def) app.edit((p) => setStick(p, side, patch, def, app.layer));
  }
</script>

{#if cfg && def && ref}
  <LayerNote kind="sticks" sides={[side]} />
  <div class="stick">
    <div class="controls">
      <Slider
        label={t('analog.stick.deadzone')}
        value={cfg.deadzone}
        def={ref.deadzone}
        {resetLabel}
        min={0}
        max={0.9}
        step={0.01}
        scale={100}
        unit="%"
        hint={t('analog.stick.deadzone_hint')}
        {disabled}
        onchange={(v) => set({ deadzone: v })}
      />
      <Slider
        label={t('analog.stick.outer')}
        value={cfg.outer}
        def={ref.outer}
        {resetLabel}
        min={0.1}
        max={1}
        step={0.01}
        scale={100}
        unit="%"
        hint={t('analog.stick.outer_hint')}
        {disabled}
        onchange={(v) => set({ outer: v })}
      />
      <Slider
        label={t('analog.stick.anti_deadzone')}
        value={cfg.anti_deadzone}
        def={ref.anti_deadzone}
        {resetLabel}
        min={0}
        max={0.9}
        step={0.01}
        scale={100}
        unit="%"
        hint={t('analog.stick.anti_deadzone_hint')}
        {disabled}
        onchange={(v) => set({ anti_deadzone: v })}
      />
      <Slider
        label={t('analog.stick.sensitivity')}
        value={cfg.sensitivity}
        def={ref.sensitivity}
        {resetLabel}
        min={0.1}
        max={3}
        step={0.05}
        unit="×"
        {disabled}
        onchange={(v) => set({ sensitivity: v })}
      />
      <Slider
        label={t('analog.stick.rotation')}
        value={cfg.rotation}
        def={ref.rotation}
        {resetLabel}
        min={-180}
        max={180}
        step={1}
        unit="°"
        hint={t('analog.stick.rotation_hint')}
        {disabled}
        onchange={(v) => set({ rotation: v })}
      />
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
      <Slider
        label={t('analog.stick.direction_width')}
        value={cfg.direction_width}
        def={ref.direction_width}
        {resetLabel}
        min={5}
        max={40}
        step={0.5}
        unit="°"
        hint={t('analog.stick.direction_width_hint')}
        {disabled}
        onchange={(v) => set({ direction_width: v })}
      />
      <div class="toggles">
        <Toggle checked={cfg.invert_x} {disabled} label={t('analog.stick.invert_x')} onchange={(v) => set({ invert_x: v })} />
        <Toggle checked={cfg.invert_y} {disabled} label={t('analog.stick.invert_y')} onchange={(v) => set({ invert_y: v })} />
        <Toggle checked={cfg.diagonals} {disabled} label={t('analog.stick.diagonals')} onchange={(v) => set({ diagonals: v })} />
      </div>
      <p class="hint">{t('analog.stick.diagonals_hint')}</p>
    </div>

    <div class="previews">
      <StickRing {cfg} {raw} {zones} />
      <p class="live-hint">{raw ? t('analog.live_hint') : t('analog.no_controller')}</p>
      <div class="sub">{t('analog.stick.curve')}</div>
      <CurveEditor {cfg} {live} {disabled} onchange={(curve: CurvePoint[]) => set({ curve })} />
    </div>
  </div>
{/if}

<style>
  .hint {
    margin: -4px 0 0;
    color: var(--text-dim);
    font-size: 12px;
  }
  .stick {
    display: grid;
    grid-template-columns: minmax(240px, 1fr) minmax(260px, 320px);
    gap: 28px;
    align-items: start;
  }
  @container analog (max-width: 620px) {
    .stick {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .controls {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }
  .toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 20px;
    padding-top: 4px;
  }
  .previews {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .live-hint {
    margin: 0 0 6px;
    text-align: center;
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .sub {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
</style>
