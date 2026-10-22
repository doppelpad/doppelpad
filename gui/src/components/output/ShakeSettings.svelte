<script lang="ts">
  // Shake actions: their strength and length on the virtual controller's accelerometer.
  import { t } from '../../lib/i18n/index.svelte';
  import { SHAKE_DEFAULTS, withShake } from '../../lib/model/output';
  import { app } from '../../lib/state/app.svelte';
  import NumberField from '../ui/NumberField.svelte';
  import OutputSection from './OutputSection.svelte';

  const shake = $derived({ ...SHAKE_DEFAULTS, ...app.profile?.virtual?.motion });

  function setShake(field: keyof typeof SHAKE_DEFAULTS, raw: string) {
    app.edit((p) => withShake(p, field, raw));
  }
</script>

<OutputSection title={t('output.shake')} hint={t('output.shake_hint')}>
  {#snippet fields()}
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
  {/snippet}
</OutputSection>
