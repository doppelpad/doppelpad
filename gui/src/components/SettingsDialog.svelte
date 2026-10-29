<script module lang="ts">
  import { FolderOpen, Gamepad2, HardDrive, Monitor, Network, Shuffle, SlidersHorizontal, Zap } from '@lucide/svelte';

  /** The pages of the settings, in the order of the list on the left. */
  const PAGES = [
    { id: 'general', icon: SlidersHorizontal },
    { id: 'controllers', icon: Gamepad2 },
    { id: 'hotkeys', icon: Zap },
    { id: 'display', icon: Monitor },
    { id: 'remapping', icon: Shuffle },
    { id: 'connections', icon: Network },
    { id: 'drivers', icon: HardDrive },
    { id: 'files', icon: FolderOpen },
  ] as const;
  type Page = (typeof PAGES)[number]['id'];

  /** The settings open on the page shown last while the app runs. */
  let lastPage: Page = 'general';
</script>

<script lang="ts">
  // The app's settings: a list of pages on the left, the chosen page on the right.
  import { getVersion } from '@tauri-apps/api/app';
  import { api } from '../lib/api';
  import { LANGUAGES, has, t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import { platform } from '../lib/state/window.svelte';
  import { ZOOM_MAX, ZOOM_MIN, ZOOM_STEP } from '../lib/state/shortcuts';
  import type { RewasdChoice } from '../lib/types';
  import ApiSettings from './settings/ApiSettings.svelte';
  import DeviceSettings from './settings/DeviceSettings.svelte';
  import { driverStates } from './settings/DriverList.svelte';
  import DriverSettings from './settings/DriverSettings.svelte';
  import DsuSettings from './settings/DsuSettings.svelte';
  import HotkeySettings from './settings/HotkeySettings.svelte';
  import LibraryLocation from './settings/LibraryLocation.svelte';
  import OsdSettings from './settings/OsdSettings.svelte';
  import OverlaySettings from './settings/OverlaySettings.svelte';
  import PathRow from './settings/PathRow.svelte';
  import PlayerLightSettings from './settings/PlayerLightSettings.svelte';
  import RuntimeSettings from './settings/RuntimeSettings.svelte';
  import SettingRow from './settings/SettingRow.svelte';
  import SettingsSection from './settings/SettingsSection.svelte';
  import Dialog from './ui/Dialog.svelte';
  import PageTabs from './ui/PageTabs.svelte';
  import Select from './ui/Select.svelte';
  import Toggle from './ui/Toggle.svelte';

  /** The window sizes offered, in percent. */
  const ZOOM_CHOICES = Array.from({ length: (ZOOM_MAX - ZOOM_MIN) / ZOOM_STEP + 1 }, (_, i) => ZOOM_MIN + i * ZOOM_STEP);

  let { onclose }: { onclose: () => void } = $props();

  let page = $state<Page>(lastPage);
  /** The drivers page carries a warning dot while a driver is missing. */
  const driverMissing = $derived(driverStates(app.live).some((d) => !d.state.available));
  const pages = $derived(
    PAGES.map((p) => ({
      ...p,
      label: t(`settings.pages.${p.id}`),
      warning: p.id === 'drivers' && driverMissing ? t('drivers.missing') : undefined,
    })),
  );

  let logFile = $state('');
  api.logFile().then((f) => (logFile = f));
  let version = $state('');
  getVersion().then((v) => (version = v));
  /** Read from Windows each time the dialog opens: Task Manager can change it too. */
  let autostart = $state<boolean | null>(null);
  api.autostartGet().then((on) => (autostart = on));

  function show(p: Page) {
    page = p;
    lastPage = p;
  }

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

<Dialog title={t('settings.title')} {onclose} width={860} gap={14}>
  <PageTabs {pages} current={page} onchange={show} label={t('settings.title')} scroll>
    {#if page === 'general'}
      <SettingRow label={t('settings.language')}>
        <Select
          value={app.settings.language}
          class="setting"
          options={Object.entries(LANGUAGES).map(([code, name]) => ({ value: code, label: name }))}
          onchange={(language) => app.setSettings({ language })}
        />
      </SettingRow>
      {#if autostart !== null}
        <Toggle
          checked={autostart}
          label={t(platform === 'macos' ? 'settings.autostart_macos' : 'settings.autostart')}
          onchange={setAutostart}
        />
      {/if}
      <Toggle checked={app.settings.stay_live} label={t('settings.stay_live')} onchange={(v) => app.setSettings({ stay_live: v })} />
      <Toggle
        checked={app.settings.hook_buttons}
        label={t('settings.hook_buttons')}
        disabled={app.settings.pad_navigation}
        onchange={(v) => app.setSettings({ hook_buttons: v })}
      />
      <Toggle checked={app.settings.pad_navigation} label={t('padnav.toggle')} onchange={(v) => app.setPadNavigation(v)} />
      <Toggle
        checked={app.settings.tray_per_controller}
        label={t('settings.tray_per_controller')}
        onchange={(v) => app.setSettings({ tray_per_controller: v })}
      />
      <p class="hint">{t('padnav.hint')}</p>
      {#if app.rewasd}
        <SettingRow label={t('settings.rewasd')}>
          <Select
            value={app.settings.rewasd_at_start}
            class="setting"
            options={(['ask', 'quit', 'ignore'] as const).map((c) => ({ value: c, label: t(`settings.rewasd_${c}`) }))}
            onchange={(c: RewasdChoice) => app.setSettings({ rewasd_at_start: c })}
          />
        </SettingRow>
        {#if app.rewasdRunning}
          <div class="running">
            <span class="note">{t('settings.rewasd_running')}</span>
            <button class="btn" disabled={app.rewasdQuitting} onclick={() => app.quitRewasd()}>{t('rewasd.quit')}</button>
          </div>
        {/if}
      {/if}
      <SettingsSection title={t('updates.section')} hint={t('updates.hint')}>
        <Toggle checked={app.settings.check_updates} label={t('updates.auto')} onchange={(v) => app.setSettings({ check_updates: v })} />
        <div class="running">
          <span>{t('updates.version', { version })}</span>
          <button class="btn" disabled={app.updater.busy} onclick={() => app.updater.check(true)}>
            {#if app.updater.progress !== null}
              {t('updates.downloading_percent', { percent: app.updater.progress })}
            {:else if app.updater.checking}
              {t('updates.checking')}
            {:else}
              {t('updates.check_now')}
            {/if}
          </button>
        </div>
      </SettingsSection>
      <SettingsSection title={t('questions.title')}>
        {#each app.settings.silenced as q (q)}
          <div class="running">
            <span>{has(`questions.ids.${q}`) ? t(`questions.ids.${q}`) : q}</span>
            <button class="btn" onclick={() => app.silence(q, false)}>{t('questions.ask_again')}</button>
          </div>
        {:else}
          <p class="hint">{t('questions.none')}</p>
        {/each}
      </SettingsSection>
      <SettingsSection title={t('welcome.settings_title')}>
        <button class="btn welcome" onclick={showWelcome}>{t('welcome.again')}</button>
      </SettingsSection>
    {:else if page === 'controllers'}
      <DeviceSettings />
      <PlayerLightSettings />
    {:else if page === 'hotkeys'}
      <HotkeySettings />
    {:else if page === 'display'}
      <SettingRow label={t('settings.zoom')}>
        <Select
          value={app.settings.zoom}
          class="setting"
          options={ZOOM_CHOICES.map((z) => ({ value: z, label: `${z} %` }))}
          onchange={(zoom) => app.setSettings({ zoom })}
        />
      </SettingRow>
      <OsdSettings />
      <OverlaySettings />
    {:else if page === 'remapping'}
      <RuntimeSettings />
    {:else if page === 'connections'}
      <DsuSettings />
      <ApiSettings />
    {:else if page === 'drivers'}
      <DriverSettings />
    {:else}
      <LibraryLocation />
      <SettingsSection title={t('settings.log_file')}>
        <PathRow path={logFile} revealLabel={t('settings.open_log_folder')} />
      </SettingsSection>
    {/if}
  </PageTabs>
</Dialog>

<style>
  .welcome {
    align-self: flex-start;
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
