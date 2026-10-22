<script lang="ts">
  // The mouse: how fast wheel steps repeat and where the physical mouse's zones start; and how
  // the game turns with mouse motion (flick stick and turn actions), with a test turn.
  import { RotateCw } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { MOUSE_ZONE_DEFAULTS, TURN_DEFAULTS, withMouseZone, withTurn } from '../../lib/model/output';
  import { app } from '../../lib/state/app.svelte';
  import NumberField from '../ui/NumberField.svelte';
  import OutputSection from './OutputSection.svelte';

  const turn = $derived({ ...TURN_DEFAULTS, ...app.profile?.virtual?.mouse });

  function setTurn(field: keyof typeof TURN_DEFAULTS, raw: string) {
    app.edit((p) => withTurn(p, field, raw));
  }

  /** Speeds (counts/s) where the physical mouse's zones start. */
  const mouseZones = $derived(app.profile?.mouse?.zones ?? MOUSE_ZONE_DEFAULTS);

  function setMouseZone(i: number, raw: string) {
    app.edit((p) => withMouseZone(p, i, raw));
  }

  const testTurn = () => app.attempt(api.testTurn(turn.counts_per_turn), t('output.test_turn_started'));
</script>

<OutputSection title={t('output.mouse')} hint={t('output.mouse_hint')}>
  {#snippet fields()}
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
  {/snippet}
</OutputSection>

<OutputSection title={t('output.turns')} hint={t('output.turns_hint')}>
  {#snippet fields()}
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
  {/snippet}
  <div class="test-turn">
    <button class="btn" onclick={testTurn}><RotateCw size={14} /> {t('output.test_turn')}</button>
    <span class="hint">{t('output.test_turn_hint')}</span>
  </div>
</OutputSection>

<style>
  .test-turn {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    margin-top: 10px;
  }
</style>
