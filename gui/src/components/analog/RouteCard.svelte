<script lang="ts">
  // One analog route of the current layer: source → target, the settings that route uses,
  // and the controls it waits for.
  import { ArrowRight, Plus, Trash2, TriangleAlert, X } from '@lucide/svelte';
  import { onDestroy } from 'svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import {
    ROUTE_SOURCES,
    removeRoute,
    routeFields,
    routeTargets,
    sourceOf,
    sourcePatch,
    targetOf,
    updateRoute,
    type RouteField,
  } from '../../lib/model/analog';
  import { controlLabel, controlLabelMixed } from '../../lib/model/controls';
  import { keyFromEvent } from '../../lib/model/keymap';
  import { app } from '../../lib/state/app.svelte';
  import type { AnalogMapping, AnalogSource, AnalogTarget, ControlName } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';
  import Toggle from '../ui/Toggle.svelte';

  let { index, route }: { index: number; route: AnalogMapping } = $props();

  type SliderField = Exclude<RouteField, 'consume' | 'spring'>;
  const SLIDERS: Record<SliderField, { min: number; max: number; step: number; scale?: number; unit?: string; hint?: boolean }> = {
    curve: { min: 0.2, max: 4, step: 0.05, hint: true },
    deadzone: { min: 0, max: 0.9, step: 0.01, scale: 100, unit: '%' },
    anti_deadzone: { min: 0, max: 0.9, step: 0.01, scale: 100, unit: '%', hint: true },
    smoothing_ms: { min: 0, max: 100, step: 1, unit: 'ms' },
    decay_ms: { min: 0, max: 500, step: 5, unit: 'ms', hint: true },
    trackball_friction: { min: 0, max: 1, step: 0.01, scale: 100, unit: '%', hint: true },
    snap: { min: 0, max: 0.5, step: 0.01, scale: 100, unit: '%', hint: true },
    noise_filter: { min: 0, max: 20, step: 1, hint: true },
    acceleration: { min: 0, max: 4, step: 0.1, unit: '×/s', hint: true },
  };

  const def = $derived(app.catalog?.defaults.analog);
  /** The route with the defaults filled in. */
  const r = $derived<Required<AnalogMapping>>({ ...def, ...route } as Required<AnalogMapping>);
  const source = $derived<AnalogSource>(sourceOf(route));
  const target = $derived<AnalogTarget>(targetOf(route));
  const fields = $derived(routeFields(source, target));
  const sliders = $derived(fields.filter((f): f is SliderField => f !== 'consume' && f !== 'spring'));
  const scaleKey = $derived(
    `${source === 'left_stick' || source === 'right_stick' ? 'stick' : source}>${target === 'mouse' || target === 'flick' ? target : 'stick'}`,
  );
  const disabled = $derived(app.readOnly);
  /** Flick stick turns only horizontally; its threshold is the route's deadzone. */
  const flick = $derived(target === 'flick');
  const noGyro = $derived(source === 'gyro' && !!app.primary && !app.primary.state.motion);

  /** Controls offered for "only while held": controller buttons, triggers, mouse buttons. */
  const heldOptions = $derived([
    ...((app.catalog?.pad_buttons ?? []) as string[]),
    'LT',
    'RT',
    'LTFull',
    'RTFull',
  ]);
  const mouseOptions = $derived((app.catalog?.mouse_buttons ?? []).map((b) => `Mouse:${b}`));

  function set(patch: Partial<AnalogMapping>) {
    if (app.profile) app.update(updateRoute(app.profile, index, patch));
  }

  function addHeld(c: ControlName) {
    if (!r.while_held.includes(c)) set({ while_held: [...r.while_held, c] });
  }

  let capturing = $state(false);

  function startCapture() {
    capturing = true;
    app.pressListener = (c) => {
      addHeld(c);
      stopCapture();
    };
  }

  function stopCapture() {
    if (!capturing) return;
    capturing = false;
    app.pressListener = null;
  }

  function onKeydown(e: KeyboardEvent) {
    if (!capturing) return;
    e.preventDefault();
    if (e.key === 'Escape') return stopCapture();
    const key = keyFromEvent(e);
    if (key) {
      addHeld(`Key:${key}`);
      stopCapture();
    }
  }

  function onAddHeld(e: Event & { currentTarget: HTMLSelectElement }) {
    const v = e.currentTarget.value;
    e.currentTarget.value = '';
    if (v === '__press') startCapture();
    else if (v) addHeld(v);
  }

  onDestroy(stopCapture);
</script>

<svelte:window onkeydown={onKeydown} />

<section class="route">
  <header>
    <label class="end">
      <span>{t('analog.routes.from')}</span>
      <select value={source} {disabled} onchange={(e) => set(sourcePatch(route, e.currentTarget.value as AnalogSource))}>
        {#each ROUTE_SOURCES as s (s)}<option value={s}>{t(`analog.routes.sources.${s}`)}</option>{/each}
      </select>
    </label>
    <ArrowRight size={16} />
    <label class="end">
      <span>{t('analog.routes.to')}</span>
      <select value={target} {disabled} onchange={(e) => set({ target: e.currentTarget.value as AnalogTarget })}>
        {#each routeTargets(source) as tg (tg)}<option value={tg}>{t(`analog.routes.targets.${tg}`)}</option>{/each}
      </select>
    </label>
    {#if !disabled}
      <button class="icon-btn del" title={t('analog.routes.delete')} onclick={() => app.profile && app.update(removeRoute(app.profile, index))}>
        <Trash2 size={15} />
      </button>
    {/if}
  </header>

  {#if flick}
    <p class="flick-note">{t('analog.routes.flick_hint')}</p>
  {/if}
  {#if noGyro}
    <div class="warn"><TriangleAlert size={14} /> {t('analog.routes.no_gyro')}</div>
  {/if}

  <div class="fields">
    <Slider
      label={t('analog.routes.sensitivity')}
      value={r.sensitivity}
      def={def?.sensitivity}
      min={0.05}
      max={10}
      step={0.05}
      unit="×"
      hint={t(`analog.routes.scale.${scaleKey}`)}
      {disabled}
      onchange={(v) => set({ sensitivity: v })}
    />
    {#if !flick}
      <Slider
        label={t('analog.routes.vertical_ratio')}
        value={r.vertical_ratio}
        def={def?.vertical_ratio}
        min={0.1}
        max={3}
        step={0.05}
        unit="×"
        hint={t('analog.routes.vertical_ratio_hint')}
        {disabled}
        onchange={(v) => set({ vertical_ratio: v })}
      />
    {/if}
    {#each sliders as f (f)}
      {@const s = SLIDERS[f]}
      <Slider
        label={t(flick && f === 'deadzone' ? 'analog.routes.flick_threshold' : `analog.routes.${f}`)}
        value={r[f]}
        def={def?.[f]}
        min={s.min}
        max={s.max}
        step={s.step}
        scale={s.scale}
        unit={s.unit}
        hint={flick && f === 'deadzone' ? t('analog.routes.flick_threshold_hint') : s.hint ? t(`analog.routes.${f}_hint`) : ''}
        {disabled}
        onchange={(v) => set({ [f]: v })}
      />
    {/each}
  </div>

  <div class="toggles">
    <Toggle checked={r.invert_x} {disabled} label={t('analog.routes.invert_x')} onchange={(v) => set({ invert_x: v })} />
    {#if !flick}
      <Toggle checked={r.invert_y} {disabled} label={t('analog.routes.invert_y')} onchange={(v) => set({ invert_y: v })} />
    {/if}
    {#if fields.includes('spring')}
      <Toggle checked={r.spring} {disabled} label={t('analog.routes.spring')} onchange={(v) => set({ spring: v })} />
    {/if}
    {#if fields.includes('consume')}
      <Toggle
        checked={r.consume}
        {disabled}
        label={t(source === 'mouse' ? 'analog.routes.consume_mouse' : 'analog.routes.consume_stick')}
        onchange={(v) => set({ consume: v })}
      />
    {/if}
  </div>

  <div class="held">
    <span class="hl">{t('analog.routes.while_held')}</span>
    {#each r.while_held as c (c)}
      <span class="chip">
        {controlLabelMixed(c, app.family)}
        {#if !disabled}
          <button class="chip-x" title={t('common.remove')} onclick={() => set({ while_held: r.while_held.filter((x) => x !== c) })}>
            <X size={11} />
          </button>
        {/if}
      </span>
    {/each}
    {#if r.while_held.length === 0}<span class="always">{t('analog.routes.always')}</span>{/if}
    {#if capturing}
      <span class="capturing">{t('analog.routes.pressing')}</span>
      <button class="icon-btn" title={t('common.cancel')} onclick={stopCapture}><X size={14} /></button>
    {:else if !disabled}
      <label class="add">
        <Plus size={13} />
        <select value="" aria-label={t('analog.routes.add_control')} onchange={onAddHeld}>
          <option value="" disabled>{t('analog.routes.add_control')}</option>
          <option value="__press">{t('analog.routes.press_control')}</option>
          <optgroup label={t('picker.gamepad')}>
            {#each heldOptions.filter((c) => !r.while_held.includes(c)) as c (c)}
              <option value={c}>{controlLabel(c, app.family)}</option>
            {/each}
          </optgroup>
          <optgroup label={t('picker.mouse')}>
            {#each mouseOptions.filter((c) => !r.while_held.includes(c)) as c (c)}
              <option value={c}>{controlLabel(c, app.family)}</option>
            {/each}
          </optgroup>
        </select>
      </label>
    {/if}
  </div>
</section>

<style>
  .route {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 12px 16px 16px;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--bg-1);
  }
  header {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    color: var(--text-faint);
  }
  header > :global(svg) {
    margin-bottom: 8px;
    flex: none;
  }
  .end {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .end span {
    font-size: 11px;
    color: var(--text-faint);
  }
  .end select {
    min-width: 140px;
  }
  .del {
    margin-left: auto;
  }
  .del:hover {
    color: var(--danger);
  }
  .flick-note {
    margin: 0;
    color: var(--text-dim);
    font-size: 12px;
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--warning);
    font-size: 12px;
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 14px 24px;
  }
  .toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 20px;
  }
  .held {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .hl {
    color: var(--text-dim);
    margin-right: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 6px 3px 10px;
    border-radius: 99px;
    background: var(--bg-3);
    border: 1px solid var(--line-strong);
    font-weight: 500;
    font-size: 12px;
  }
  .chip-x {
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    color: var(--text-dim);
  }
  .chip-x:hover {
    background: var(--danger);
    color: white;
  }
  .always,
  .capturing {
    color: var(--text-faint);
    font-size: 12px;
  }
  .capturing {
    color: var(--accent);
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--accent);
    border: 1px dashed var(--accent);
    border-radius: 99px;
    padding: 0 4px 0 8px;
  }
  .add select {
    background: none;
    border: none;
    color: var(--accent);
    padding: 3px 2px;
    font-size: 12px;
    cursor: pointer;
  }
  .add select option,
  .add select optgroup {
    color: var(--text);
    background: var(--bg-2);
  }
</style>
