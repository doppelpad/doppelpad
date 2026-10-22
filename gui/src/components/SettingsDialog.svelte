<script lang="ts">
  import { CircleCheck, CircleAlert, FolderOpen, RefreshCw } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { LANGUAGES, t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import type { Corner, MappingsOverlaySettings, OsdSettings, PadOverlaySettings, PlayerLights, RewasdChoice } from '../lib/types';
  import DeviceSettings from './settings/DeviceSettings.svelte';
  import ApiSettings from './settings/ApiSettings.svelte';
  import DsuSettings from './settings/DsuSettings.svelte';
  import RuntimeSettings from './settings/RuntimeSettings.svelte';
  import HotkeySettings from './settings/HotkeySettings.svelte';
  import SettingRow from './settings/SettingRow.svelte';
  import SettingsSection from './settings/SettingsSection.svelte';
  import Dialog from './ui/Dialog.svelte';
  import Slider from './ui/Slider.svelte';
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

  const drivers = $derived(
    app.live
      ? [
          { key: 'virtual_pads', state: app.live.drivers.virtual_pads },
          { key: 'device_hiding', state: app.live.drivers.device_hiding },
        ]
      : [],
  );

  const osd = $derived(app.settings.osd);
  const OSD_EVENTS = ['remap', 'slot', 'layer', 'controller', 'battery'] as const;
  const CORNERS: Corner[] = ['top_left', 'top_right', 'bottom_left', 'bottom_right'];
  let monitors = $state<string[]>([]);
  api.osdMonitors().then((m) => (monitors = m));

  function setOsd(patch: Partial<OsdSettings>) {
    app.setSettings({ osd: { ...osd, ...patch } });
  }

  const PLAYER_LIGHTS: PlayerLights[] = ['while_remapping', 'always', 'off'];

  const setLights = (player: PlayerLights, mic: boolean) => app.attempt(api.setLights(player, mic));

  const padOverlay = $derived(app.settings.pad_overlay);
  const mappingsOverlay = $derived(app.settings.mappings_overlay);
  const PAD_SIZES = [50, 70, 90] as const;

  function setPadOverlay(patch: Partial<PadOverlaySettings>) {
    app.setSettings({ pad_overlay: { ...padOverlay, ...patch } });
  }

  function setMappingsOverlay(patch: Partial<MappingsOverlaySettings>) {
    app.setSettings({ mappings_overlay: { ...mappingsOverlay, ...patch } });
  }

  const refresh = () => app.attempt(api.refreshDrivers());
</script>

<Dialog title={t('settings.title')} {onclose} width={520} gap={14} scroll>

  <SettingRow label={t('settings.language')}>
    <select value={app.settings.language} onchange={(e) => app.setSettings({ language: e.currentTarget.value })}>
      {#each Object.entries(LANGUAGES) as [code, name] (code)}
        <option value={code}>{name}</option>
      {/each}
    </select>
  </SettingRow>

  <Toggle
    checked={app.settings.stay_live}
    label={t('settings.stay_live')}
    onchange={(v) => app.setSettings({ stay_live: v })}
  />
  <Toggle
    checked={app.settings.hook_buttons}
    label={t('settings.hook_buttons')}
    onchange={(v) => app.setSettings({ hook_buttons: v })}
  />
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

  <SettingsSection title={t('settings.osd.title')} hint={t('settings.osd.hint')}>
    {#snippet actions()}
      <button class="btn" disabled={!osd.enabled} onclick={() => app.attempt(api.osdTest())}>{t('settings.osd.test')}</button>
    {/snippet}
    <Toggle checked={osd.enabled} label={t('settings.osd.enabled')} onchange={(v) => setOsd({ enabled: v })} />
    {#if osd.enabled}
      <div class="events">
        {#each OSD_EVENTS as e (e)}
          <label class="check">
            <input type="checkbox" checked={osd[e]} onchange={(ev) => setOsd({ [e]: ev.currentTarget.checked })} />
            <span>{t(`settings.osd.${e}`)}</span>
          </label>
          {#if e === 'layer'}
            <label class="check sub">
              <input
                type="checkbox"
                checked={osd.keep_layer}
                disabled={!osd.layer}
                onchange={(ev) => setOsd({ keep_layer: ev.currentTarget.checked })}
              />
              <span>{t('settings.osd.keep_layer')}</span>
            </label>
          {/if}
        {/each}
      </div>
      <SettingRow label={t('settings.osd.monitor')}>
        <select value={osd.monitor} onchange={(e) => setOsd({ monitor: Number(e.currentTarget.value) })}>
          {#each monitors as size, i (i)}
            <option value={i}>{i === 0 ? t('settings.osd.primary', { size }) : t('settings.osd.monitor_n', { n: i + 1, size })}</option>
          {/each}
        </select>
      </SettingRow>
      <SettingRow label={t('settings.osd.corner')}>
        <select value={osd.corner} onchange={(e) => setOsd({ corner: e.currentTarget.value as Corner })}>
          {#each CORNERS as c (c)}<option value={c}>{t(`settings.osd.corners.${c}`)}</option>{/each}
        </select>
      </SettingRow>
      <Slider label={t('settings.osd.scale')} value={osd.scale} def={100} min={50} max={200} step={10} unit="%" onchange={(v) => setOsd({ scale: v })} />
      <Slider label={t('settings.osd.opacity')} value={osd.opacity} def={80} min={20} max={100} step={5} unit="%" onchange={(v) => setOsd({ opacity: v })} />
      <Slider label={t('settings.osd.seconds')} value={osd.seconds} def={5} min={1} max={30} step={1} unit="s" onchange={(v) => setOsd({ seconds: v })} />
    {/if}
  </SettingsSection>

  <SettingsSection title={t('welcome.settings_title')}>
    <button class="btn" onclick={showWelcome}>{t('welcome.again')}</button>
  </SettingsSection>

  <DeviceSettings />

  <RuntimeSettings />

  <DsuSettings />

  <ApiSettings />

  {#if app.live}
    {@const live = app.live}
    <SettingsSection title={t('settings.lights.title')}>
      <SettingRow label={t('settings.lights.player')}>
        <select value={live.player_lights} onchange={(e) => setLights(e.currentTarget.value as PlayerLights, live.mic_led)}>
          {#each PLAYER_LIGHTS as p (p)}<option value={p}>{t(`settings.lights.players.${p}`)}</option>{/each}
        </select>
      </SettingRow>
      <Toggle checked={live.mic_led} label={t('settings.lights.mic')} onchange={(v) => setLights(live.player_lights, v)} />
    </SettingsSection>
  {/if}

  <SettingsSection title={t('settings.overlays.title')} hint={t('settings.overlays.hint')}>
    <div class="sub">{t('settings.overlays.pad')}</div>
    <SettingRow label={t('settings.osd.corner')}>
      <select value={padOverlay.corner} onchange={(e) => setPadOverlay({ corner: e.currentTarget.value as Corner })}>
        {#each CORNERS as c (c)}<option value={c}>{t(`settings.osd.corners.${c}`)}</option>{/each}
      </select>
    </SettingRow>
    <SettingRow label={t('settings.osd.scale')}>
      <select value={padOverlay.size} onchange={(e) => setPadOverlay({ size: Number(e.currentTarget.value) })}>
        {#each PAD_SIZES as s (s)}<option value={s}>{t(`settings.overlays.sizes.${s}`)}</option>{/each}
      </select>
    </SettingRow>
    <Slider label={t('settings.osd.opacity')} value={padOverlay.opacity} def={90} min={20} max={100} step={5} unit="%" onchange={(v) => setPadOverlay({ opacity: v })} />
    <Slider
      label={t('settings.overlays.hide_after')}
      value={padOverlay.hide_after}
      def={5}
      min={0}
      max={30}
      step={1}
      unit="s"
      hint={t('settings.overlays.hide_after_hint')}
      onchange={(v) => setPadOverlay({ hide_after: v })}
    />
    <div class="sub">{t('settings.overlays.mappings')}</div>
    <SettingRow label={t('settings.osd.corner')}>
      <select value={mappingsOverlay.corner} onchange={(e) => setMappingsOverlay({ corner: e.currentTarget.value as Corner })}>
        {#each CORNERS as c (c)}<option value={c}>{t(`settings.osd.corners.${c}`)}</option>{/each}
      </select>
    </SettingRow>
    <Slider label={t('settings.osd.opacity')} value={mappingsOverlay.opacity} def={90} min={20} max={100} step={5} unit="%" onchange={(v) => setMappingsOverlay({ opacity: v })} />
  </SettingsSection>

  <SettingsSection title={t('settings.drivers')}>
    {#snippet actions()}
      <button class="icon-btn" title={t('settings.recheck')} onclick={refresh}><RefreshCw size={15} /></button>
    {/snippet}
    {#each drivers as d (d.key)}
      <div class="driver" class:missing={!d.state.available} title={d.state.error ?? ''}>
        {#if d.state.available}<CircleCheck size={15} />{:else}<CircleAlert size={15} />{/if}
        <span>{t(`drivers.${d.key}`)}</span>
        {#if !d.state.available}<span class="state">{t('drivers.missing')}</span>{/if}
      </div>
    {/each}
  </SettingsSection>

  <SettingsSection title={t('settings.profiles_dir')}>
    <div class="dir">
      <code>{profilesDir}</code>
      <button class="icon-btn" title={t('common.reveal')} onclick={() => profilesDir && app.attempt(api.reveal(profilesDir))}>
        <FolderOpen size={15} />
      </button>
    </div>
  </SettingsSection>

  <SettingsSection title={t('settings.log_file')}>
    <div class="dir">
      <code>{logFile}</code>
      <button class="icon-btn" title={t('settings.open_log_folder')} onclick={() => logFile && app.attempt(api.reveal(logFile))}>
        <FolderOpen size={15} />
      </button>
    </div>
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
  .events {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .check.sub {
    margin-left: 24px;
  }
  .sub {
    margin-top: 4px;
    color: var(--text-dim);
    font-weight: 600;
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
    font-size: var(--fs-s);
  }
  .dir {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  code {
    flex: 1;
    font-family: var(--mono);
    font-size: var(--fs-s);
    color: var(--text-dim);
    background: var(--bg-2);
    border-radius: var(--radius-s);
    padding: 6px 8px;
    overflow-wrap: anywhere;
    user-select: text;
  }
</style>
