<script lang="ts">
  import { api } from '../lib/api';
  import { LANGUAGES, t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import type { RewasdChoice } from '../lib/types';
  import ApiSettings from './settings/ApiSettings.svelte';
  import DeviceSettings from './settings/DeviceSettings.svelte';
  import DriverSettings from './settings/DriverSettings.svelte';
  import DsuSettings from './settings/DsuSettings.svelte';
  import HotkeySettings from './settings/HotkeySettings.svelte';
  import OsdSettings from './settings/OsdSettings.svelte';
  import OverlaySettings from './settings/OverlaySettings.svelte';
  import PathRow from './settings/PathRow.svelte';
  import PlayerLightSettings from './settings/PlayerLightSettings.svelte';
  import RuntimeSettings from './settings/RuntimeSettings.svelte';
  import SettingRow from './settings/SettingRow.svelte';
  import SettingsSection from './settings/SettingsSection.svelte';
  import Dialog from './ui/Dialog.svelte';
  import Toggle from './ui/Toggle.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let profilesDir = $state('');
  api.profilesDir().then((d) => (profilesDir = d));
  let logFile = $state('');
  api.logFile().then((f) => (logFile = f));
  /** Read from Windows each time the dialog opens: Task Manager can change it too. */
  let autostart = $state<boolean | null>(null);
  api.autostartGet().then((on) => (autostart = on));

  async function setAutostart(on: boolean) {
    await app.attempt(api.autostartSet(on));
    autostart = await api.autostartGet();
  }

  /** Close the settings and start the welcome guide again. */
  function showWelcome() {
    app.setSettings({ welcome_done: false });
    onclose();
  }
</script>

<Dialog title={t('settings.title')} {onclose} width={520} gap={14} scroll>
  <SettingRow label={t('settings.language')}>
    <select value={app.settings.language} onchange={(e) => app.setSettings({ language: e.currentTarget.value })}>
      {#each Object.entries(LANGUAGES) as [code, name] (code)}
        <option value={code}>{name}</option>
      {/each}
    </select>
  </SettingRow>

  <Toggle checked={app.settings.stay_live} label={t('settings.stay_live')} onchange={(v) => app.setSettings({ stay_live: v })} />
  <Toggle checked={app.settings.hook_buttons} label={t('settings.hook_buttons')} onchange={(v) => app.setSettings({ hook_buttons: v })} />
  {#if autostart !== null}
    <Toggle checked={autostart} label={t('settings.autostart')} onchange={setAutostart} />
  {/if}

  {#if app.rewasd}
    <SettingRow label={t('settings.rewasd')}>
      <select
        value={app.settings.rewasd_at_start}
        onchange={(e) => app.setSettings({ rewasd_at_start: e.currentTarget.value as RewasdChoice })}
      >
        <option value="ask">{t('settings.rewasd_ask')}</option>
        <option value="quit">{t('settings.rewasd_quit')}</option>
        <option value="ignore">{t('settings.rewasd_ignore')}</option>
      </select>
    </SettingRow>
    {#if app.rewasdRunning}
      <div class="running">
        <span class="note">{t('settings.rewasd_running')}</span>
        <button class="btn" disabled={app.rewasdQuitting} onclick={() => app.quitRewasd()}>{t('rewasd.quit')}</button>
      </div>
    {/if}
  {/if}

  <HotkeySettings />

  <OsdSettings />

  <SettingsSection title={t('welcome.settings_title')}>
    <button class="btn" onclick={showWelcome}>{t('welcome.again')}</button>
  </SettingsSection>

  <DeviceSettings />

  <RuntimeSettings />

  <DsuSettings />

  <ApiSettings />

  <PlayerLightSettings />

  <OverlaySettings />

  <DriverSettings />

  <SettingsSection title={t('settings.profiles_dir')}>
    <PathRow path={profilesDir} revealLabel={t('common.reveal')} />
  </SettingsSection>

  <SettingsSection title={t('settings.log_file')}>
    <PathRow path={logFile} revealLabel={t('settings.open_log_folder')} />
  </SettingsSection>
</Dialog>

<style>
  select {
    min-width: 160px;
  }
  .running {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .note {
    color: var(--warning);
  }
</style>
