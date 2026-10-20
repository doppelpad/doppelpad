<script lang="ts">
  // Settings of one shift layer: its name and which of Main's mappings it takes over.
  import { X } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { controlLabel } from '../lib/model/controls';
  import {
    comboKey,
    comboList,
    inheritableTriggers,
    layerInheritance,
    renameLayer,
    sameCombo,
    setLayerInherit,
    setTriggerInherited,
  } from '../lib/model/profile';
  import { app } from '../lib/state/app.svelte';
  import type { Combo } from '../lib/types';
  import Toggle from './ui/Toggle.svelte';

  let { layer, onclose }: { layer: number; onclose: () => void } = $props();

  const inheritance = $derived(app.profile ? layerInheritance(app.profile, layer) : { inherit: true, notInherited: [] });
  const triggers = $derived(app.profile ? inheritableTriggers(app.profile) : []);

  const label = (c: Combo) => comboList(c).map((x) => controlLabel(x, app.family)).join(' + ');
  const inherited = (c: Combo) => !inheritance.notInherited.some((x) => sameCombo(x, c));

  function rename(name: string) {
    if (app.profile && name.trim()) app.update(renameLayer(app.profile, layer, name.trim()));
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="backdrop" role="presentation" onclick={onclose}>
  <div class="dialog" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <div class="head">
      <h3>{t('layers.settings_title', { layer: app.layerLabel(layer) })}</h3>
      <button class="icon-btn" onclick={onclose} title={t('common.close')}><X size={17} /></button>
    </div>

    <label class="row">
      <span>{t('layers.name_prompt')}</span>
      <input value={app.layerLabel(layer)} disabled={app.readOnly} onchange={(e) => rename(e.currentTarget.value)} />
    </label>

    <Toggle
      checked={inheritance.inherit}
      disabled={app.readOnly}
      label={t('layers.inherit')}
      onchange={(v) => app.profile && app.update(setLayerInherit(app.profile, layer, v))}
    />
    <p class="hint">{t(inheritance.inherit ? 'layers.inherit_hint_on' : 'layers.inherit_hint_off')}</p>

    {#if inheritance.inherit}
      <div class="section">
        <div class="section-head">{t('layers.inherited_list')}</div>
        {#if triggers.length === 0}
          <p class="hint">{t('layers.inherit_none')}</p>
        {:else}
          <div class="list">
            {#each triggers as c (comboKey(c))}
              <label class="item">
                <input
                  type="checkbox"
                  checked={inherited(c)}
                  disabled={app.readOnly}
                  onchange={(e) =>
                    app.profile && app.update(setTriggerInherited(app.profile, layer, c, e.currentTarget.checked))}
                />
                <span>{label(c)}</span>
              </label>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(5, 7, 10, 0.6);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .dialog {
    width: 460px;
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 64px);
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow);
    padding: 18px 20px 22px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .row input {
    flex: 1;
    max-width: 240px;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: 12px;
  }
  .section {
    border-top: 1px solid var(--line);
    padding-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }
  .section-head {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
    max-height: 320px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }
</style>
