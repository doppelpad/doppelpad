<script lang="ts">
  // Shaping of both triggers: output deadzone, the press point of the LT/RT controls (the low
  // zone) and the full pull, with the live pull and output of the controller in use. The
  // medium and high zones are edited in the binding panel's zone editor.
  import { t } from '../../lib/i18n/index.svelte';
  import { SIDES, SIDE_NAMES, controlLabel } from '../../lib/model/controls';
  import { triggerAxis } from '../../lib/model/devices';
  import { setTrigger, shapeTrigger, triggerConfig, type Side } from '../../lib/model/analog';
  import { setTriggerLevel, triggerThresholds, zoneDefaults as zoneDefaultsOf } from '../../lib/model/zones';
  import { app } from '../../lib/state/app.svelte';
  import type { TriggerConfig } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';
  import LayerNote from './LayerNote.svelte';
  import TriggerEffectEditor from './TriggerEffectEditor.svelte';


  const def = $derived(app.catalog?.defaults.trigger);
  const zoneDefaults = $derived(zoneDefaultsOf(app.catalog?.defaults));
  const disabled = $derived(app.readOnly);
  const resetLabel = $derived(app.layer === 0 ? undefined : t('analog.layer.reset_field'));

  function set(side: Side, patch: Partial<TriggerConfig>) {
    if (def) app.edit((p) => setTrigger(p, side, patch, def, app.layer));
  }

  const pct = (v: number) => `${(Math.min(1, Math.max(0, v)) * 100).toFixed(1)}%`;
</script>

{#if app.profile && def}
  <LayerNote kind="triggers" sides={['left', 'right']} />
  <div class="triggers">
    {#each SIDE_NAMES as side (side)}
      {@const cfg = triggerConfig(app.profile, side, def, app.layer)}
      {@const ref = app.layer === 0 ? def : triggerConfig(app.profile, side, def)}
      {@const raw = app.primary ? triggerAxis(app.primary, side) : null}
      {@const name = controlLabel(SIDES[side].trigger, app.family)}
      {@const zones = triggerThresholds(cfg.zones, cfg.full_pull, zoneDefaults)}
      <section class="card">
        <h3>{name}</h3>

        <div class="meter">
          <span class="ml">{t('analog.trigger.pull')}</span>
          <div class="bar">
            {#if cfg.deadzone > 0}<div class="zone dz" style:width={pct(cfg.deadzone)}></div>{/if}
            <div class="zone full" style:left={pct(cfg.full_pull)}></div>
            {#if raw !== null}<div class="fill raw" style:width={pct(raw)}></div>{/if}
            {#each [zones[1], zones[2]] as z, i (i)}<div class="mark zone" style:left={pct(z)}></div>{/each}
            <div class="mark" style:left={pct(zones[0])}></div>
          </div>
          <span class="ml">{t('analog.trigger.output')}</span>
          <div class="bar">
            {#if raw !== null}<div class="fill out" style:width={pct(shapeTrigger(raw, cfg))}></div>{/if}
          </div>
        </div>
        <div class="states">
          <span class="state" class:on={raw !== null && raw >= Math.max(zones[0], 0.001)}>
            {t('analog.trigger.pressed')}
          </span>
          <span class="state" class:on={raw !== null && raw >= cfg.full_pull}>{t('analog.trigger.full')}</span>
          {#if raw === null}<span class="none">{t('analog.no_controller')}</span>{/if}
        </div>

        <Slider
          label={t('analog.trigger.deadzone')}
          value={cfg.deadzone}
          def={ref.deadzone}
          {resetLabel}
          min={0}
          max={0.9}
          step={0.01}
          scale={100}
          unit="%"
          hint={t('analog.trigger.deadzone_hint')}
          {disabled}
          onchange={(v) => set(side, { deadzone: v })}
        />
        <Slider
          label={t('analog.trigger.press_threshold')}
          value={zones[0]}
          def={triggerThresholds(ref.zones, ref.full_pull, zoneDefaults)[0]}
          {resetLabel}
          min={0.01}
          max={1}
          step={0.01}
          scale={100}
          unit="%"
          hint={t('analog.trigger.press_threshold_hint', { control: name })}
          {disabled}
          onchange={(v) => set(side, { zones: setTriggerLevel(cfg.zones, 'low', v) })}
        />
        <Slider
          label={t('analog.trigger.full_pull')}
          value={cfg.full_pull}
          def={ref.full_pull}
          {resetLabel}
          min={0.05}
          max={1}
          step={0.01}
          scale={100}
          unit="%"
          hint={t('analog.trigger.full_pull_hint', { control: controlLabel(`${SIDES[side].trigger}Full`, app.family) })}
          {disabled}
          onchange={(v) => set(side, { full_pull: v })}
        />
        <TriggerEffectEditor
          effect={cfg.effect}
          onlayer={app.layer !== 0}
          {disabled}
          onchange={(e) => set(side, { effect: e })}
        />
      </section>
    {/each}
  </div>
{/if}

<style>
  .triggers {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
    gap: 16px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px 16px 16px;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--bg-1);
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  .meter {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 8px 10px;
  }
  .ml {
    font-size: 11.5px;
    color: var(--text-faint);
  }
  .bar {
    position: relative;
    height: 14px;
    border-radius: 4px;
    background: var(--bg-0);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .zone {
    position: absolute;
    top: 0;
    bottom: 0;
  }
  .zone.dz {
    left: 0;
    background: color-mix(in srgb, var(--text-faint) 30%, transparent);
  }
  .zone.full {
    right: 0;
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .fill {
    position: absolute;
    left: 0;
    top: 3px;
    bottom: 3px;
    border-radius: 2px;
  }
  .fill.raw {
    background: var(--text-dim);
  }
  .fill.out {
    background: var(--accent);
  }
  .mark {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--layer-2);
  }
  .mark.zone {
    opacity: 0.4;
  }
  .states {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: -4px;
  }
  .state {
    font-size: 11.5px;
    padding: 2px 8px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--line);
    color: var(--text-faint);
  }
  .state.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .none {
    font-size: 11.5px;
    color: var(--text-faint);
  }
</style>
