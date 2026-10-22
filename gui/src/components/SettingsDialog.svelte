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
  import Toggle from './ui/Toggle.svelte';

  /** The window sizes offered, in percent. */
  const ZOOM_CHOICES = Array.from({ length: (ZOOM_MAX - ZOOM_MIN) / ZOOM_STEP + 1 }, (_, i) => ZOOM_MIN + i * ZOOM_STEP);

  let { onclose }: { onclose: () => void } = $props();

  const id = $props.id();
  let page = $state<Page>(lastPage);
  /** The drivers page carries a warning dot while a driver is missing. */
  const driverMissing = $derived(driverStates(app.live).some((d) => !d.state.available));

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

  /** Up and down (Home, End) move through the pages, as in any vertical tab list. */
  function onListKey(e: KeyboardEvent) {
    const i = PAGES.findIndex((p) => p.id === page);
    const to = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: PAGES.length - 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    const next = PAGES[(to + PAGES.length) % PAGES.length]!;
    show(next.id);
    document.getElementById(`${id}-tab-${next.id}`)?.focus();
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
  <div class="layout">
    <div class="pages" role="tablist" aria-orientation="vertical" tabindex="-1" onkeydown={onListKey}>
      {#each PAGES as p (p.id)}
        <button
          type="button"
          role="tab"
          id="{id}-tab-{p.id}"
          aria-selected={page === p.id}
          aria-controls="{id}-page"
          tabindex={page === p.id ? 0 : -1}
          class:on={page === p.id}
          onclick={() => show(p.id)}
        >
          <p.icon size={16} />
          <span class="name">{t(`settings.pages.${p.id}`)}</span>
          {#if p.id === 'drivers' && driverMissing}<span class="dot" title={t('drivers.missing')}></span>{/if}
        </button>
      {/each}
    </div>

    {#key page}
      <div class="page" role="tabpanel" id="{id}-page" aria-labelledby="{id}-tab-{page}" tabindex="-1">
        {#if page === 'general'}
          <SettingRow label={t('settings.language')}>
            <select value={app.settings.language} onchange={(e) => app.setSettings({ language: e.currentTarget.value })}>
              {#each Object.entries(LANGUAGES) as [code, name] (code)}
                <option value={code}>{name}</option>
              {/each}
            </select>
          </SettingRow>
          {#if autostart !== null}
            <Toggle checked={autostart} label={t('settings.autostart')} onchange={setAutostart} />
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
          <SettingsSection title={t('updates.section')} hint={t('updates.hint')}>
            <Toggle
              checked={app.settings.check_updates}
              label={t('updates.auto')}
              onchange={(v) => app.setSettings({ check_updates: v })}
            />
            <div class="running">
              <span>{t('updates.version', { version })}</span>
              <button class="btn" disabled={app.updateChecking || app.updateProgress !== null} onclick={() => app.checkForUpdate(true)}>
                {#if app.updateProgress !== null}
                  {t('updates.downloading_percent', { percent: app.updateProgress })}
                {:else if app.updateChecking}
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
            <select value={app.settings.zoom} onchange={(e) => app.setSettings({ zoom: Number(e.currentTarget.value) })}>
              {#each ZOOM_CHOICES as z (z)}
                <option value={z}>{z} %</option>
              {/each}
            </select>
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
      </div>
    {/key}
  </div>
</Dialog>

<style>
  /* The same height on every page, so the dialog does not jump when the page changes. */
  .layout {
    display: grid;
    grid-template-columns: 184px 1fr;
    gap: 20px;
    height: min(560px, calc(100vh - 150px));
  }
  .pages {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-right: 12px;
    border-right: 1px solid var(--line);
    outline: none;
  }
  .pages button {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: var(--radius-m);
    color: var(--text-dim);
    text-align: left;
  }
  .pages button:hover:not(.on) {
    color: var(--text);
    background: var(--bg-2);
  }
  .pages button.on {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .name {
    flex: 1;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--warning);
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    overflow-y: auto;
    padding-right: 6px;
    outline: none;
  }
  /* The page's first section needs no line above it. */
  .page > :global(.section:first-child) {
    border-top: none;
    padding-top: 0;
  }
  select {
    min-width: 160px;
  }
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
