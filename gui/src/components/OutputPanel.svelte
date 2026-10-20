<script lang="ts">
  import { Gamepad2, Keyboard, RotateCw, TriangleAlert } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { setProfileTiming, setVirtualDevice, type TimingField } from '../lib/model/profile';
  import { app } from '../lib/state/app.svelte';
  import type { KeyRepeat, Output, Profile, VirtualPadType } from '../lib/types';
  import Toggle from './ui/Toggle.svelte';

  /** Profile-wide timing; mappings can override all but the shortcut window. */
  const TIMING: TimingField[] = ['combo_window_ms', 'long_press_ms', 'double_tap_ms', 'triple_tap_ms', 'turbo_ms', 'key_delay_ms'];

  const repeat = $derived<KeyRepeat>(app.profile?.virtual?.keyboard?.repeat ?? 'native');
  const REPEATS = ['native', 'fast', 'slow', 'off', 'custom'] as const;

  function setRepeat(r: KeyRepeat) {
    if (!app.profile) return;
    app.update(setVirtualDevice(app.profile, 'keyboard', r === 'native' ? undefined : { repeat: r }));
  }

  /** How the game turns with mouse motion: flick stick and turn actions. */
  const TURN = { counts_per_turn: 18000, turn_ms: 100, wheel_repeat_ms: 0 };
  const turn = $derived({ ...TURN, ...app.profile?.virtual?.mouse });

  function setTurn(field: keyof typeof TURN, raw: string) {
    if (!app.profile) return;
    const n = Math.round(Number(raw.replace(',', '.')));
    const ok = field === 'counts_per_turn' ? n > 0 : n >= 0 && n <= 5000;
    const next = { ...turn, [field]: Number.isFinite(n) && ok ? n : TURN[field] };
    const isDefault = (Object.keys(TURN) as (keyof typeof TURN)[]).every((k) => next[k] === TURN[k]);
    app.update(setVirtualDevice(app.profile, 'mouse', isDefault ? undefined : next));
  }

  /** Speeds (counts/s) where the physical mouse's zones start. */
  const MOUSE_ZONES: [number, number, number] = [200, 1000, 3000];
  const mouseZones = $derived(app.profile?.mouse?.zones ?? MOUSE_ZONES);

  function setMouseZone(i: number, raw: string) {
    if (!app.profile) return;
    const n = Math.round(Number(raw));
    if (!Number.isFinite(n) || n <= 0) return;
    const z = [...mouseZones] as [number, number, number];
    z[i] = n;
    for (let j = i + 1; j < 3; j++) z[j] = Math.max(z[j], z[j - 1] + 1);
    for (let j = i - 1; j >= 0; j--) z[j] = Math.min(z[j], z[j + 1] - 1);
    const p: Profile = { ...app.profile, mouse: { zones: z } };
    if (z.every((v, j) => v === MOUSE_ZONES[j])) delete p.mouse;
    app.update(p);
  }

  async function testTurn() {
    try {
      await api.testTurn(turn.counts_per_turn);
      app.toast(t('output.test_turn_started'), 'ok');
    } catch (e) {
      app.toast(String(e), 'error');
    }
  }

  /** Shake actions: strength and length (the virtual controller's accelerometer). */
  const SHAKE = { shake_g: 2, shake_ms: 150 };
  const shake = $derived({ ...SHAKE, ...app.profile?.virtual?.motion });

  function setShake(field: keyof typeof SHAKE, raw: string) {
    if (!app.profile) return;
    const n = Number(raw.replace(',', '.'));
    const v = field === 'shake_ms' ? Math.round(n) : n;
    const next = { ...shake, [field]: Number.isFinite(v) && v > 0 ? v : SHAKE[field] };
    const isDefault = next.shake_g === SHAKE.shake_g && next.shake_ms === SHAKE.shake_ms;
    app.update(setVirtualDevice(app.profile, 'motion', isDefault ? undefined : next));
  }

  function setTiming(field: TimingField, raw: string) {
    if (!app.profile) return;
    const n = parseInt(raw, 10);
    app.update(setProfileTiming(app.profile, field, Number.isFinite(n) && n >= 0 ? n : undefined));
  }

  const out = $derived<Required<Output>>({
    ...(app.catalog?.defaults.output ?? { virtual_pad: 'xbox360', hide_physical: false, passthrough: true, mute_mapped_inputs: false }),
    ...(app.profile?.output ?? {}),
  } as Required<Output>);

  const drivers = $derived(app.live?.drivers);

  function set(patch: Partial<Output>) {
    if (!app.profile) return;
    app.update({ ...app.profile, output: { ...(app.profile.output ?? {}), ...patch } });
  }

  const PADS: { id: VirtualPadType; icon: typeof Gamepad2 }[] = [
    { id: 'xbox360', icon: Gamepad2 },
    { id: 'dual_shock4', icon: Gamepad2 },
    { id: 'none', icon: Keyboard },
  ];
</script>

<div class="output">
  <h2>{t('views.output')}</h2>

  <div class="label">{t('output.virtual_pad')}</div>
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
  </div>

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

  <div class="label timing-label">{t('output.mouse')}</div>
  <p class="hint">{t('output.mouse_hint')}</p>
  <div class="timing">
    <label class="field">
      <span class="fl">{t('output.wheel_repeat')}</span>
      <input
        type="number"
        min="0"
        max="1000"
        step="10"
        value={turn.wheel_repeat_ms}
        disabled={app.readOnly}
        onchange={(e) => setTurn('wheel_repeat_ms', e.currentTarget.value)}
      />
      <span class="unit">{t('common.ms')}</span>
    </label>
    {#each ['low', 'med', 'high'] as level, i (level)}
      <label class="field">
        <span class="fl">{t(`output.mouse_zone_${level}`)}</span>
        <input
          type="number"
          min="1"
          step="50"
          value={mouseZones[i]}
          disabled={app.readOnly}
          onchange={(e) => setMouseZone(i, e.currentTarget.value)}
        />
        <span class="unit">/s</span>
      </label>
    {/each}
  </div>

  <div class="label timing-label">{t('output.turns')}</div>
  <p class="hint">{t('output.turns_hint')}</p>
  <div class="timing">
    <label class="field">
      <span class="fl">{t('output.counts_per_turn')}</span>
      <input
        type="number"
        min="1"
        step="100"
        value={turn.counts_per_turn}
        disabled={app.readOnly}
        onchange={(e) => setTurn('counts_per_turn', e.currentTarget.value)}
      />
    </label>
    <label class="field">
      <span class="fl">{t('output.turn_ms')}</span>
      <input
        type="number"
        min="0"
        max="5000"
        step="10"
        value={turn.turn_ms}
        disabled={app.readOnly}
        onchange={(e) => setTurn('turn_ms', e.currentTarget.value)}
      />
      <span class="unit">{t('common.ms')}</span>
    </label>
  </div>
  <div class="test-turn">
    <button class="btn" onclick={testTurn}><RotateCw size={14} /> {t('output.test_turn')}</button>
    <span class="hint">{t('output.test_turn_hint')}</span>
  </div>

  <div class="label timing-label">{t('output.shake')}</div>
  <p class="hint">{t('output.shake_hint')}</p>
  <div class="timing">
    <label class="field">
      <span class="fl">{t('output.shake_g')}</span>
      <input
        type="number"
        min="0.5"
        max="8"
        step="0.5"
        value={shake.shake_g}
        disabled={app.readOnly}
        onchange={(e) => setShake('shake_g', e.currentTarget.value)}
      />
      <span class="unit">g</span>
    </label>
    <label class="field">
      <span class="fl">{t('output.shake_ms')}</span>
      <input
        type="number"
        min="10"
        max="2000"
        step="10"
        value={shake.shake_ms}
        disabled={app.readOnly}
        onchange={(e) => setShake('shake_ms', e.currentTarget.value)}
      />
      <span class="unit">{t('common.ms')}</span>
    </label>
  </div>

  <div class="label timing-label">{t('output.timing')}</div>
  <p class="hint">{t('output.timing_hint')}</p>
  <div class="timing">
    {#each TIMING as f (f)}
      <label class="field">
        <span class="fl">{t(`output.timing_fields.${f}`)}</span>
        <input
          type="number"
          min="0"
          step="10"
          value={app.profile?.timing?.[f] ?? ''}
          placeholder={String(app.catalog?.defaults.timing[f] ?? '')}
          disabled={app.readOnly}
          onchange={(e) => setTiming(f, e.currentTarget.value)}
        />
        <span class="unit">{t('common.ms')}</span>
      </label>
    {/each}
  </div>
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
  .label {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
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
    border-radius: var(--radius-m);
    border: 1px solid var(--line);
    background: var(--bg-1);
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
    background: rgba(245, 158, 11, 0.08);
    border: 1px solid rgba(245, 158, 11, 0.3);
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
    font-size: 12px;
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
