<script lang="ts">
  import { CircleCheck, CircleAlert, FolderOpen, RefreshCw, X } from '@lucide/svelte';
  import { api, errorText } from '../lib/api';
  import { LANGUAGES, t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import type { RewasdChoice } from '../lib/types';
  import HotkeySettings from './HotkeySettings.svelte';
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
    try {
      await api.autostartSet(on);
    } catch (e) {
      app.toast(errorText(e), 'error');
    }
    autostart = await api.autostartGet();
  }

  const drivers = $derived(
    app.live
      ? [
          { key: 'virtual_pads', state: app.live.drivers.virtual_pads },
          { key: 'device_hiding', state: app.live.drivers.device_hiding },
        ]
      : [],
  );

  async function refresh() {
    try {
      await api.refreshDrivers();
    } catch (e) {
      app.toast(errorText(e), 'error');
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="backdrop" role="presentation" onclick={onclose}>
  <div class="dialog" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <div class="head">
      <h3>{t('settings.title')}</h3>
      <button class="icon-btn" onclick={onclose} title={t('common.close')}><X size={17} /></button>
    </div>

    <label class="row">
      <span>{t('settings.language')}</span>
      <select value={app.settings.language} onchange={(e) => app.setSettings({ language: e.currentTarget.value })}>
        {#each Object.entries(LANGUAGES) as [code, name] (code)}
          <option value={code}>{name}</option>
        {/each}
      </select>
    </label>

    <div class="row">
      <Toggle
        checked={app.settings.stay_live}
        label={t('settings.stay_live')}
        onchange={(v) => app.setSettings({ stay_live: v })}
      />
    </div>
    <div class="row">
      <Toggle
        checked={app.settings.hook_buttons}
        label={t('settings.hook_buttons')}
        onchange={(v) => app.setSettings({ hook_buttons: v })}
      />
    </div>
    {#if autostart !== null}
      <div class="row">
        <Toggle checked={autostart} label={t('settings.autostart')} onchange={setAutostart} />
      </div>
    {/if}

    {#if app.rewasd}
      <label class="row">
        <span>{t('settings.rewasd')}</span>
        <select
          value={app.settings.rewasd_at_start}
          onchange={(e) => app.setSettings({ rewasd_at_start: e.currentTarget.value as RewasdChoice })}
        >
          <option value="ask">{t('settings.rewasd_ask')}</option>
          <option value="quit">{t('settings.rewasd_quit')}</option>
          <option value="ignore">{t('settings.rewasd_ignore')}</option>
        </select>
      </label>
      {#if app.rewasdRunning}
        <div class="row">
          <span class="note">{t('settings.rewasd_running')}</span>
          <button class="btn" disabled={app.rewasdQuitting} onclick={() => app.quitRewasd()}>{t('rewasd.quit')}</button>
        </div>
      {/if}
    {/if}

    <HotkeySettings />

    <div class="section">
      <div class="section-head">
        <span>{t('settings.drivers')}</span>
        <button class="icon-btn" title={t('settings.recheck')} onclick={refresh}><RefreshCw size={15} /></button>
      </div>
      {#each drivers as d (d.key)}
        <div class="driver" class:missing={!d.state.available} title={d.state.error ?? ''}>
          {#if d.state.available}<CircleCheck size={15} />{:else}<CircleAlert size={15} />{/if}
          <span>{t(`drivers.${d.key}`)}</span>
          {#if !d.state.available}<span class="state">{t('drivers.missing')}</span>{/if}
        </div>
      {/each}
    </div>

    <div class="section">
      <div class="section-head"><span>{t('settings.profiles_dir')}</span></div>
      <div class="dir">
        <code>{profilesDir}</code>
        <button class="icon-btn" title={t('common.reveal')} onclick={() => profilesDir && api.reveal(profilesDir)}>
          <FolderOpen size={15} />
        </button>
      </div>
    </div>

    <div class="section">
      <div class="section-head"><span>{t('settings.log_file')}</span></div>
      <div class="dir">
        <code>{logFile}</code>
        <button class="icon-btn" title={t('settings.open_log_folder')} onclick={() => logFile && api.reveal(logFile)}>
          <FolderOpen size={15} />
        </button>
      </div>
    </div>
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
    width: 520px;
    max-width: calc(100vw - 32px);
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    box-shadow: var(--shadow);
    padding: 18px 20px 22px;
    max-height: calc(100vh - 48px);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
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
  .row select {
    min-width: 160px;
  }
  .note {
    color: var(--warning);
  }
  .section {
    border-top: 1px solid var(--line);
    padding-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .driver {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }
  .driver :global(svg) {
    color: var(--ok);
  }
  .driver.missing :global(svg) {
    color: var(--warning);
  }
  .state {
    margin-left: auto;
    color: var(--warning);
    font-size: 12px;
  }
  .dir {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  code {
    flex: 1;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-dim);
    background: var(--bg-2);
    border-radius: var(--radius-s);
    padding: 6px 8px;
    overflow-wrap: anywhere;
    user-select: text;
  }
</style>
