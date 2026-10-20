<script lang="ts">
  // Shaping of one physical stick: deadzones, sensitivity, rotation, inversion, the
  // response curve and the threshold for its direction controls, with a live preview.
  import { t } from '../../lib/i18n/index.svelte';
  import { orient, setStick, stickConfig, type CurvePoint, type Side } from '../../lib/model/analog';
  import { app } from '../../lib/state/app.svelte';
  import type { StickConfig } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import CurveEditor from './CurveEditor.svelte';
  import StickRing from './StickRing.svelte';

  let { side }: { side: Side } = $props();

  const def = $derived(app.catalog?.defaults.stick);
  const cfg = $derived(app.profile && def ? stickConfig(app.profile, side, def) : null);
  const disabled = $derived(app.readOnly);

  /** The physical stick of the controller in use (y up). */
  const raw = $derived.by<[number, number] | null>(() => {
    const a = app.primary?.state.axes;
    if (!a) return null;
    return side === 'left' ? [a[0], a[1]] : [a[2], a[3]];
  });
  const live = $derived(raw && cfg ? Math.hypot(...orient(raw, cfg)) : null);

  function set(patch: Partial<StickConfig>) {
    if (app.profile && def) app.update(setStick(app.profile, side, patch, def));
  }
</script>

{#if cfg && def}
  <div class="stick">
    <div class="controls">
      <Slider
        label={t('analog.stick.deadzone')}
        value={cfg.deadzone}
        def={def.deadzone}
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
        def={def.outer}
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
        def={def.anti_deadzone}
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
        def={def.sensitivity}
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
        def={def.rotation}
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
        value={cfg.direction_threshold}
        def={def.direction_threshold}
        min={0.05}
        max={0.95}
        step={0.01}
        scale={100}
        unit="%"
        hint={t('analog.stick.direction_threshold_hint')}
        {disabled}
        onchange={(v) => set({ direction_threshold: v })}
      />
      <div class="toggles">
        <Toggle checked={cfg.invert_x} {disabled} label={t('analog.stick.invert_x')} onchange={(v) => set({ invert_x: v })} />
        <Toggle checked={cfg.invert_y} {disabled} label={t('analog.stick.invert_y')} onchange={(v) => set({ invert_y: v })} />
      </div>
    </div>

    <div class="previews">
      <StickRing {cfg} {raw} />
      <p class="live-hint">{raw ? t('analog.live_hint') : t('analog.no_controller')}</p>
      <div class="sub">{t('analog.stick.curve')}</div>
      <CurveEditor {cfg} {live} {disabled} onchange={(curve: CurvePoint[]) => set({ curve })} />
    </div>
  </div>
{/if}

<style>
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
