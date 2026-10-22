<script lang="ts">
  // A time in the binding panel's labels: a small number input, then "ms". Empty means not set
  // (the placeholder tells what applies then); `onchange` gets the raw text.
  import { t } from '../../lib/i18n/index.svelte';

  let {
    value,
    placeholder,
    min = 0,
    max,
    step = 10,
    disabled = false,
    onchange,
  }: {
    value: number | undefined;
    placeholder?: string;
    min?: number;
    max?: number;
    step?: number;
    disabled?: boolean;
    onchange: (raw: string) => void;
  } = $props();
</script>

<input type="number" {min} {max} {step} value={value ?? ''} {placeholder} {disabled} onchange={(e) => onchange(e.currentTarget.value)} />
{t('common.ms')}

<style>
  input {
    width: 72px;
    padding: 4px 6px;
  }
</style>
