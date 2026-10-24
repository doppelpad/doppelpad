<script lang="ts">
  // The binding panel when nothing is selected: how to start, how to reach a layer nothing
  // changes to yet, and the mappings of the current layer.
  import { ChevronRight, Info, Route } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { comboLabel, mappingSummary } from '../../lib/model/labels';
  import { isReached } from '../../lib/model/lints';
  import { activatorOf, layerOf } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { Combo, Mapping } from '../../lib/types';
  import LayerNote from '../ui/LayerNote.svelte';
  import ActivatorIcon from './ActivatorIcon.svelte';

  const layerMappings = $derived(app.profile ? app.profile.mappings.filter((m) => layerOf(m) === app.layer) : []);

  function selectMapping(m: Mapping) {
    app.select(m.trigger as Combo);
    app.activator = activatorOf(m);
  }
</script>

<div class="intro">
  <Info size={16} />
  <p>{t('binding.choose')}</p>
</div>
{#if app.profile && !isReached(app.profile, app.layer)}
  <LayerNote icon={Route}>{t('binding.layer_unreached', { layer: app.layerLabel(app.layer) })}</LayerNote>
{/if}
<div class="section-title">{t('binding.layer_mappings', { layer: app.layerLabel(app.layer) })}</div>
{#if layerMappings.length === 0}
  <p class="muted">{t('binding.no_layer_mappings')}</p>
{/if}
<div class="overview">
  {#each layerMappings as m, i (i)}
    <button class="ov-row" onclick={() => selectMapping(m)}>
      <span class="ov-act"><ActivatorIcon activator={activatorOf(m)} size={14} /></span>
      <span class="ov-name">{comboLabel(m.trigger, app.family)}</span>
      <span class="ov-sum">{mappingSummary(m, app.layerLabel)}</span>
      <ChevronRight size={14} />
    </button>
  {/each}
</div>

<style>
  .intro {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    color: var(--text-dim);
    background: var(--bg-2);
    border-radius: var(--radius-m);
    padding: 10px 12px;
  }
  .intro p {
    margin: 0;
  }
  .section-title {
    margin-top: 6px;
  }
  .muted {
    color: var(--text-faint);
    margin: 0;
  }
  .overview {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ov-row {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr) minmax(0, 1.2fr) 14px;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    border-radius: var(--radius-s);
    text-align: left;
    color: var(--text-dim);
  }
  .ov-row:hover {
    background: var(--bg-2);
    color: var(--text);
  }
  .ov-act {
    color: var(--accent);
    display: inline-flex;
  }
  .ov-name,
  .ov-sum {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ov-name {
    color: var(--text);
  }
</style>
