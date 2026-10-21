// Typed wrappers around the Rust commands in gui/src-tauri/src/commands.rs.

import { Channel, invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { t } from './i18n/index.svelte';
import type {
  Catalog,
  DiagMsg,
  DsuSettings,
  DeviceGroup,
  Game,
  GameMeta,
  GenericLayout,
  GuiSettings,
  Hotkeys,
  HttpSettings,
  LayerPreset,
  Library,
  LiveState,
  LoadedConfig,
  PlayerLights,
  PresetEntry,
  Profile,
  RewasdStatus,
  RuntimeOptions,
  Template,
} from './types';

export const api = {
  catalog: () => invoke<Catalog>('catalog'),

  libraryScan: () => invoke<Library>('library_scan'),
  gameCreate: (name: string) => invoke<Game>('game_create', { name }),
  gameUpdate: (dir: string, meta: GameMeta) => invoke<void>('game_update', { dir, meta }),
  gameDelete: (dir: string) => invoke<void>('game_delete', { dir }),
  gameCreateTemplate: (name: string, config: string, template: Template) =>
    invoke<Game>('game_create_template', { name, config, template }),
  configFromTemplate: (gameDir: string, name: string, template: Template) =>
    invoke<string>('config_from_template', { gameDir, name, template }),
  openDriverPage: (driver: string) => invoke<void>('open_driver_page', { driver }),
  gameExport: (dir: string, out: string) => invoke<void>('game_export', { dir, out }),
  gameImport: (file: string) => invoke<Game>('game_import', { file }),
  presetList: () => invoke<PresetEntry[]>('preset_list'),
  presetSave: (preset: LayerPreset) => invoke<string>('preset_save', { preset }),
  presetLoad: (path: string) => invoke<LayerPreset>('preset_load', { path }),
  presetDelete: (path: string) => invoke<void>('preset_delete', { path }),
  sheetSave: (path: string, html: string) => invoke<void>('sheet_save', { path, html }),

  configLoad: (path: string) => invoke<LoadedConfig>('config_load', { path }),
  configSave: (path: string, profile: Profile) => invoke<DiagMsg[]>('config_save', { path, profile }),
  configCreate: (gameDir: string, name: string) => invoke<string>('config_create', { gameDir, name }),
  configDuplicate: (path: string) => invoke<string>('config_duplicate', { path }),
  configRename: (path: string, name: string) => invoke<void>('config_rename', { path, name }),
  configDelete: (path: string) => invoke<void>('config_delete', { path }),
  configImport: (gameDir: string, source: string) =>
    invoke<{ path: string; diagnostics: DiagMsg[] }>('config_import', { gameDir, source }),
  reveal: (path: string) => invoke<void>('reveal', { path }),

  /** `device`: that controller's own slots instead of the shared ones. */
  applyToSlot: (slot: number, path: string, device: string | null = null) =>
    invoke<string>('apply_to_slot', { slot, path, device }),
  selectSlot: (slot: number, device: string | null = null) => invoke<string>('select_slot', { slot, device }),
  clearSlot: (slot: number, device: string | null = null) => invoke<string>('clear_slot', { slot, device }),
  /** Give a controller slots of its own, or let it use the shared ones again. */
  ownSlots: (device: string, on: boolean) => invoke<string>('own_slots', { device, on }),
  setRemap: (on: boolean) => invoke<string>('set_remap', { on }),
  setAutoSwitch: (on: boolean) => invoke<string>('set_auto_switch', { on }),
  setPreferredDevice: (id: string | null) => invoke<string>('set_preferred_device', { id }),
  refreshDrivers: () => invoke<string>('refresh_drivers'),
  hotkeysGet: () => invoke<Hotkeys>('hotkeys_get'),
  hotkeysSet: (hotkeys: Hotkeys) => invoke<string>('hotkeys_set', { hotkeys }),
  /** Pause the hotkeys while recording a new one. */
  hotkeysPause: (on: boolean) => invoke<string>('hotkeys_pause', { on }),
  /** One full in-game turn of `counts` mouse counts, 3 s from now (calibration). */
  testTurn: (counts: number) => invoke<string>('test_turn', { counts }),
  /** Gyro calibration of a controller (`null`: the controller in use). */
  gyroCalibrate: (device: string | null) => invoke<string>('gyro_calibrate', { device }),
  gyroAuto: (device: string | null, on: boolean) => invoke<string>('gyro_auto', { device, on }),
  gyroReset: (device: string | null) => invoke<string>('gyro_reset', { device }),
  /** `null` when reWASD is not installed. */
  rewasdStatus: () => invoke<RewasdStatus | null>('rewasd_status'),
  /** `false` when the user declined the administrator prompt. */
  rewasdQuit: () => invoke<boolean>('rewasd_quit'),

  /** Receive live snapshots until the page reloads or another subscription replaces it. */
  liveSubscribe(onState: (s: LiveState) => void) {
    const channel = new Channel<LiveState>();
    channel.onmessage = onState;
    return invoke<void>('live_subscribe', { onEvent: channel });
  },

  settingsGet: () => invoke<GuiSettings>('settings_get'),
  settingsSet: (settings: GuiSettings) => invoke<void>('settings_set', { settings }),
  /** Show a sample on-screen notification. */
  /** What the player lights and the microphone LED show. */
  setLights: (player: PlayerLights, mic: boolean) => invoke<string>('set_lights', { player, mic }),
  /** Ignore a device completely, or use it again. */
  setBlocked: (id: string, blocked: boolean) => invoke<string>('set_blocked', { id, blocked }),
  setGroups: (groups: DeviceGroup[]) => invoke<string>('set_groups', { groups }),
  setPowerOffAfter: (minutes: number) => invoke<string>('set_power_off_after', { minutes }),
  powerOff: (id: string) => invoke<string>('power_off', { id }),
  setLayout: (layout: GenericLayout) => invoke<string>('set_layout', { layout }),
  forgetLayout: (device: string) => invoke<string>('forget_layout', { device }),
  setDsu: (settings: DsuSettings) => invoke<string>('set_dsu', { settings }),
  setHttp: (settings: HttpSettings) => invoke<string>('set_http', { settings }),
  newHttpToken: () => invoke<string>('new_http_token'),
  setRuntimeOptions: (options: RuntimeOptions) => invoke<string>('set_runtime_options', { options }),
  osdTest: () => invoke<void>('osd_test'),
  /** Sizes of the monitors, the primary one first. */
  osdMonitors: () => invoke<string[]>('osd_monitors'),
  /** Start hidden in the tray when the user signs in; `null` where unsupported (not Windows). */
  autostartGet: () => invoke<boolean | null>('autostart_get'),
  autostartSet: (on: boolean) => invoke<void>('autostart_set', { on }),
  /** Show the window, e.g. when the app started hidden and needs to ask something. */
  showWindow: () => invoke<void>('window_show'),
  profilesDir: () => invoke<string>('profiles_dir'),
  logFile: () => invoke<string>('log_file'),
  takeNotices: () => invoke<string[]>('take_notices'),
};

/** Error text from a rejected command. */
export function errorText(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/**
 * Ask a question with OK and Cancel in the system's dialog; `true` for OK. The app cannot use
 * `window.confirm`: in the webview it returns a promise without asking.
 */
export function confirmAsk(message: string): Promise<boolean> {
  return ask(message, { kind: 'warning', okLabel: t('common.ok'), cancelLabel: t('common.cancel') });
}
