<script lang="ts">
  // Settings of one shift layer: its name and which of Main's mappings it takes over.
  import { X } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { comboLabel } from '../lib/model/labels';
  import {
    comboKey,
    inheritableTriggers,
    layerInheritance,
    layerUnlock,
    renameLayer,
    sameCombo,
    setLayerInherit,
    setLayerUnlock,
    setTriggerInherited,
  } from '../lib/model/profile';
  import { app } from '../lib/state/app.svelte';
  import type { Combo, Unlock } from '../lib/types';
  import Toggle from './ui/Toggle.svelte';

  let { layer, onclose }: { layer: number; onclose: () => void } = $props();

  const inheritance = $derived(app.profile ? layerInheritance(app.profile, layer) : { inherit: true, notInherited: [] });
  const unlock = $derived(app.profile ? layerUnlock(app.profile, layer) : undefined);
  const UNLOCKS = ['none', 'all', 'keyboard', 'mouse'] as const;
  const triggers = $derived(app.profile ? inheritableTriggers(app.profile) : []);

  const label = (c: Combo) => comboLabel(c, app.family);
  const inherited = (c: Combo) => !inheritance.notInherited.some((x) => sameCombo(x, c));

  function rename(name: string) {
    if (name.trim()) app.edit((p) => renameLayer(p, layer, name.trim()));
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

    <label class="row">
      <span>{t('layers.unlock')}</span>
      <select
        value={unlock ?? 'none'}
        disabled={app.readOnly}
        onchange={(e) => {
          const v = e.currentTarget.value;
          app.edit((p) => setLayerUnlock(p, layer, v === 'none' ? undefined : (v as Unlock)));
        }}
      >
        {#each UNLOCKS as u (u)}<option value={u}>{t(`layers.unlock_options.${u}`)}</option>{/each}
      </select>
    </label>
    {#if unlock}<p class="hint">{t('layers.unlock_hint')}</p>{/if}

    <Toggle
      checked={inheritance.inherit}
      disabled={app.readOnly}
      label={t('layers.inherit')}
      onchange={(v) => app.edit((p) => setLayerInherit(p, layer, v))}
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
                    app.edit((p) => setTriggerInherited(p, layer, c, e.currentTarget.checked))}
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
    background: var(--backdrop);
    display: grid;
    place-items: center;
    z-index: var(--z-dialog);
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
  .row input,
  .row select {
    flex: 1;
    max-width: 240px;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
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
    font-size: var(--fs-xs);
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
