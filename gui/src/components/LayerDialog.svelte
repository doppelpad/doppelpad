<script lang="ts">
  // Settings of one shift layer: its name and which of Main's mappings it takes over.
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
  import SettingRow from './settings/SettingRow.svelte';
  import SettingsSection from './settings/SettingsSection.svelte';
  import Dialog from './ui/Dialog.svelte';
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

<Dialog title={t('layers.settings_title', { layer: app.layerLabel(layer) })} {onclose}>

  <SettingRow label={t('layers.name_prompt')}>
    <input value={app.layerLabel(layer)} disabled={app.readOnly} onchange={(e) => rename(e.currentTarget.value)} />
  </SettingRow>

  <SettingRow label={t('layers.unlock')}>
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
  </SettingRow>
  {#if unlock}<p class="hint">{t('layers.unlock_hint')}</p>{/if}

  <Toggle
    checked={inheritance.inherit}
    disabled={app.readOnly}
    label={t('layers.inherit')}
    onchange={(v) => app.edit((p) => setLayerInherit(p, layer, v))}
  />
  <p class="hint">{t(inheritance.inherit ? 'layers.inherit_hint_on' : 'layers.inherit_hint_off')}</p>

  {#if inheritance.inherit}
    <SettingsSection title={t('layers.inherited_list')} shrink>
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
    </SettingsSection>
  {/if}
</Dialog>

<style>
  input:not([type='checkbox']),
  select {
    flex: 1;
    max-width: 240px;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
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
