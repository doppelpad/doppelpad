<script lang="ts">
  // The config's timing; mappings can override all but the shortcut window. Empty fields use
  // the defaults (shown as placeholders).
  import { t } from '../../lib/i18n/index.svelte';
  import { parseNonNegInt } from '../../lib/model/numbers';
  import { setProfileTiming, type TimingField } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import NumberField from '../ui/NumberField.svelte';
  import OutputSection from './OutputSection.svelte';

  const TIMING: TimingField[] = ['combo_window_ms', 'long_press_ms', 'double_tap_ms', 'triple_tap_ms', 'turbo_ms', 'key_delay_ms'];

  function setTiming(field: TimingField, raw: string) {
    app.edit((p) => setProfileTiming(p, field, parseNonNegInt(raw) ?? undefined));
  }
</script>

<OutputSection title={t('output.timing')} hint={t('output.timing_hint')}>
  {#snippet fields()}
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
  {/snippet}
</OutputSection>
