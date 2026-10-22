// The JSON the Rust side sends (fixtures written by gui/src-tauri/src/tests.rs) against the
// type declarations: each key list must name exactly the keys of its type (tsc checks that)
// and exactly the keys Rust writes (this test checks that).

import { describe, expect, it } from 'vitest';
import { setLanguage } from '../i18n/index.svelte';
import type {
  Battery,
  Catalog,
  DeviceGroup,
  DriverState,
  DsuSettings,
  GuiSettings,
  GyroCalibration,
  Hotkeys,
  HttpSettings,
  LiveDevice,
  LiveDsu,
  LiveHttp,
  LiveSession,
  SessionOutput,
  LiveState,
  MappingsOverlaySettings,
  OsdMessage,
  OsdSettings,
  PadOverlaySettings,
  PadState,
  RawInput,
  RuntimeOptions,
  StickConfig,
  TriggerConfig,
} from '../types';
import { applyCurve, shapeStick, shapeTrigger } from './analog';
import { SETTINGS_PLACEHOLDER } from '../state/settings';
import { controlLabel } from './controls';
import contract from './fixtures/contract.json';
import controls from './fixtures/controls.json';
import defaultsJson from './fixtures/defaults.json';
import shaping from './fixtures/shaping.json';
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
const drivers = { virtual_pads: true, device_hiding: true } satisfies Keys<LiveState['drivers']>;
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
  monitor: true,
  corner: true,
  scale: true,
  opacity: true,
  seconds: true,
} satisfies Keys<OsdSettings>;
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
