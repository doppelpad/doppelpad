<script lang="ts">
  // A DualSense trigger effect: resistance from a point, a gun trigger that gives way, or a
  // vibration, with presets and the raw bytes for experiments. On a layer, "as on Main"
  // follows Main and "off" turns Main's effect off.
  import { t } from '../../lib/i18n/index.svelte';
  import {
    EFFECT_KINDS,
    EFFECT_PRESETS,
    effectHex,
    freshEffect,
    parseEffectHex,
    sameEffect,
    withEffectStart,
    type EffectKind,
  } from '../../lib/model/triggerEffect';
  import type { TriggerEffect } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';

  let {
    effect,
    onlayer,
    disabled = false,
    onchange,
  }: { effect: TriggerEffect | undefined; onlayer: boolean; disabled?: boolean; onchange: (e: TriggerEffect | undefined) => void } =
    $props();

  function pick(v: string) {
    onchange(v === '' ? undefined : freshEffect(v as EffectKind));
  }

  /** Change fields of the effect `e` (the one shown), keeping its kind. */
  function set<E extends TriggerEffect>(e: E, patch: Partial<Omit<E, 'kind'>>) {
    onchange({ ...e, ...patch });
  }

  function setRaw(text: string) {
    const bytes = parseEffectHex(text);
    if (bytes) onchange({ kind: 'raw', bytes });
  }
</script>

<div class="effect">
  <label class="row">
    <span>{t('analog.trigger.effect')}</span>
    <select value={effect?.kind ?? ''} {disabled} onchange={(e) => pick(e.currentTarget.value)}>
      <option value="">{t(onlayer ? 'analog.trigger.effect_main' : 'analog.trigger.effect_none')}</option>
      {#each EFFECT_KINDS as k (k)}<option value={k}>{t(`analog.trigger.effects.${k}`)}</option>{/each}
    </select>
  </label>
  {#if !disabled}
    <div class="presets">
      {#each EFFECT_PRESETS as p (p.id)}
        <button class="chip" class:active={sameEffect(p.effect, effect)} onclick={() => onchange(p.effect)}>
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
      onchange={(v) => effect && onchange(withEffectStart(effect, v))}
    />
  {/if}
  {#if effect?.kind === 'weapon'}
    {@const e = effect}
    <Slider
      label={t('analog.trigger.effect_end')}
      value={effect.end}
      min={effect.start + 1}
      max={8}
      step={1}
      scale={10}
      unit="%"
      {disabled}
      onchange={(v) => set(e, { end: v })}
    />
  {/if}
  {#if effect?.kind === 'resistance' || effect?.kind === 'weapon'}
    {@const e = effect}
    <Slider label={t('analog.trigger.effect_strength')} value={e.strength} min={1} max={8} step={1} {disabled} onchange={(v) => set(e, { strength: v })} />
  {/if}
  {#if effect?.kind === 'vibration'}
    {@const e = effect}
    <Slider label={t('analog.trigger.effect_amplitude')} value={e.amplitude} min={1} max={8} step={1} {disabled} onchange={(v) => set(e, { amplitude: v })} />
    <Slider label={t('analog.trigger.effect_frequency')} value={e.frequency} min={1} max={255} step={1} unit="Hz" {disabled} onchange={(v) => set(e, { frequency: v })} />
  {/if}
  {#if effect?.kind === 'raw'}
    <label class="row">
      <span>{t('analog.trigger.effect_bytes')}</span>
      <input class="raw" value={effectHex(effect.bytes)} {disabled} onchange={(e) => setRaw(e.currentTarget.value)} />
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
    border-radius: var(--radius-pill);
    border: 1px solid var(--line-strong);
    font-size: var(--fs-s);
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
    font-size: var(--fs-s);
  }
  .hint {
    margin: 0;
    color: var(--text-faint);
    font-size: var(--fs-s);
  }
</style>
