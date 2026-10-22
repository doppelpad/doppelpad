<script lang="ts">
  import { Gamepad2, Keyboard, RotateCw, TriangleAlert } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { setProfileTiming, setVirtualDevice, type TimingField } from '../lib/model/profile';
  import { MOUSE_ZONE_DEFAULTS, SHAKE_DEFAULTS, TURN_DEFAULTS, withMouseZone, withShake, withTurn } from '../lib/model/output';
  import { parseNonNegInt } from '../lib/model/numbers';
  import { app } from '../lib/state/app.svelte';
  import type { KbmSource, KeyRepeat, Output, VirtualPadType } from '../lib/types';
  import CrosshairSettings from './crosshair/CrosshairSettings.svelte';
  import LightSettings from './output/LightSettings.svelte';
  import VibrationSettings from './output/VibrationSettings.svelte';
  import RadialSettings from './radial/RadialSettings.svelte';
  import NumberField from './ui/NumberField.svelte';
  import Toggle from './ui/Toggle.svelte';

  /** Profile-wide timing; mappings can override all but the shortcut window. */
  const TIMING: TimingField[] = ['combo_window_ms', 'long_press_ms', 'double_tap_ms', 'triple_tap_ms', 'turbo_ms', 'key_delay_ms'];

  const repeat = $derived<KeyRepeat>(app.profile?.virtual?.keyboard?.repeat ?? 'native');
  const REPEATS = ['native', 'fast', 'slow', 'off', 'custom'] as const;

  function setRepeat(r: KeyRepeat) {
    app.edit((p) => setVirtualDevice(p, 'keyboard', r === 'native' ? undefined : { repeat: r }));
  }

  /** How the game turns with mouse motion: flick stick and turn actions. */
  const turn = $derived({ ...TURN_DEFAULTS, ...app.profile?.virtual?.mouse });

  function setTurn(field: keyof typeof TURN_DEFAULTS, raw: string) {
    app.edit((p) => withTurn(p, field, raw));
  }

  const KBM_SOURCES: KbmSource[] = ['all', 'physical', 'scripts'];

  /** Speeds (counts/s) where the physical mouse's zones start. */
  const mouseZones = $derived(app.profile?.mouse?.zones ?? MOUSE_ZONE_DEFAULTS);

  function setMouseZone(i: number, raw: string) {
    app.edit((p) => withMouseZone(p, i, raw));
  }

  const testTurn = () => app.attempt(api.testTurn(turn.counts_per_turn), t('output.test_turn_started'));

  /** Shake actions: strength and length (the virtual controller's accelerometer). */
  const shake = $derived({ ...SHAKE_DEFAULTS, ...app.profile?.virtual?.motion });

  function setShake(field: keyof typeof SHAKE_DEFAULTS, raw: string) {
    app.edit((p) => withShake(p, field, raw));
  }

  function setTiming(field: TimingField, raw: string) {
    app.edit((p) => setProfileTiming(p, field, parseNonNegInt(raw) ?? undefined));
  }

  const out = $derived<Required<Output>>({
    ...(app.catalog?.defaults.output ?? { virtual_pad: 'xbox360', hide_physical: true, passthrough: true, mute_mapped_inputs: true }),
    ...(app.profile?.output ?? {}),
  } as Required<Output>);

  const drivers = $derived(app.live?.drivers);

  function set(patch: Partial<Output>) {
    app.edit((p) => ({ ...p, output: { ...(p.output ?? {}), ...patch } }));
  }

  const PADS: { id: VirtualPadType; icon: typeof Gamepad2 }[] = [
    { id: 'xbox360', icon: Gamepad2 },
    { id: 'dual_shock4', icon: Gamepad2 },
    { id: 'none', icon: Keyboard },
  ];
</script>

<div class="output">
  <h2>{t('views.output')}</h2>

  <div class="section-title">{t('output.virtual_pad')}</div>
  <div class="cards">
    {#each PADS as p (p.id)}
      <button
        class="card"
        class:active={out.virtual_pad === p.id}
        disabled={app.readOnly}
        onclick={() => set({ virtual_pad: p.id })}
      >
        <p.icon size={26} strokeWidth={1.5} />
        <span>{t(`output.${p.id}`)}</span>
      </button>
    {/each}
  </div>
  {#if out.virtual_pad !== 'none' && drivers && !drivers.virtual_pads.available}
    <div class="warn">
      <TriangleAlert size={15} />
      {t('output.driver_missing', { driver: 'ViGEmBus' })}
    </div>
  {/if}

  <div class="options">
    <Toggle
      checked={out.hide_physical}
      disabled={app.readOnly}
      label={t('output.hide_physical')}
      onchange={(v) => set({ hide_physical: v })}
    />
    {#if out.hide_physical && drivers && !drivers.device_hiding.available}
      <div class="warn">
        <TriangleAlert size={15} />
        {t('output.hide_missing')}
      </div>
    {/if}
    <Toggle
      checked={out.passthrough}
      disabled={app.readOnly}
      label={t('output.passthrough')}
      onchange={(v) => set({ passthrough: v })}
    />
    <Toggle
      checked={out.mute_mapped_inputs}
      disabled={app.readOnly}
      label={t('output.mute_mapped')}
      onchange={(v) => set({ mute_mapped_inputs: v })}
    />
    <Toggle
      checked={app.profile?.layer_change?.repress ?? true}
      disabled={app.readOnly}
      label={t('output.repress')}
      onchange={(v) => app.edit((p) => ({ ...p, layer_change: v ? undefined : { repress: false } }))}
    />
    <p class="hint">{t('output.repress_hint')}</p>
  </div>

  <label class="field repeat">
    <span class="fl">{t('output.kbm_source')}</span>
    <select
      value={app.profile?.kbm_source ?? 'all'}
      disabled={app.readOnly}
      onchange={(e) => {
        const v = e.currentTarget.value as KbmSource;
        app.edit((p) => ({ ...p, kbm_source: v === 'all' ? undefined : v }));
      }}
    >
      {#each KBM_SOURCES as s (s)}<option value={s}>{t(`output.kbm_sources.${s}`)}</option>{/each}
    </select>
  </label>
  <p class="hint">{t('output.kbm_source_hint')}</p>

  <label class="field repeat">
    <span class="fl">{t('output.key_repeat')}</span>
    <select
      value={typeof repeat === 'number' ? 'custom' : repeat}
      disabled={app.readOnly}
      onchange={(e) => {
        const v = e.currentTarget.value;
        setRepeat(v === 'custom' ? 30 : (v as KeyRepeat));
      }}
    >
      {#each REPEATS as r (r)}
        <option value={r}>{t(`output.repeat.${r}`)}</option>
      {/each}
    </select>
    {#if typeof repeat === 'number'}
      <input
        type="number"
        min="1"
        max="1000"
        value={repeat}
        disabled={app.readOnly}
        onchange={(e) => {
          const n = parseInt(e.currentTarget.value, 10);
          if (n >= 1 && n <= 1000) setRepeat(n);
        }}
      />
      <span class="unit">{t('common.ms')}</span>
    {/if}
  </label>
  <p class="hint">{t(`output.repeat_hint.${typeof repeat === 'number' ? 'custom' : repeat}`)}</p>

  <div class="section-title timing-label">{t('output.mouse')}</div>
  <p class="hint">{t('output.mouse_hint')}</p>
  <div class="timing">
    <NumberField
      label={t('output.wheel_repeat')}
      value={turn.wheel_repeat_ms}
      unit={t('common.ms')}
      min={0}
      max={1000}
      step={10}
      disabled={app.readOnly}
      onchange={(v) => setTurn('wheel_repeat_ms', v)}
    />
    {#each ['low', 'med', 'high'] as level, i (level)}
      <NumberField
        label={t(`output.mouse_zone_${level}`)}
        value={mouseZones[i]}
        unit="/s"
        min={1}
        step={50}
        disabled={app.readOnly}
        onchange={(v) => setMouseZone(i, v)}
      />
    {/each}
  </div>

  <div class="section-title timing-label">{t('output.turns')}</div>
  <p class="hint">{t('output.turns_hint')}</p>
  <div class="timing">
    <NumberField
      label={t('output.counts_per_turn')}
      value={turn.counts_per_turn}
      min={1}
      step={100}
      disabled={app.readOnly}
      onchange={(v) => setTurn('counts_per_turn', v)}
    />
    <NumberField
      label={t('output.turn_ms')}
      value={turn.turn_ms}
      unit={t('common.ms')}
      min={0}
      max={5000}
      step={10}
      disabled={app.readOnly}
      onchange={(v) => setTurn('turn_ms', v)}
    />
  </div>
  <div class="test-turn">
    <button class="btn" onclick={testTurn}><RotateCw size={14} /> {t('output.test_turn')}</button>
    <span class="hint">{t('output.test_turn_hint')}</span>
  </div>

  <div class="section-title timing-label">{t('output.shake')}</div>
  <p class="hint">{t('output.shake_hint')}</p>
  <div class="timing">
    <NumberField
      label={t('output.shake_g')}
      value={shake.shake_g}
      unit="g"
      min={0.5}
      max={8}
      step={0.5}
      disabled={app.readOnly}
      onchange={(v) => setShake('shake_g', v)}
    />
    <NumberField
      label={t('output.shake_ms')}
      value={shake.shake_ms}
      unit={t('common.ms')}
      min={10}
      max={2000}
      step={10}
      disabled={app.readOnly}
      onchange={(v) => setShake('shake_ms', v)}
    />
  </div>

  <div class="section-title timing-label">{t('output.timing')}</div>
  <p class="hint">{t('output.timing_hint')}</p>
  <div class="timing">
    {#each TIMING as f (f)}
      <NumberField
        label={t(`output.timing_fields.${f}`)}
        value={app.profile?.timing?.[f] ?? ''}
        unit={t('common.ms')}
        min={0}
        step={10}
        placeholder={String(app.catalog?.defaults.timing[f] ?? '')}
        disabled={app.readOnly}
        onchange={(v) => setTiming(f, v)}
      />
    {/each}
  </div>

  <LightSettings />
  <VibrationSettings />
  <CrosshairSettings />
  <RadialSettings />
</div>

<style>
  .output {
    max-width: 640px;
    margin: 0 auto;
    padding: 32px 28px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 18px;
    font-weight: 600;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 18px 10px;
    color: var(--text-dim);
    text-align: center;
  }
  .card:hover:not(:disabled) {
    border-color: var(--line-strong);
    color: var(--text);
  }
  .card.active {
    border-color: var(--accent);
    color: var(--text);
    background: var(--accent-soft);
  }
  .card.active :global(svg) {
    color: var(--accent);
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
    border-radius: var(--radius-s);
    padding: 8px 10px;
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 10px;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }
  .timing-label {
    margin-top: 10px;
    padding-top: 16px;
    border-top: 1px solid var(--line);
  }
  .hint {
    margin: -6px 0 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .test-turn {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    margin-top: 10px;
  }
  .test-turn .hint {
    margin: 0;
  }
  .timing {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 8px 20px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .fl {
    flex: 1;
  }
  .field input {
    width: 80px;
  }
  .field.repeat {
    margin-top: 4px;
  }
  .field.repeat .fl {
    flex: 0 0 auto;
  }
  .field.repeat select {
    min-width: 200px;
  }
  .unit {
    color: var(--text-dim);
  }
</style>
