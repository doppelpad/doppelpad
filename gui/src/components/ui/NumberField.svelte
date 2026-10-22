<script lang="ts">
  // A number setting on one line: its name, the input and a unit. `onchange` gets the raw text;
  // the caller parses and checks it.
  let {
    label,
    value,
    unit,
    min,
    max,
    step,
    placeholder,
    disabled = false,
    onchange,
  }: {
    label: string;
    /** Empty when not set. */
    value: number | '';
    unit?: string;
    min?: number;
    max?: number;
    step?: number;
    placeholder?: string;
    disabled?: boolean;
    onchange: (raw: string) => void;
  } = $props();
</script>

<label class="field">
  <span class="label">{label}</span>
  <input type="number" {min} {max} {step} {value} {placeholder} {disabled} onchange={(e) => onchange(e.currentTarget.value)} />
  {#if unit}<span class="unit">{unit}</span>{/if}
</label>

<style>
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .label {
    flex: 1;
  }
  input {
    width: 80px;
  }
  .unit {
    color: var(--text-dim);
  }
</style>
