<script lang="ts">
  // A labelled slider with a number field and a reset button. Values are stored as given
  // (e.g. 0.08) and shown scaled (e.g. 8 %).
  import { RotateCcw } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';

  let {
    label,
    value,
    def,
    min,
    max,
    step,
    scale = 1,
    unit = '',
    hint = '',
    disabled = false,
    resetLabel,
    onchange,
  }: {
    label: string;
    value: number;
    /** Shows a reset button while the value differs from it. */
    def?: number;
    min: number;
    max: number;
    step: number;
    /** Factor for display, e.g. 100 for percent. */
    scale?: number;
    unit?: string;
    hint?: string;
    disabled?: boolean;
    /** Title of the reset button (default: "Reset to default"). */
    resetLabel?: string;
    onchange: (v: number) => void;
  } = $props();

  const decimals = (n: number) => (String(n).split('.')[1] ?? '').length;
  /** Decimals of a stored value: those of the step (0.01 → 2). */
  const valueDecimals = $derived(decimals(step));
  const shownDecimals = $derived(Math.max(0, valueDecimals - Math.round(Math.log10(scale))));

  function set(v: number) {
    if (!Number.isFinite(v)) return;
    const f = 10 ** valueDecimals;
    onchange(Math.round(Math.min(max, Math.max(min, v)) * f) / f);
  }

  const shown = $derived(Number((value * scale).toFixed(shownDecimals)));
  /** How far the track is filled, for its two colors. */
  const fill = $derived(max > min ? Math.min(100, Math.max(0, ((value - min) / (max - min)) * 100)) : 0);
</script>

<div class="slider" class:disabled>
  <div class="top">
    <span class="label">{label}</span>
    {#if def !== undefined && Math.abs(value - def) > 1e-6 && !disabled}
      <button class="reset" title={resetLabel ?? t('common.reset')} onclick={() => onchange(def)}><RotateCcw size={12} /></button>
    {/if}
    <span class="value">
      <input
        type="number"
        min={min * scale}
        max={max * scale}
        step={step * scale}
        value={shown}
        {disabled}
        aria-label={label}
        onchange={(e) => set(Number(e.currentTarget.value) / scale)}
      />
      {#if unit}<span class="unit">{unit}</span>{/if}
    </span>
  </div>
  <input
    type="range"
    {min}
    {max}
    {step}
    {value}
    {disabled}
    aria-label={label}
    style:--fill="{fill}%"
    oninput={(e) => set(Number(e.currentTarget.value))}
  />
  {#if hint}<div class="hint">{hint}</div>{/if}
</div>

<style>
  .slider {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .label {
    flex: 1;
    min-width: 0;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .reset {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: var(--radius-s);
    color: var(--text-faint);
  }
  .reset:hover {
    color: var(--accent);
    background: var(--bg-3);
  }
  .value {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .value input {
    width: 64px;
    padding: 3px 6px;
    text-align: right;
  }
  .unit {
    min-width: 14px;
    color: var(--text-faint);
    font-size: var(--fs-s);
  }
  /* Drawn here instead of natively: a thin track, teal up to the value, and a teal knob. */
  input[type='range'] {
    appearance: none;
    width: 100%;
    height: 18px;
    margin: 0;
    padding: 0;
    border: none;
    border-radius: 0;
    background: transparent;
    cursor: pointer;
  }
  input[type='range']::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--accent) var(--fill), var(--line-strong) var(--fill));
  }
  input[type='range']::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    margin-top: -5px;
    border-radius: 50%;
    background: var(--accent);
    transition: box-shadow 0.12s;
  }
  input[type='range']:hover::-webkit-slider-thumb,
  input[type='range']:focus-visible::-webkit-slider-thumb {
    box-shadow: 0 0 0 4px var(--accent-soft);
  }
  .disabled input[type='range'] {
    cursor: default;
    opacity: 0.45;
  }
  .disabled input[type='range']::-webkit-slider-thumb {
    box-shadow: none;
  }
</style>
