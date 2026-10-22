<script lang="ts">
  // How remapping fits in with the rest of the system: sleep, focus, disconnects.
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import type { RuntimeOptions } from '../../lib/types';
  import Toggle from '../ui/Toggle.svelte';

  const options = $derived(app.live?.runtime ?? null);

  function set(patch: Partial<RuntimeOptions>) {
    if (options) app.attempt(api.setRuntimeOptions({ ...options, ...patch }));
  }

  const setAutoSwitch = (on: boolean) => app.attempt(api.setAutoSwitch(on));
</script>

{#if options}
  <div class="section">
    <div class="section-title"><span>{t('settings.runtime.title')}</span></div>
    <Toggle checked={app.live?.auto_switch ?? true} label={t('settings.runtime.auto_switch')} onchange={setAutoSwitch} />
    <p class="hint">{t('settings.runtime.auto_switch_hint')}</p>
    <Toggle checked={options.prevent_sleep} label={t('settings.runtime.prevent_sleep')} onchange={(v) => set({ prevent_sleep: v })} />
    <Toggle checked={options.pause_unfocused} label={t('settings.runtime.pause_unfocused')} onchange={(v) => set({ pause_unfocused: v })} />
    <p class="hint">{t('settings.runtime.pause_unfocused_hint')}</p>
    <Toggle checked={options.keep_offline_pads} label={t('settings.runtime.keep_offline_pads')} onchange={(v) => set({ keep_offline_pads: v })} />
    <p class="hint">{t('settings.runtime.locked_hint')}</p>
  </div>
{/if}

<style>
  .section {
    border-top: 1px solid var(--line);
    padding-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
</style>
