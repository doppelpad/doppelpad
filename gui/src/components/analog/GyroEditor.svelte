<script lang="ts">
  // How the motion sensors become gyro controls: by turning (motion) or by tilting from a
  // centre pose (tilt), which axis steers, whether the gyro starts on, how gyro routes read
  // turning, and where the zones start. For the whole config.
  import { t } from '../../lib/i18n/index.svelte';
  import { gyroConfig, setGyro } from '../../lib/model/analog';
  import { app } from '../../lib/state/app.svelte';
  import type { GyroSettings } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';
  import Toggle from '../ui/Toggle.svelte';

  const def = $derived(app.catalog?.defaults.gyro);
  const cfg = $derived(app.profile && def ? gyroConfig(app.profile, def) : null);
  const disabled = $derived(app.readOnly);
  const tilt = $derived(cfg?.mode === 'tilt');
  const noGyro = $derived(!!app.primary && !app.primary.state.motion);

  function set(patch: Partial<GyroSettings>) {
    if (app.profile && def) app.update(setGyro(app.profile, patch, def));
  }

  const LEVELS = ['low', 'med', 'high'] as const;

  /** Set one zone threshold, moving the others so they keep increasing. */
  function setZone(i: number, v: number) {
    if (!cfg) return;
    const key = tilt ? 'tilt_zones' : 'motion_zones';
    const z = [...cfg[key]] as [number, number, number];
    z[i] = v;
    for (let j = i + 1; j < 3; j++) z[j] = Math.max(z[j], z[j - 1] + 1);
    for (let j = i - 1; j >= 0; j--) z[j] = Math.min(z[j], z[j + 1] - 1);
    set({ [key]: z.map((x) => Math.max(x, 0.5)) as [number, number, number] });
  }
</script>

{#snippet choice<T extends string>(label: string, value: T, options: readonly T[], key: string, onpick: (v: T) => void)}
  <div class="choice">
    <span class="cl">{label}</span>
    <div class="seg">
      {#each options as o (o)}
        <button class:on={value === o} {disabled} onclick={() => onpick(o)}>{t(`analog.gyro.${key}.${o}`)}</button>
      {/each}
    </div>
  </div>
{/snippet}

{#if cfg && def}
  <div class="gyro">
    {#if noGyro}<p class="warn">{t('analog.routes.no_gyro')}</p>{/if}
    {@render choice(t('analog.gyro.mode_label'), cfg.mode, ['motion', 'tilt'] as const, 'mode', (v) => set({ mode: v }))}
    <p class="hint">{t(`analog.gyro.mode_hint.${cfg.mode}`)}</p>
    {@render choice(t('analog.gyro.steering_label'), cfg.steering, ['roll', 'yaw'] as const, 'steering', (v) => set({ steering: v }))}
    <p class="hint">{t('analog.gyro.steering_hint')}</p>
    {@render choice(t('analog.gyro.space_label'), cfg.space, ['local', 'player'] as const, 'space', (v) => set({ space: v }))}
    <p class="hint">{t('analog.gyro.space_hint')}</p>
    <Toggle checked={cfg.initially_on} {disabled} label={t('analog.gyro.initially_on')} onchange={(v) => set({ initially_on: v })} />
    <p class="hint">{t('analog.gyro.initially_on_hint')}</p>

    <div class="sub">{t('analog.gyro.zones')}</div>
    <p class="hint">{t(tilt ? 'analog.gyro.zones_hint_tilt' : 'analog.gyro.zones_hint_motion')}</p>
    {#each LEVELS as level, i (level)}
      <Slider
        label={t(`zones.${level}`)}
        value={(tilt ? cfg.tilt_zones : cfg.motion_zones)[i]}
        def={(tilt ? def.tilt_zones : def.motion_zones)[i]}
        min={tilt ? 1 : 10}
        max={tilt ? 45 : 1000}
        step={tilt ? 0.5 : 5}
        unit={tilt ? '°' : '°/s'}
        {disabled}
        onchange={(v) => setZone(i, v)}
      />
    {/each}
  </div>
{/if}

<style>
  .gyro {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 560px;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .cl {
    min-width: 120px;
    font-size: 13px;
  }
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    overflow: hidden;
  }
  .seg button {
    padding: 5px 12px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .seg button + button {
    border-left: 1px solid var(--line);
  }
  .seg button.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .hint {
    margin: -4px 0 4px;
    color: var(--text-dim);
    font-size: 12px;
  }
  .warn {
    margin: 0;
    color: var(--warning);
    font-size: 12px;
  }
  .sub {
    margin-top: 8px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
</style>
