// The JSON the Rust side sends (fixtures written by gui/src-tauri/src/tests.rs) against the
// type declarations: each key list must name exactly the keys of its type (tsc checks that)
// and exactly the keys Rust writes (this test checks that).

import { describe, expect, it } from 'vitest';
import { EVENTS } from '../api';
import { has, setLanguage } from '../i18n/index.svelte';
import { PATCH_SUFFIX } from '../overlay';
import { ZOOM_MAX, ZOOM_MIN } from '../state/shortcuts';
import type {
  Battery,
  BundleLaunches,
  Catalog,
  CatalogLimits,
  CmdError,
  ConfigEntry,
  ConfigVersion,
  DeviceGroup,
  DiagMsg,
  DriverState,
  DsuSettings,
  ErrorCode,
  Game,
  GameMeta,
  GuiSettings,
  GyroCalibration,
  Hint,
  Hotkeys,
  HttpSettings,
  Imported,
  InstalledGame,
  Launch,
  LaunchPlace,
  Library,
  LibraryMoved,
  LibraryPlace,
  LibraryTarget,
  LiveDevice,
  LiveDsu,
  LiveHttp,
  LiveSession,
  LoadedConfig,
  MappingsOverlayContent,
  Notice,
  SessionOutput,
  LiveState,
  MappingsOverlaySettings,
  OsdEvent,
  OsdMessage,
  OsdSettings,
  PadOverlayContent,
  PadOverlaySettings,
  PadState,
  PresetEntry,
  RadialContent,
  RadialPointed,
  RawInput,
  RewasdStatus,
  RunningProgram,
  RuntimeOptions,
  ScreenRef,
  StickConfig,
  TriggerConfig,
  UncheckedConfig,
  UpdateInfo,
} from '../types';
import { applyCurve, shapeStick, shapeTrigger } from './analog';
import { SETTINGS_PLACEHOLDER } from '../state/settings';
import { controlLabel } from './controls';
import { MAX_CROSSHAIRS } from './crosshair';
import bridge from './fixtures/bridge.json';
import contract from './fixtures/contract.json';
import controls from './fixtures/controls.json';
import defaultsJson from './fixtures/defaults.json';
import shaping from './fixtures/shaping.json';
import { PAD_COLORS, padColor } from './padColors';
import { MAX_SECTORS } from './radial';
import { stickThresholds, stickZoneAt, triggerThresholds, triggerZoneAt, zoneDefaults } from './zones';

type Keys<T> = Record<keyof T, true>;

const live = {
  devices: true,
  primary: true,
  profile: true,
  profile_path: true,
  layer: true,
  layer_name: true,
  slots: true,
  active_slot: true,
  remap_on: true,
  auto_switch: true,
  auto_active: true,
  player_lights: true,
  mic_led: true,
  seen_devices: true,
  blocked: true,
  groups: true,
  power_off_after_min: true,
  switch2_bluetooth: true,
  layouts: true,
  dsu: true,
  http: true,
  runtime: true,
  locked: true,
  focus_paused: true,
  suspended: true,
  foreground: true,
  virtual_pad: true,
  hidden_devices: true,
  warnings: true,
  drivers: true,
  device_slots: true,
  sessions: true,
  crosshair: true,
  radial: true,
  has_radial: true,
  pad_overlay: true,
  mappings_overlay: true,
  app_navigation: true,
  screenshot: true,
} satisfies Keys<LiveState>;

const device = {
  id: true,
  name: true,
  family: true,
  backend: true,
  vendor_id: true,
  product_id: true,
  state: true,
  gyro_calibration: true,
  battery: true,
  raw: true,
  mirror: true,
} satisfies Keys<LiveDevice>;

const padState = { buttons: true, axes: true, motion: true, touch: true } satisfies Keys<PadState>;
const motion = { gyro: true, accel: true } satisfies Keys<NonNullable<PadState['motion']>>;
const touchPoint = { x: true, y: true } satisfies Keys<NonNullable<PadState['touch'][number]>>;
const gyroCalibration = { gyro_bias: true, auto: true, measuring: true, last_manual: true } satisfies Keys<GyroCalibration>;
const battery = { level: true, charging: true, wired: true } satisfies Keys<Battery>;
const rawInput = { buttons: true, axes: true, hats: true } satisfies Keys<RawInput>;
const seenDevice = { id: true, name: true } satisfies Keys<LiveState['seen_devices'][number]>;
const group = { name: true, members: true } satisfies Keys<DeviceGroup>;
const liveDsu = { settings: true, address: true, clients: true, error: true } satisfies Keys<LiveDsu>;
const dsuSettings = { enabled: true, lan: true, port: true } satisfies Keys<DsuSettings>;
const liveHttp = { settings: true, address: true, error: true } satisfies Keys<LiveHttp>;
const httpSettings = { enabled: true, lan: true, port: true, token: true } satisfies Keys<HttpSettings>;
const runtime = { prevent_sleep: true, pause_unfocused: true, keep_offline_pads: true } satisfies Keys<RuntimeOptions>;
const drivers = { virtual_pads: true, device_hiding: true, input_access: true, bluetooth: true } satisfies Keys<LiveState['drivers']>;
const driverState = { available: true, error: true } satisfies Keys<DriverState>;
const deviceSlots = { id: true, slots: true, active_slot: true } satisfies Keys<LiveState['device_slots'][number]>;
const session = {
  owner: true,
  device: true,
  profile: true,
  profile_path: true,
  layer: true,
  virtual_pad: true,
  output: true,
} satisfies Keys<LiveSession>;
const sessionOutput = { pad: true, keys: true, mouse_buttons: true } satisfies Keys<SessionOutput>;
const radial = { menu: true, pointed: true } satisfies Keys<NonNullable<LiveState['radial']>>;

const guiSettings = {
  language: true,
  stay_live: true,
  hook_buttons: true,
  rewasd_at_start: true,
  check_updates: true,
  osd: true,
  pad_overlay: true,
  mappings_overlay: true,
  welcome_done: true,
  show_notes: true,
  silenced: true,
  zoom: true,
  pad_navigation: true,
  tray_per_controller: true,
  controller_colors: true,
} satisfies Keys<GuiSettings>;
const osdSettings = {
  enabled: true,
  remap: true,
  slot: true,
  layer: true,
  keep_layer: true,
  controller: true,
  battery: true,
  screenshot: true,
  monitor: true,
  corner: true,
  scale: true,
  opacity: true,
  seconds: true,
} satisfies Keys<Omit<OsdSettings, 'screen'>>; // `screen` only once a monitor was chosen: see ScreenRef
const padOverlay = { corner: true, size: true, opacity: true, hide_after: true } satisfies Keys<PadOverlaySettings>;
const mappingsOverlay = { corner: true, opacity: true } satisfies Keys<MappingsOverlaySettings>;
const hotkeys = {
  hold_ms: true,
  slot1: true,
  slot2: true,
  slot3: true,
  slot4: true,
  remap: true,
  stop_macros: true,
  emergency_stop: true,
  pad_overlay: true,
  mappings_overlay: true,
  app_navigation: true,
  rumble: true,
} satisfies Keys<Hotkeys>;
const osdMessage = {
  kind: true,
  name: true,
  level: true,
  id: true,
  lang: true,
  opacity: true,
  scale: true,
  corner: true,
} satisfies Keys<Extract<OsdMessage, { kind: 'battery_low' }>>;
const launch = { file: true, config: true, place: true, path: true, args: true } satisfies Keys<Launch>;
const mappingPlace = { kind: true, layer: true, layer_name: true, trigger: true } satisfies Keys<Extract<LaunchPlace, { kind: 'mapping' }>>;
const radialPlace = { kind: true, menu: true, name: true, sector: true, label: true } satisfies Keys<
  Extract<LaunchPlace, { kind: 'radial' }>
>;

const l = contract.live;
const d = l.devices[0];
const cases: [string, object, object][] = [
  ['LiveState', live, l],
  ['LiveDevice', device, d],
  ['PadState', padState, d.state],
  ['Motion', motion, d.state.motion],
  ['TouchPoint', touchPoint, d.state.touch[0]!],
  ['GyroCalibration', gyroCalibration, d.gyro_calibration],
  ['Battery', battery, d.battery],
  ['RawInput', rawInput, d.raw],
  ['SeenDevice', seenDevice, l.seen_devices[0]],
  ['DeviceGroup', group, l.groups[0]],
  ['LiveDsu', liveDsu, l.dsu],
  ['DsuSettings', dsuSettings, l.dsu.settings],
  ['LiveHttp', liveHttp, l.http],
  ['HttpSettings', httpSettings, l.http.settings],
  ['RuntimeOptions', runtime, l.runtime],
  ['drivers', drivers, l.drivers],
  ['DriverState', driverState, l.drivers.virtual_pads],
  ['device_slots', deviceSlots, l.device_slots[0]],
  ['LiveSession', session, l.sessions[0]],
  ['SessionOutput', sessionOutput, l.sessions[0]!.output],
  ['radial', radial, l.radial],
  ['GuiSettings', guiSettings, contract.gui_settings],
  ['OsdSettings', osdSettings, contract.gui_settings.osd],
  ['PadOverlaySettings', padOverlay, contract.gui_settings.pad_overlay],
  ['MappingsOverlaySettings', mappingsOverlay, contract.gui_settings.mappings_overlay],
  ['Hotkeys', hotkeys, contract.hotkeys],
  ['OsdMessage', osdMessage, contract.osd_message],
  ['Launch', launch, contract.launches[0]!],
  ['LaunchPlace mapping', mappingPlace, contract.launches[0]!.place],
  ['LaunchPlace radial', radialPlace, contract.launches[1]!.place],
];

const sorted = (o: object) => Object.keys(o).sort();

describe('the types match what Rust sends', () => {
  it.each(cases)('%s', (_, keys, json) => {
    expect(sorted(json)).toEqual(sorted(keys));
  });

  it("the window's settings placeholder is Rust's default, the welcome guide aside", () => {
    expect({ ...SETTINGS_PLACEHOLDER, welcome_done: false }).toEqual(contract.gui_settings);
  });

  it('the crosshair and the radial menu arrive', () => {
    expect(l.crosshair).not.toBeNull();
    expect(l.radial.menu).toBeTypeOf('object');
  });
});

// ----- the bridge's own types (fixtures/bridge.json) -------------------------------------

const b = bridge;
const game = b.library.games[0]!;
const bridgeCases: [string, object, object][] = [
  ['CatalogLimits', { zoom: true, radial_sectors: true, crosshairs: true, pad_colors: true } satisfies Keys<CatalogLimits>, b.limits],
  ['CmdError', { code: true, detail: true } satisfies Keys<CmdError>, b.error],
  ['UpdateInfo', { version: true, current: true, notes: true } satisfies Keys<UpdateInfo>, b.update_info],
  ['Library', { games: true, unsorted: true, errors: true } satisfies Keys<Library>, b.library],
  ['Game', { dir: true, meta: true, configs: true } satisfies Keys<Game>, game],
  [
    'GameMeta',
    { name: true, processes: true, image: true, default_config: true, order: true, description: true } satisfies Keys<GameMeta>,
    game.meta,
  ],
  ['ConfigEntry', { path: true, name: true, error: true } satisfies Keys<ConfigEntry>, game.configs[0]!],
  ['LibraryPlace', { dir: true, default: true, is_default: true, portable: true } satisfies Keys<LibraryPlace>, b.library_place],
  ['LibraryMoved', { dir: true, moved_from: true, left_behind: true } satisfies Keys<LibraryMoved>, b.library_moved],
  ['LoadedConfig', { profile: true, diagnostics: true, read_only: true } satisfies Keys<LoadedConfig>, b.loaded_config],
  ['DiagMsg', { severity: true, path: true, message: true } satisfies Keys<DiagMsg>, b.loaded_config.diagnostics[0]!],
  ['Imported', { path: true, diagnostics: true } satisfies Keys<Imported>, b.imported],
  ['ConfigVersion', { path: true, time_ms: true } satisfies Keys<ConfigVersion>, b.versions[0]!],
  ['BundleLaunches', { launches: true, unchecked: true } satisfies Keys<BundleLaunches>, b.bundle_launches],
  ['UncheckedConfig', { file: true, error: true } satisfies Keys<UncheckedConfig>, b.bundle_launches.unchecked[0]!],
  ['PresetEntry', { name: true, path: true } satisfies Keys<PresetEntry>, b.presets[0]!],
  [
    'InstalledGame',
    { store: true, name: true, dir: true, programs: true, cover: true } satisfies Keys<InstalledGame>,
    b.installed_games[0]!,
  ],
  ['RunningProgram', { exe: true, title: true } satisfies Keys<RunningProgram>, b.running_programs[0]!],
  ['RewasdStatus', { service: true, programs: true } satisfies Keys<RewasdStatus>, b.rewasd_status],
  ['Hint', { id: true, game: true } satisfies Keys<Hint>, b.hint],
  ['ScreenRef', { name: true, x: true, y: true } satisfies Keys<ScreenRef>, b.osd_screen],
  ['RadialContent', { menu: true, pointed: true, images: true, cursor: true } satisfies Keys<RadialContent>, b.radial],
  ['RadialPointed', { pointed: true } satisfies Keys<RadialPointed>, b.radial_pointed],
  ['PadOverlayContent', { device: true, opacity: true, corner: true } satisfies Keys<PadOverlayContent>, b.pad_overlay],
  [
    'MappingsOverlayContent',
    { profile: true, layer: true, family: true, lang: true, opacity: true, corner: true } satisfies Keys<MappingsOverlayContent>,
    b.mappings_overlay,
  ],
];

/** The fields of each notification kind, `kind` included. */
const osdEvents = {
  remap_on: { kind: true },
  remap_off: { kind: true },
  slot: { kind: true, slot: true, config: true },
  layer: { kind: true, layer: true, name: true },
  connected: { kind: true, name: true },
  disconnected: { kind: true, name: true },
  battery_low: { kind: true, name: true, level: true },
  battery_critical: { kind: true, name: true, level: true },
  steam_input: { kind: true, game: true },
  screenshot: { kind: true, file: true, error: true },
  test: { kind: true },
} satisfies { [K in OsdEvent['kind']]: Keys<Extract<OsdEvent, { kind: K }>> };

/** Every error code, as the catalogs word them. */
const errorCodes = {
  failed: true,
  nothing_to_allow: true,
  sheet_not_html: true,
  service_answer: true,
  not_in_library: true,
  no_update: true,
  portable_update: true,
  portable_autostart: true,
  autostart_unsupported: true,
} satisfies Record<ErrorCode, true>;

describe("the bridge's own types match what Rust sends", () => {
  it.each(bridgeCases)('%s', (_, keys, json) => {
    expect(sorted(json)).toEqual(sorted(keys));
  });

  it('every notification kind, each with its fields', () => {
    expect(b.osd_events.map((e) => e.kind).sort()).toEqual(sorted(osdEvents));
    for (const e of b.osd_events) expect(sorted(e), e.kind).toEqual(sorted(osdEvents[e.kind as OsdEvent['kind']]));
  });

  it('every error code, each with its text', () => {
    expect([...b.error_codes].sort()).toEqual(sorted(errorCodes));
    setLanguage('en');
    for (const code of b.error_codes) expect(has(`errors.${code}`), code).toBe(true);
  });

  it('the other unions', () => {
    expect([...b.library_targets].sort()).toEqual(
      sorted({ empty: true, library: true, foreign: true } satisfies Record<LibraryTarget, true>),
    );
    expect(b.notices).toEqual(['service-takeover' satisfies Notice]);
    expect(b.installed_games.map((g) => g.store)).toEqual(['steam' satisfies InstalledGame['store']]);
  });

  it('the event names', () => {
    expect(EVENTS).toEqual({ openFiles: b.events.open_files, hint: b.events.hint });
    expect(PATCH_SUFFIX).toBe(b.events.overlay_patch);
  });
});

describe("the window's own copies of the catalog's limits", () => {
  // Kept here until the window reads them from the catalog.
  it('zoom, radial sectors and crosshairs', () => {
    expect([ZOOM_MIN, ZOOM_MAX]).toEqual(b.limits.zoom);
    expect(MAX_SECTORS).toBe(b.limits.radial_sectors);
    expect(MAX_CROSSHAIRS).toBe(b.limits.crosshairs);
  });

  it("the controllers' colours and the pick by id", () => {
    expect(PAD_COLORS).toEqual(b.limits.pad_colors);
    for (const [id, color] of Object.entries(b.pad_colors)) expect(padColor(id, {}), id).toBe(color);
  });
});

describe('control names', () => {
  it('every pad control has a label', () => {
    setLanguage('en');
    // Without a controller family the face buttons go by their names.
    const ownName = new Set(['A', 'B', 'X', 'Y']);
    for (const c of controls) {
      const label = controlLabel(c, null);
      if (!ownName.has(c)) expect(label, c).not.toBe(c);
      expect(label, `${c}: no missing catalog key`).not.toMatch(/\b[a-z_0-9]+\.[a-z_0-9]+\b/);
    }
  });
});

// Through `unknown`: JSON imports type arrays as `number[]`, the model uses tuples.
const def = defaultsJson as unknown as Catalog['defaults'];
const zd = zoneDefaults(def);

const near = (a: number, b: number) => Math.abs(a - b) < 1e-5;

describe('shaping matches the engine', () => {
  it('response curves', () => {
    for (const c of shaping.curve) {
      const points = c.points as [number, number][];
      c.t.forEach((t, i) => expect(near(applyCurve(points, t), c.out[i]), `${JSON.stringify(points)} at ${t}`).toBe(true));
    }
  });

  it('sticks', () => {
    for (const c of shaping.stick) {
      const cfg = { ...def.stick, ...(c.config as StickConfig) };
      const t = stickThresholds(cfg.zones, zd);
      expect(
        t.x.every((v, i) => near(v, c.thresholds.x[i])),
        'x thresholds',
      ).toBe(true);
      expect(
        t.y.every((v, i) => near(v, c.thresholds.y[i])),
        'y thresholds',
      ).toBe(true);
      c.in.forEach(([x, y], i) => {
        const [ox, oy] = shapeStick([x, y], cfg);
        expect(near(ox, c.out[i][0]) && near(oy, c.out[i][1]), `${x}, ${y}`).toBe(true);
        expect(stickZoneAt(x, y, t), `${x}, ${y}`).toBe(c.zones[i]);
      });
    }
  });

  it('triggers', () => {
    for (const c of shaping.trigger) {
      const cfg = { ...def.trigger, ...(c.config as TriggerConfig) };
      const t = triggerThresholds(cfg.zones, cfg.full_pull, zd);
      expect(
        t.every((v, i) => near(v, c.thresholds[i])),
        'thresholds',
      ).toBe(true);
      c.in.forEach((v, i) => {
        expect(near(shapeTrigger(v, cfg), c.out[i]), `${v}`).toBe(true);
        expect(triggerZoneAt(v, t), `${v}`).toBe(c.zones[i]);
      });
    }
  });
});
