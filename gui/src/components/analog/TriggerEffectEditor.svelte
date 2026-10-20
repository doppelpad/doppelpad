<script lang="ts">
  // A DualSense trigger effect: resistance from a point, a gun trigger that gives way, or a
  // vibration, with presets and the raw bytes for experiments. On a layer, "as on Main"
  // follows Main and "off" turns Main's effect off.
  import { t } from '../../lib/i18n/index.svelte';
  import type { TriggerEffect } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';

  type Kind = TriggerEffect['kind'];
  let {
    effect,
    onlayer,
    disabled = false,
    onchange,
  }: { effect: TriggerEffect | undefined; onlayer: boolean; disabled?: boolean; onchange: (e: TriggerEffect | undefined) => void } =
    $props();

  const KINDS: Kind[] = ['resistance', 'weapon', 'vibration', 'off', 'raw'];
  const FRESH: Record<Kind, TriggerEffect> = {
    off: { kind: 'off' },
    resistance: { kind: 'resistance', start: 0, strength: 4 },
    weapon: { kind: 'weapon', start: 4, end: 6, strength: 8 },
    vibration: { kind: 'vibration', start: 0, amplitude: 4, frequency: 20 },
    raw: { kind: 'raw', bytes: [0x05, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
  };
  const PRESETS: { id: string; effect: TriggerEffect }[] = [
    { id: 'soft', effect: { kind: 'resistance', start: 0, strength: 2 } },
    { id: 'medium', effect: { kind: 'resistance', start: 0, strength: 4 } },
    { id: 'hard', effect: { kind: 'resistance', start: 0, strength: 7 } },
    { id: 'half', effect: { kind: 'resistance', start: 5, strength: 6 } },
    { id: 'rifle', effect: { kind: 'weapon', start: 4, end: 6, strength: 8 } },
    { id: 'pulse', effect: { kind: 'vibration', start: 0, amplitude: 4, frequency: 10 } },
    { id: 'rumble', effect: { kind: 'vibration', start: 0, amplitude: 6, frequency: 40 } },
  ];

  function pick(v: string) {
    onchange(v === '' ? undefined : FRESH[v as Kind]);
  }

  function set(patch: Record<string, number>) {
    if (effect) onchange({ ...effect, ...patch } as TriggerEffect);
  }

  const hex = (bytes: number[]) => bytes.map((b) => b.toString(16).padStart(2, '0')).join(' ');

  function setRaw(text: string) {
    const bytes = text
      .trim()
      .split(/[\s,]+/)
      .map((h) => parseInt(h, 16));
    if (bytes.length === 11 && bytes.every((b) => Number.isInteger(b) && b >= 0 && b <= 255)) onchange({ kind: 'raw', bytes });
  }
</script>

<div class="effect">
  <label class="row">
    <span>{t('analog.trigger.effect')}</span>
    <select value={effect?.kind ?? ''} {disabled} onchange={(e) => pick(e.currentTarget.value)}>
      <option value="">{t(onlayer ? 'analog.trigger.effect_main' : 'analog.trigger.effect_none')}</option>
      {#each KINDS as k (k)}<option value={k}>{t(`analog.trigger.effects.${k}`)}</option>{/each}
    </select>
  </label>
  {#if !disabled}
    <div class="presets">
      {#each PRESETS as p (p.id)}
        <button class="chip" class:active={JSON.stringify(p.effect) === JSON.stringify(effect)} onclick={() => onchange(p.effect)}>
          {t(`analog.trigger.presets.${p.id}`)}
        </button>
      {/each}
    </div>
  {/if}
  {#if effect?.kind === 'resistance' || effect?.kind === 'weapon' || effect?.kind === 'vibration'}
    <Slider
      label={t('analog.trigger.effect_start')}
      value={effect.start}
      min={effect.kind === 'weapon' ? 2 : 0}
      max={effect.kind === 'weapon' ? 7 : 9}
      step={1}
      scale={10}
      unit="%"
      {disabled}
      onchange={(v) => set(effect?.kind === 'weapon' ? { start: v, end: Math.max(v + 1, effect.end) } : { start: v })}
    />
  {/if}
  {#if effect?.kind === 'weapon'}
    <Slider
      label={t('analog.trigger.effect_end')}
      value={effect.end}
      min={effect.start + 1}
      max={8}
      step={1}
      scale={10}
      unit="%"
      {disabled}
      onchange={(v) => set({ end: v })}
    />
  {/if}
  {#if effect?.kind === 'resistance' || effect?.kind === 'weapon'}
    <Slider label={t('analog.trigger.effect_strength')} value={effect.strength} min={1} max={8} step={1} {disabled} onchange={(v) => set({ strength: v })} />
  {/if}
  {#if effect?.kind === 'vibration'}
    <Slider label={t('analog.trigger.effect_amplitude')} value={effect.amplitude} min={1} max={8} step={1} {disabled} onchange={(v) => set({ amplitude: v })} />
    <Slider label={t('analog.trigger.effect_frequency')} value={effect.frequency} min={1} max={255} step={1} unit="Hz" {disabled} onchange={(v) => set({ frequency: v })} />
  {/if}
  {#if effect?.kind === 'raw'}
    <label class="row">
      <span>{t('analog.trigger.effect_bytes')}</span>
      <input class="raw" value={hex(effect.bytes)} {disabled} onchange={(e) => setRaw(e.currentTarget.value)} />
    </label>
  {/if}
  <p class="hint">{t('analog.trigger.effect_hint')}</p>
</div>

<style>
  .effect {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    color: var(--text-dim);
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    padding: 2px 10px;
    border-radius: 99px;
    border: 1px solid var(--line-strong);
    font-size: 12px;
    color: var(--text-dim);
  }
  .chip.active {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .raw {
    width: 230px;
    font-family: var(--mono, monospace);
    font-size: 12px;
  }
  .hint {
    margin: 0;
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
