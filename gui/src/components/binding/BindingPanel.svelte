<script lang="ts">
  // Right-hand editor for the selected control: activator, actions, mode, block, layer
  // jump, description and timing overrides.
  import { ChevronRight, Info, SlidersHorizontal, Trash2 } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { analogTabOf } from '../../lib/model/controls';
  import { comboLabel } from '../../lib/model/labels';
  import { removeMapping, singleControl } from '../../lib/model/profile';
  import { app, type AnalogTab } from '../../lib/state/app.svelte';
  import type { Mapping } from '../../lib/types';
  import LayerNote from '../ui/LayerNote.svelte';
  import ActionChips from './ActionChips.svelte';
  import ActivatorTabs from './ActivatorTabs.svelte';
  import JumpEditor from './JumpEditor.svelte';
  import LayerOverview from './LayerOverview.svelte';
  import ModeSection from './ModeSection.svelte';
  import PanelHeader from './PanelHeader.svelte';
  import TimingFields from './TimingFields.svelte';
  import ZoneTabs from './ZoneTabs.svelte';

  const combo = $derived(app.selected);
  const readOnly = $derived(app.readOnly);

  const at = $derived<{ own?: Mapping; inherited?: Mapping }>(combo ? app.mappingAt(app.layer, combo, app.activator) : {});
  /** What the panel shows: this layer's mapping, else the inherited one. */
  const shown = $derived<Mapping | undefined>(at.own ?? at.inherited);

  const title = $derived(combo ? comboLabel(combo, app.family) : '');
  /** The selected control, when it is a single one. */
  const single = $derived(combo ? singleControl(combo) : null);
  /** The analog settings page of a selected stick or trigger control (including its zones). */
  const analogTab = $derived<AnalogTab | null>(single ? analogTabOf(single) : null);

  /** Apply a change to the selected mapping (see `app.editMapping`). */
  function edit(change: (m: Mapping) => void, keepEmpty = false) {
    if (combo) app.editMapping(app.layer, combo, app.activator, change, keepEmpty);
  }
</script>

<div class="panel">
  {#if !combo}
    <LayerOverview />
  {:else}
    <PanelHeader {title} layer={app.layer} onclose={() => app.select(null)} />

    {#if analogTab}
      {@const stick = analogTab !== 'triggers'}
      <button class="analog-link" onclick={() => app.openAnalog(analogTab)}>
        <SlidersHorizontal size={14} />
        <span class="al-text">
          <span>{t(stick ? 'binding.stick_settings' : 'binding.trigger_settings')}</span>
          <span class="al-hint">{t(stick ? 'binding.stick_settings_hint' : 'binding.trigger_settings_hint')}</span>
        </span>
        <ChevronRight size={14} />
      </button>
    {/if}

    {#if single}<ZoneTabs control={single} />{/if}

    <ActivatorTabs {combo} />

    {#if at.inherited}
      <LayerNote icon={Info}>{t('binding.inherited')}</LayerNote>
    {/if}

    <div class="section-title">{t('binding.actions')}</div>
    {#key app.selectedKey + app.activator}
      <ActionChips mapping={shown} {readOnly} {edit} />
    {/key}

    <ModeSection mapping={shown} {combo} {readOnly} {edit} />

    <div class="section-title">{t('binding.jump')}</div>
    <JumpEditor mapping={shown} {readOnly} {edit} />

    <div class="section-title">{t('binding.description')}</div>
    <textarea
      rows="2"
      placeholder={t('binding.description_placeholder')}
      value={shown?.description ?? ''}
      disabled={readOnly}
      onchange={(e) => edit((m) => (m.description = e.currentTarget.value.trim() || undefined))}
    ></textarea>

    <TimingFields mapping={shown} {readOnly} {edit} />

    {#if at.own && !readOnly}
      <button class="btn danger" onclick={() => app.edit((p) => removeMapping(p, app.layer, combo, app.activator))}>
        <Trash2 size={14} /> {t('binding.delete')}
      </button>
    {/if}
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 14px 20px;
  }
  .analog-link {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line);
    background: var(--bg-2);
    color: var(--text-dim);
    text-align: left;
  }
  .analog-link:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .al-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .al-text > span:first-child {
    color: var(--text);
    font-weight: 500;
  }
  .al-hint {
    font-size: 11.5px;
    color: var(--text-faint);
  }
  .section-title {
    margin-top: 6px;
  }
  textarea {
    resize: vertical;
    min-height: 44px;
  }
  .danger {
    align-self: flex-start;
    color: var(--danger);
    margin-top: 6px;
  }
</style>
