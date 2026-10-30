// Typed wrappers around the Rust commands in gui/src-tauri/src/commands/.

import { Channel, invoke } from '@tauri-apps/api/core';
import { has, t } from './i18n/index.svelte';
import type {
  BundleLaunches,
  Catalog,
  CmdError,
  ConfigVersion,
  DeviceGroup,
  DiagMsg,
  Driver,
  DsuSettings,
  Game,
  GameMeta,
  GenericLayout,
  GuiSettings,
  Hotkeys,
  HttpSettings,
  Imported,
  InstalledGame,
  Launch,
  LayerPreset,
  Library,
  LibraryMoved,
  LibraryPlace,
  LibraryTarget,
  LiveState,
  LoadedConfig,
  Notice,
  PlayerLights,
  PresetEntry,
  Profile,
  RewasdStatus,
  RunningProgram,
  RuntimeOptions,
  Template,
  UpdateInfo,
} from './types';

/** Events the app sends the main window: bundles opened from the file manager (`string[]`) and hints (`Hint`). */
export const EVENTS = { openFiles: 'open-files', hint: 'hint' } as const;

export const api = {
  catalog: () => invoke<Catalog>('catalog'),

  libraryScan: () => invoke<Library>('library_scan'),
  gameCreate: (name: string) => invoke<Game>('game_create', { name }),
  gameUpdate: (dir: string, meta: GameMeta) => invoke<void>('game_update', { dir, meta }),
  gameDelete: (dir: string) => invoke<void>('game_delete', { dir }),
  /** Copy a game with its configs and picture (the copy has no programs). */
  gameDuplicate: (dir: string) => invoke<Game>('game_duplicate', { dir }),
  /** Copy a picture into the game's folder, or (`null`) remove it. */
  gameSetPicture: (dir: string, source: string | null) => invoke<GameMeta>('game_set_picture', { dir, source }),
  /** The game's picture as a `data:` URL. */
  gamePicture: (dir: string) => invoke<string | null>('game_picture', { dir }),
  /** Programs with a visible window now. */
  runningPrograms: () => invoke<RunningProgram[]>('running_programs'),
  /** Games the local stores (Steam, Epic, GOG) have installed. */
  installedGames: () => invoke<InstalledGame[]>('installed_games'),
  /** A new game from a store's game, with its program and cover. */
  gameCreateFromStore: (name: string, program: string | null, cover: string | null) =>
    invoke<Game>('game_create_from_store', { name, program, cover }),
  /** The executable file name of the program in front. */
  foregroundProgram: () => invoke<string | null>('foreground_program'),
  gameCreateTemplate: (name: string, config: string, template: Template) =>
    invoke<Game>('game_create_template', { name, config, template }),
  configFromTemplate: (gameDir: string, name: string, template: Template) =>
    invoke<string>('config_from_template', { gameDir, name, template }),
  openDriverPage: (driver: Driver) => invoke<void>('open_driver_page', { driver }),
  gameExport: (dir: string, out: string) => invoke<void>('game_export', { dir, out }),
  /** The launch commands in a game bundle's configs, and the configs that cannot be checked for them. */
  bundleLaunches: (file: string) => invoke<BundleLaunches>('game_launches', { file }),
  gameImport: (file: string, keepLaunches: boolean) => invoke<Game>('game_import', { file, keepLaunches }),
  presetList: () => invoke<PresetEntry[]>('preset_list'),
  presetSave: (preset: LayerPreset) => invoke<string>('preset_save', { preset }),
  presetLoad: (path: string) => invoke<LayerPreset>('preset_load', { path }),
  sheetSave: (path: string, html: string) => invoke<void>('sheet_save', { path, html }),

  configLoad: (path: string) => invoke<LoadedConfig>('config_load', { path }),
  configSave: (path: string, profile: Profile) => invoke<DiagMsg[]>('config_save', { path, profile }),
  /** Validate a profile being edited, without saving it. */
  configCheck: (profile: Profile) => invoke<DiagMsg[]>('config_check', { profile }),
  /** Earlier versions kept of a config, the newest first. */
  configVersions: (path: string) => invoke<ConfigVersion[]>('config_versions', { path }),
  configCreate: (gameDir: string, name: string) => invoke<string>('config_create', { gameDir, name }),
  configDuplicate: (path: string) => invoke<string>('config_duplicate', { path }),
  configDelete: (path: string) => invoke<void>('config_delete', { path }),
  configImportLaunches: (source: string) => invoke<Launch[]>('config_import_launches', { source }),
  configImport: (gameDir: string, source: string, keepLaunches: boolean) =>
    invoke<Imported>('config_import', { gameDir, source, keepLaunches }),
  reveal: (path: string) => invoke<void>('reveal', { path }),

  /** `device`: that controller's own slots instead of the shared ones. */
  applyToSlot: (slot: number, path: string, device: string | null = null) => invoke<void>('apply_to_slot', { slot, path, device }),
  selectSlot: (slot: number, device: string | null = null) => invoke<void>('select_slot', { slot, device }),
  clearSlot: (slot: number, device: string | null = null) => invoke<void>('clear_slot', { slot, device }),
  /** Give a controller slots of its own, or let it use the shared ones again. */
  ownSlots: (device: string, on: boolean) => invoke<void>('own_slots', { device, on }),
  setRemap: (on: boolean) => invoke<void>('set_remap', { on }),
  setAutoSwitch: (on: boolean) => invoke<void>('set_auto_switch', { on }),
  setPreferredDevice: (id: string | null) => invoke<void>('set_preferred_device', { id }),
  refreshDrivers: () => invoke<void>('refresh_drivers'),
  /** Look for controllers now, also those another program held until now. */
  rescanDevices: () => invoke<void>('rescan_devices'),
  hotkeysGet: () => invoke<Hotkeys>('hotkeys_get'),
  hotkeysSet: (hotkeys: Hotkeys) => invoke<void>('hotkeys_set', { hotkeys }),
  /** Pause the hotkeys while recording a new one. */
  hotkeysPause: (on: boolean) => invoke<void>('hotkeys_pause', { on }),
  /** One full in-game turn of `counts` mouse counts, 3 s from now (calibration). */
  testTurn: (counts: number) => invoke<void>('test_turn', { counts }),
  /** Gyro calibration of a controller (`null`: the controller in use). */
  gyroCalibrate: (device: string | null) => invoke<void>('gyro_calibrate', { device }),
  gyroAuto: (device: string | null, on: boolean) => invoke<void>('gyro_auto', { device, on }),
  gyroReset: (device: string | null) => invoke<void>('gyro_reset', { device }),
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
  /** What the player lights and the microphone LED show. */
  setLights: (player: PlayerLights, mic: boolean) => invoke<void>('set_lights', { player, mic }),
  /** Ignore a device completely, or use it again. */
  setBlocked: (id: string, blocked: boolean) => invoke<void>('set_blocked', { id, blocked }),
  setGroups: (groups: DeviceGroup[]) => invoke<void>('set_groups', { groups }),
  setPowerOffAfter: (minutes: number) => invoke<void>('set_power_off_after', { minutes }),
  setSwitch2Bluetooth: (on: boolean) => invoke<void>('set_switch2_bluetooth', { on }),
  powerOff: (id: string) => invoke<void>('power_off', { id }),
  setLayout: (layout: GenericLayout) => invoke<void>('set_layout', { layout }),
  forgetLayout: (device: string) => invoke<void>('forget_layout', { device }),
  setDsu: (settings: DsuSettings) => invoke<void>('set_dsu', { settings }),
  setHttp: (settings: HttpSettings) => invoke<void>('set_http', { settings }),
  newHttpToken: () => invoke<void>('new_http_token'),
  setRuntimeOptions: (options: RuntimeOptions) => invoke<void>('set_runtime_options', { options }),
  /** Show a sample on-screen notification. */
  osdTest: () => invoke<void>('osd_test'),
  /** Sizes of the monitors, the primary one first. */
  osdMonitors: () => invoke<string[]>('osd_monitors'),
  /** Start hidden in the tray when the user signs in; `null` where unsupported (not Windows). */
  autostartGet: () => invoke<boolean | null>('autostart_get'),
  autostartSet: (on: boolean) => invoke<void>('autostart_set', { on }),
  /** Show the window, e.g. when the app started hidden and needs to ask something. */
  showWindow: () => invoke<void>('window_show'),
  /** Where the library is, and the default folder. */
  libraryPlace: () => invoke<LibraryPlace>('library_place'),
  /** What a folder offered for the library holds; rejects when the library cannot go there. */
  libraryTarget: (dir: string) => invoke<LibraryTarget>('library_target', { dir }),
  /** Move the library to `dir`, or use the library there; `null`: back to the default folder. */
  libraryMove: (dir: string | null) => invoke<LibraryMoved>('library_move', { dir }),
  logFile: () => invoke<string>('log_file'),
  takeNotices: () => invoke<Notice[]>('take_notices'),
  /** Game bundles the app was started with (opened from Explorer). */
  takeOpenFiles: () => invoke<string[]>('take_open_files'),

  /** Ask the update endpoints (GitHub) for a newer version; `null` when this one is the newest. */
  updateCheck: () => invoke<UpdateInfo | null>('update_check'),
  /**
   * Download the update the last check found, check its signature and install it. The app stops
   * remapping and exits, the setup starts it again; the promise settles only when that failed.
   */
  updateInstall(onProgress: (percent: number) => void) {
    const channel = new Channel<number>();
    channel.onmessage = onProgress;
    return invoke<void>('update_install', { onProgress: channel });
  },
};

/** Whether `e` is a command's error with a code (`CmdError` in gui/src-tauri/src/error.rs). */
function isCmdError(e: unknown): e is CmdError {
  return typeof e === 'object' && e !== null && typeof (e as CmdError).code === 'string' && typeof (e as CmdError).detail === 'string';
}

/** Error text from a rejected command, in the window's language where the app words it (`errors.<code>`). */
export function errorText(e: unknown): string {
  if (isCmdError(e)) {
    const key = `errors.${e.code}`;
    return has(key) ? t(key, { detail: e.detail }) : e.detail || e.code;
  }
  return typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
