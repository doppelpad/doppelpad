// TypeScript mirror of the serde JSON shapes of dp-core's profile model and of the
// GUI commands. Fields that Rust skips when they hold their default are optional here;
// missing fields get Rust's defaults again when a profile is saved.

/** A control in profile syntax: `A`, `LeftStickUp`, `LT`, `RTHigh`, `Key:Space`, `Mouse:Left`, … */
export type ControlName = string;
/** One control, or several that must be held together. */
export type Combo = ControlName | ControlName[];

export type Activator = 'single' | 'double' | 'triple' | 'long' | 'start' | 'release';
export type Mode = 'hold' | 'turbo' | 'toggle' | 'once' | 'repeat' | 'while_held';
export type Press = 'auto' | 'tap' | 'down' | 'up';
export type StickName = 'left' | 'right';
export type TriggerName = 'left' | 'right';
export type Dir = 'up' | 'down' | 'left' | 'right';
export type MouseButtonName = 'left' | 'right' | 'middle' | 'x1' | 'x2';
export type WheelDir = 'up' | 'down' | 'left' | 'right';
export type PadButtonName =
  | 'A'
  | 'B'
  | 'X'
  | 'Y'
  | 'LB'
  | 'RB'
  | 'Back'
  | 'Start'
  | 'LS'
  | 'RS'
  | 'DpadUp'
  | 'DpadDown'
  | 'DpadLeft'
  | 'DpadRight'
  | 'Guide'
  | 'Touchpad'
  | 'Misc'
  | 'PaddleUL'
  | 'PaddleUR'
  | 'PaddleLL'
  | 'PaddleLR'
  | 'Misc2';
export type VirtualPadType = 'xbox360' | 'dual_shock4' | 'none';
/** A virtual controller that exists (sessions report the one they play on). */
export type VirtualPadKind = Exclude<VirtualPadType, 'none'>;
/** Mouse buttons as controls (`Mouse:Left`); the catalog lists them. */
export type MouseControlButton = 'Left' | 'Right' | 'Middle' | 'X1' | 'X2';
export type JumpMode = 'hold' | 'toggle' | 'switch';

export type Command =
  | 'remap_off'
  | 'toggle_gyro'
  | 'gyro_on'
  | 'gyro_off'
  | 'reset_tilt'
  | 'switch_gyro_axis'
  | 'shutdown'
  | 'restart'
  | 'sleep'
  | 'hibernate'
  | 'log_off'
  | 'screenshot'
  | 'close_window'
  | 'toggle_crosshair'
  | 'radial_next'
  | 'radial_prev'
  | 'radial_up'
  | 'radial_down'
  | 'radial_left'
  | 'radial_right'
  | 'radial_apply'
  | 'radial_back'
  | 'pad_overlay'
  | 'mappings_overlay'
  | 'power_off'
  /** Show crosshair N (1-based), or hide it if it is the one showing. */
  | { crosshair: number }
  | { launch: { path: string; args?: string } }
  | { unsupported: string };

/** A gesture a touch action plays on the virtual touchpad. */
export type TouchOutput =
  | 'tap'
  | 'tap_left'
  | 'tap_center'
  | 'tap_right'
  | 'multi_tap'
  | 'swipe_up'
  | 'swipe_down'
  | 'swipe_left'
  | 'swipe_right'
  | 'swipe2_up'
  | 'swipe2_down'
  | 'swipe2_left'
  | 'swipe2_right'
  | 'zoom_in'
  | 'zoom_out';

/** Accelerometer axis and the direction a shake starts in. */
export type ShakeAxis = '+x' | '-x' | '+y' | '-y' | '+z' | '-z';

export type Action =
  | { key: string; action?: Press }
  | { pad: PadButtonName; action?: Press }
  | { mouse: MouseButtonName; action?: Press }
  | { wheel: WheelDir; clicks?: number }
  | { stick: StickName; x: number; y: number; absolute?: boolean; action?: Press }
  | { trigger: TriggerName; value?: number; action?: Press }
  | { mouse_move: Dir; speed?: number; acceleration?: number }
  | { mouse_to: [number, number] }
  | { mouse_clip: [number, number, number, number] }
  | { pause: number }
  | { rumble: number; low?: number; high?: number }
  | { shake: ShakeAxis }
  | { turn: number }
  | { touch: TouchOutput; ms?: number }
  | { command: Command }
  | { break: true };

export interface Jump {
  layer: number;
  mode?: JumpMode;
  delay_ms?: number;
  /** Run the mapping's actions when the delayed jump happens instead of at the press. */
  postpone?: boolean;
}

export interface Mapping {
  layer?: number;
  trigger: Combo;
  activator?: Activator;
  mode?: Mode;
  actions?: Action[];
  jump?: Jump;
  turbo_ms?: number;
  repeat?: number;
  repeat_pause_ms?: number;
  /** Extra random wait of up to this many ms before each turbo or repeat cycle. */
  random_ms?: number;
  long_press_ms?: number;
  double_tap_ms?: number;
  triple_tap_ms?: number;
  key_delay_ms?: number;
  description?: string;
}

export interface Layer {
  id: number;
  name?: string;
  inherit?: boolean;
  not_inherited?: Combo[];
  /** Stick and trigger fields this layer changes; the rest are Main's. */
  sticks?: Sticks;
  triggers?: Triggers;
  virtual?: { sticks?: VirtualSticks };
  /** Give the physical keyboard and/or mouse back while the layer is active. */
  unlock?: Unlock;
  /** The light bar while this layer is active. */
  lightbar?: Light;
}

export type Unlock = 'all' | 'keyboard' | 'mouse';

/** How the game's rumble reaches the controller (mapping rumble stays as it is). */
export interface Vibration {
  /** 0–1; 0 turns the motor off. */
  left?: number;
  right?: number;
  swap?: boolean;
  min_delay_ms?: number;
}

export type LightMode = 'off' | 'solid' | 'breath' | 'rainbow' | 'shimmer' | 'heartbeat' | 'strobe';

/** A light-bar setting; whatever it leaves out is the default. */
export interface Light {
  mode?: LightMode;
  /** `#rrggbb`, 1–10. */
  colors?: string[];
  /** Use the next colour every period. */
  cycle?: boolean;
  period_ms?: number;
}

/** What the controllers' player lights show. */
export type PlayerLights = 'while_remapping' | 'always' | 'off';

/** Controllers that act as one (`group:<name>`), the first member leading. */
export interface DeviceGroup {
  name: string;
  members: string[];
}

export type CrosshairShape =
  'cross' | 'cross_gap' | 'dot' | 'circle' | 'circle_dot' | 'circle_cross' | 'x' | 't' | 'chevron' | 'square' | 'brackets';

/** A crosshair drawn over the game; whatever it leaves out is the default. */
export interface Crosshair {
  name?: string;
  shape?: CrosshairShape;
  /** `#rrggbb`. */
  color?: string;
  /** 0–1. */
  opacity?: number;
  /** Logical pixels, 10–200. */
  size?: number;
  /** Percent of the widest line (a fifth of the size), 10–100. */
  thickness?: number;
  /** Pixels, 0–10. */
  outline?: number;
  outline_color?: string;
  /** Pixels from the screen centre, right and down. */
  offset?: [number, number];
  /** Off: the entry only magnifies. */
  visible?: boolean;
  magnifier?: Magnifier;
  /** Shown while one of these layers is active. */
  layers?: number[];
}

/** A crosshair with its defaults filled in; Rust leaves `name` and `layers` out while empty. */
export type CrosshairFull = Required<Omit<Crosshair, 'magnifier' | 'name' | 'layers'>> & Pick<Crosshair, 'name' | 'layers'>;

/** What points at a sector of an open radial menu. */
export type RadialSelect = 'left_stick' | 'right_stick' | 'mouse' | 'commands';

/** When the pointed sector runs: as the menu's layer ends, or with the apply command. */
export type RadialApply = 'release' | 'command';

/** Where a radial menu sits: the middle, an edge, a corner, or the mouse pointer as it opens. */
export type RadialPosition =
  'center' | 'top' | 'bottom' | 'left' | 'right' | 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right' | 'cursor';

/** A ring of sectors over the game, open while its layer is active. */
export interface RadialMenu {
  name?: string;
  /** 0: only as a sub-menu. */
  layer?: number;
  select?: RadialSelect;
  apply?: RadialApply;
  delay_ms?: number;
  /** 0–1. */
  tint?: number;
  /** 0.5–2. */
  scale?: number;
  position?: RadialPosition;
  /** Pixels from `position`, right and down. */
  offset?: [number, number];
  sectors: RadialSector[];
}

export interface RadialSector {
  label?: string;
  /** A name of the app's icon set, or an image file. */
  icon?: string;
  /** `#rrggbb`. */
  color?: string;
  actions?: Action[];
  /** Open this menu (1-based) instead. */
  submenu?: number;
  close?: boolean;
}

export interface Magnifier {
  /** 1–16. */
  zoom: number;
  /** Diameter as a fraction of the screen height. */
  size: number;
}

export interface Output {
  virtual_pad?: VirtualPadType;
  hide_physical?: boolean;
  passthrough?: boolean;
  mute_mapped_inputs?: boolean;
}

export interface Timing {
  long_press_ms?: number;
  double_tap_ms?: number;
  triple_tap_ms?: number;
  key_delay_ms?: number;
  turbo_ms?: number;
  combo_window_ms?: number;
}

/** A stick zone threshold: one value for a circle, `[x, y]` for an ellipse. */
export type ZoneRadius = number | [number, number];

/** Where each zone starts (fraction of full travel); unset med/high are evenly spaced. */
export interface StickZones {
  low?: ZoneRadius;
  med?: ZoneRadius;
  high?: ZoneRadius;
}

export interface TriggerZones {
  low?: number;
  med?: number;
  high?: number;
}

export interface StickConfig {
  deadzone?: number;
  outer?: number;
  anti_deadzone?: number;
  sensitivity?: number;
  curve?: [number, number][];
  invert_x?: boolean;
  invert_y?: boolean;
  rotation?: number;
  zones?: StickZones;
  diagonals?: boolean;
  direction_width?: number;
}

/** A DualSense trigger effect; positions are tenths of the trigger's travel. */
export type TriggerEffect =
  | { kind: 'off' }
  | { kind: 'resistance'; start: number; strength: number }
  | { kind: 'weapon'; start: number; end: number; strength: number }
  | { kind: 'vibration'; start: number; amplitude: number; frequency: number }
  | { kind: 'raw'; bytes: number[] };

export interface TriggerConfig {
  deadzone?: number;
  zones?: TriggerZones;
  full_pull?: number;
  /** Resistance or vibration against the finger (DualSense). */
  effect?: TriggerEffect;
}

/** A trigger's settings with the defaults filled in; `effect` stays optional (none by default). */
export type TriggerFull = Required<Omit<TriggerConfig, 'effect'>> & Pick<TriggerConfig, 'effect'>;

export interface Sticks {
  left?: StickConfig;
  right?: StickConfig;
}

export interface Triggers {
  left?: TriggerConfig;
  right?: TriggerConfig;
}

export type AnalogSource = 'left_stick' | 'right_stick' | 'mouse' | 'gyro' | 'touchpad';
export type AnalogTarget = 'mouse' | 'left_stick' | 'right_stick' | 'flick';

export interface AnalogMapping {
  layer?: number;
  source?: AnalogSource;
  target?: AnalogTarget;
  sensitivity?: number;
  vertical_ratio?: number;
  curve?: number;
  invert_x?: boolean;
  invert_y?: boolean;
  deadzone?: number;
  /** Stick sources: deflection treated as fully pushed. */
  outer?: number;
  anti_deadzone?: number;
  smoothing_ms?: number;
  decay_ms?: number;
  trackball_friction?: number;
  while_held?: ControlName[];
  consume?: boolean;
  spring?: boolean;
  noise_filter?: number;
  snap?: number;
  acceleration?: number;
}

/** Auto-repeat of keys that mappings hold: a mode or an interval in ms. */
export type KeyRepeat = 'native' | 'fast' | 'slow' | 'off' | number;

/** A small offset that keeps moving around a virtual stick's output. */
export interface Jitter {
  enabled?: boolean;
  pattern?: 'circle' | 'square' | 'cross';
  min?: number;
  max?: number;
  delay_min_ms?: number;
  delay_max_ms?: number;
}

/** How mappings push a virtual stick: start, cap and growth of the deflection, and its curve. */
export interface VirtualStick {
  initial?: number;
  max?: number;
  speed?: number;
  acceleration?: number;
  multiplier_x?: number;
  multiplier_y?: number;
  curve?: [number, number][];
  jitter?: Jitter;
}

/** A virtual stick with every setting filled in, its jitter's too (the defaults). */
export type VirtualStickFull = Required<Omit<VirtualStick, 'jitter'>> & { jitter: Required<Jitter> };

export interface VirtualSticks {
  left?: VirtualStick;
  right?: VirtualStick;
}

export interface VirtualDevices {
  keyboard?: { repeat?: KeyRepeat };
  sticks?: VirtualSticks;
  motion?: { shake_g?: number; shake_ms?: number };
  /** How the game turns with mouse motion: counts for 360° and how long a turn takes. */
  mouse?: { counts_per_turn?: number; turn_ms?: number; wheel_repeat_ms?: number };
}

/** How the touchpad becomes region controls. */
export interface TouchpadSettings {
  /** Left and right halves are two trackpads. */
  split?: boolean;
  zones?: 'radial' | 'grid';
  /** Radial zones: radius of the centre region (fraction of half the pad). */
  centre?: number;
  click_required?: boolean;
}

/** How the motion sensors become gyro controls and feed gyro routes. */
export interface GyroSettings {
  mode?: 'motion' | 'tilt';
  steering?: 'roll' | 'yaw';
  initially_on?: boolean;
  space?: 'local' | 'player';
  /** Low, medium, high: degrees per second (motion) or degrees (tilt). */
  motion_zones?: [number, number, number];
  tilt_zones?: [number, number, number];
}

export interface Profile {
  name?: string;
  description?: string;
  processes?: string[];
  /** Which keyboard and mouse input the mappings react to (default all). */
  kbm_source?: KbmSource;
  output?: Output;
  timing?: Timing;
  sticks?: Sticks;
  triggers?: Triggers;
  gyro?: GyroSettings;
  touchpad?: TouchpadSettings;
  /** The physical mouse as a source: speeds (counts/s) where its zones start. */
  mouse?: { zones?: [number, number, number] };
  layers?: Layer[];
  /** Off: held controls keep their old layer's action until released. */
  layer_change?: { repress?: boolean };
  /** Crosshairs drawn over the game, at most ten. */
  crosshairs?: Crosshair[];
  /** Radial menus, each shown while its layer is active. */
  radial_menus?: RadialMenu[];
  /** The controller's light bar; without it the game sets the colour. */
  lightbar?: Light;
  /** How the game's rumble reaches the controller. */
  vibration?: Vibration;
  mappings: Mapping[];
  analog?: AnalogMapping[];
  virtual?: VirtualDevices;
}

// ----- live state ---------------------------------------------------------------------

export interface PadState {
  /** Bitset; bit index = position in `Catalog.pad_buttons`. */
  buttons: number;
  /** LeftX, LeftY, RightX, RightY (−1..1, up positive), LeftTrigger, RightTrigger (0..1); see `AXES` in model/devices.ts. */
  axes: number[];
  motion: { gyro: number[]; accel: number[] } | null;
  touch: ({ x: number; y: number } | null)[];
}

export type PadFamily = 'xbox' | 'dual_shock4' | 'dual_sense' | 'switch_pro' | 'joy_con' | 'game_cube' | 'generic';

/** A controller's gyro calibration: the bias taken off its rates (°/s) and the automatic switch. */
export interface GyroCalibration {
  gyro_bias: [number, number, number];
  auto: boolean;
  /** A manual calibration runs: how far it is, 0–1. */
  measuring: number | null;
  /** How the last manual calibration ended. */
  last_manual: boolean | null;
}

export interface LiveDevice {
  id: string;
  name: string;
  family: PadFamily;
  backend: string;
  vendor_id: number;
  product_id: number;
  state: PadState;
  /** Gyro calibration, for controllers with motion sensors. */
  gyro_calibration: GyroCalibration | null;
  /** `null` when the controller does not report one. */
  battery: Battery | null;
  /** Unmapped input of a generic pad, for identifying its buttons. */
  raw: RawInput | null;
  /** Another program's virtual pad for a controller it reads (Steam's on Linux): never picked on its own and not announced. */
  mirror: boolean;
}

/** All keyboard/mouse input, physical devices only, or only what scripts inject. */
export type KbmSource = 'all' | 'physical' | 'scripts';

export interface HttpSettings {
  enabled: boolean;
  /** Listen on all network interfaces; every request then needs the token. */
  lan: boolean;
  port: number;
  token: string;
}

/** The local HTTP API. */
export interface LiveHttp {
  settings: HttpSettings;
  /** Where it listens while it runs. */
  address: string | null;
  error: string | null;
}

/** How remapping fits in with the rest of the system. */
export interface RuntimeOptions {
  prevent_sleep: boolean;
  pause_unfocused: boolean;
  keep_offline_pads: boolean;
}

export interface DsuSettings {
  enabled: boolean;
  /** Listen on all network interfaces, not only this computer. */
  lan: boolean;
  port: number;
}

/** The DSU motion server. */
export interface LiveDsu {
  settings: DsuSettings;
  /** Where it listens while it runs. */
  address: string | null;
  /** Emulators subscribed right now. */
  clients: number;
  error: string | null;
}

/** What a generic pad reports: buttons in HID order, axes -1..1, hats 0–7 clockwise from up. */
export interface RawInput {
  buttons: boolean[];
  axes: number[];
  hats: (number | null)[];
}

export type HatDir = 'up' | 'right' | 'down' | 'left';
export type StickAxis = 'LeftX' | 'LeftY' | 'RightX' | 'RightY';

export type RawSource =
  | { kind: 'button'; index: number }
  | { kind: 'hat'; index: number; dir: HatDir }
  | { kind: 'axis'; index: number; rest: number; full: number };

/** Which raw input of a generic pad model is which pad control. */
export interface GenericLayout {
  /** `vvvv:pppp` in hex. */
  device: string;
  buttons: { button: PadButtonName; from: RawSource }[];
  triggers: { trigger: 'left' | 'right'; from: RawSource }[];
  sticks: { axis: StickAxis; from: number; invert: boolean }[];
}

export interface Battery {
  /** Percent; coarse steps on some controllers. */
  level: number;
  charging: boolean;
  /** Runs on the cable without a battery. */
  wired: boolean;
}

/** The drivers Doppelpad uses, by what they do, and what keyboard and mouse need from the OS (`Driver` in commands.rs). */
export type Driver = 'virtual_pads' | 'device_hiding' | 'input_access';

/** A newer version than the running one (`UpdateInfo` in updates.rs). */
export interface UpdateInfo {
  version: string;
  /** The running version. */
  current: string;
  /** The release notes, as plain text. */
  notes: string | null;
}

/** Messages from before the window was ready (`Notice` in commands.rs; `notices.<name>` keys). */
export type Notice = 'service-takeover';

export interface DriverState {
  available: boolean;
  error: string | null;
}

export interface LiveState {
  devices: LiveDevice[];
  primary: string | null;
  profile: string | null;
  profile_path: string | null;
  layer: number;
  /** Name of the active layer, if it has one. */
  layer_name: string | null;
  slots: (string | null)[];
  active_slot: number;
  remap_on: boolean;
  auto_switch: boolean;
  /** Autodetect chose the running config for the game in front, not the active slot. */
  auto_active: boolean;
  player_lights: PlayerLights;
  mic_led: boolean;
  /** Every device the backends report, blocked and grouped ones included. */
  seen_devices: { id: string; name: string }[];
  /** Devices Doppelpad ignores. */
  blocked: string[];
  groups: DeviceGroup[];
  /** Wireless controllers turn off after this many minutes without input; 0 never. */
  power_off_after_min: number;
  /** Generic pad models with a recorded layout (`vvvv:pppp`). */
  layouts: string[];
  dsu: LiveDsu;
  http: LiveHttp;
  runtime: RuntimeOptions;
  /** Paused: the desktop is locked or a secure desktop shows. */
  locked: boolean;
  /** Paused: the config's game is not in front. */
  focus_paused: boolean;
  suspended: boolean;
  foreground: string | null;
  virtual_pad: string | null;
  hidden_devices: number;
  warnings: string[];
  /** `input_access`: on macOS the Accessibility permission keyboard and mouse need; available elsewhere. */
  drivers: { virtual_pads: DriverState; device_hiding: DriverState; input_access: DriverState };
  /** Controllers with slots of their own. */
  device_slots: { id: string; slots: (string | null)[]; active_slot: number }[];
  /** Running profiles, the default session (`owner` null) first. */
  sessions: LiveSession[];
  /** The crosshair drawn over the game. */
  crosshair: Crosshair | null;
  /** The radial menu open over the game, and the sector pointed at. */
  radial: { menu: RadialMenu; pointed: number | null } | null;
  /** The running profile has radial menus. */
  has_radial: boolean;
  pad_overlay: boolean;
  mappings_overlay: boolean;
  /** How often the app-navigation hotkey fired; each change turns controller navigation on or off. */
  app_navigation: number;
}

// ----- library --------------------------------------------------------------------------

export interface GameMeta {
  name: string;
  processes: string[];
  image?: string;
  default_config?: string;
  order?: number;
  /** A note about the game, shown under its name. */
  description?: string;
}

export interface ConfigEntry {
  path: string;
  name: string;
  error: string | null;
}

/** One layer kept to reuse: its settings, mappings and routes (layer ids left out). */
export interface LayerPreset {
  name: string;
  layer?: Layer;
  mappings?: Mapping[];
  analog?: AnalogMapping[];
}

export interface PresetEntry {
  name: string;
  path: string;
}

export interface Game {
  dir: string;
  meta: GameMeta;
  configs: ConfigEntry[];
}

export interface Library {
  games: Game[];
  unsorted: ConfigEntry[];
  errors: string[];
}

/** Where the library is (`library_place`). */
export interface LibraryPlace {
  dir: string;
  default: string;
  is_default: boolean;
  /** In portable mode, the folder beside the app that holds everything. */
  portable: string | null;
}

/**
 * What a folder offered for the library holds: `empty` (the library moves there), `library` (a
 * Doppelpad library, used as it is), `foreign` (other files: refused).
 */
export type LibraryTarget = 'empty' | 'library' | 'foreign';

/** The library after `library_move`. */
export interface LibraryMoved {
  dir: string;
  /** The old folder, when the files moved (not when an existing library is used). */
  moved_from: string | null;
  /** Old files that could not be removed; their copies are in `dir`. */
  left_behind: number;
}

// ----- commands ---------------------------------------------------------------------------

export interface DiagMsg {
  severity: 'info' | 'warning' | 'error';
  path: string;
  message: string;
}

export interface LoadedConfig {
  profile: Profile;
  diagnostics: DiagMsg[];
  read_only: boolean;
}

export interface Catalog {
  keys: string[];
  pad_buttons: PadButtonName[];
  mouse_buttons: MouseControlButton[];
  defaults: {
    profile: Profile;
    mapping: Mapping;
    output: Required<Output>;
    timing: Required<Timing>;
    stick: Required<StickConfig>;
    trigger: TriggerFull;
    analog: Required<AnalogMapping>;
    virtual_stick: VirtualStickFull;
    gyro: Required<GyroSettings>;
    touchpad: Required<TouchpadSettings>;
    crosshair: CrosshairFull;
    magnifier: Magnifier;
    radial_menu: Required<Omit<RadialMenu, 'sectors' | 'name'>>;
    radial_sector: RadialSector;
    light: Required<Light>;
    vibration: Required<Vibration>;
  };
}

/** What to do when reWASD runs as the app starts. */
export type RewasdChoice = 'ask' | 'quit' | 'ignore';

export interface GuiSettings {
  language: string;
  stay_live: boolean;
  hook_buttons: boolean;
  rewasd_at_start: RewasdChoice;
  /** Ask GitHub for a newer version at start-up. */
  check_updates: boolean;
  osd: OsdSettings;
  pad_overlay: PadOverlaySettings;
  mappings_overlay: MappingsOverlaySettings;
  /** The welcome guide was finished or skipped. */
  welcome_done: boolean;
  /** The editor shows the mappings' notes instead of their actions. */
  show_notes: boolean;
  /** Questions the user asked not to be asked again (see `QuestionId`); they count as answered OK. */
  silenced: string[];
  /** Size of the window's contents, in percent (80–150). */
  zoom: number;
  /** The controller drives the app (focus, activate, back) instead of selecting the pressed control. */
  pad_navigation: boolean;
  /** One more tray icon for each connected controller. */
  tray_per_controller: boolean;
  /** The colour of each controller's tray icon (`#rrggbb`) by device id. */
  controller_colors: Record<string, string>;
}

/** Starting points for new configs. */
export type Template = 'virtual_xbox' | 'virtual_ds4' | 'desktop' | 'keyboard_mouse';

export type Corner = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';

/** The live controller overlay. */
export interface PadOverlaySettings {
  corner: Corner;
  /** Percent of 600 logical pixels across. */
  size: number;
  opacity: number;
  /** Seconds without input before it hides (0: stays). */
  hide_after: number;
}

/** The overlay listing the current layer's mappings. */
export interface MappingsOverlaySettings {
  corner: Corner;
  opacity: number;
}

/** On-screen notifications over games. */
export interface OsdSettings {
  enabled: boolean;
  remap: boolean;
  slot: boolean;
  layer: boolean;
  /** Keep the layer message up while a layer other than Main is active. */
  keep_layer: boolean;
  controller: boolean;
  battery: boolean;
  /** Index into the monitors, the primary one first. */
  monitor: number;
  corner: Corner;
  /** Percent. */
  scale: number;
  /** Percent. */
  opacity: number;
  seconds: number;
}

/** What an on-screen notification is about (sent by gui/src-tauri/src/osd.rs). */
export type OsdEvent =
  | { kind: 'remap_on' }
  | { kind: 'remap_off' }
  | { kind: 'slot'; slot: number; config: string | null }
  | { kind: 'layer'; layer: number; name: string | null }
  | { kind: 'connected'; name: string }
  | { kind: 'disconnected'; name: string }
  | { kind: 'battery_low'; name: string; level: number }
  | { kind: 'battery_critical'; name: string; level: number }
  | { kind: 'steam_input'; game: string }
  | { kind: 'test' };

export type OsdMessage = OsdEvent & { id: number; lang: string; opacity: number; scale: number; corner: Corner };

/** What the controller overlay shows (sent by gui/src-tauri/src/pad_overlay.rs). */
export interface PadOverlayContent {
  device: LiveDevice;
  opacity: number;
  corner: Corner;
}

/** What the mappings overlay shows (sent by gui/src-tauri/src/pad_overlay.rs). */
export interface MappingsOverlayContent {
  profile: Profile;
  layer: number;
  family: PadFamily | null;
  lang: string;
  opacity: number;
  corner: Corner;
}

/** A radial menu on screen (sent by gui/src-tauri/src/radial.rs). */
export interface RadialContent {
  menu: RadialMenu;
  pointed: number | null;
  /** Per sector, its image file as a data URL (icons of the app's set stay names). */
  images: (string | null)[];
  /** Where the mouse pointer was as the menu opened, in the window's logical pixels. */
  cursor: [number, number] | null;
}

/** A config made from an imported file (sent by gui/src-tauri/src/commands.rs). */
export interface Imported {
  path: string;
  diagnostics: DiagMsg[];
}

/** Where a launch command sits: a mapping, or a radial menu sector (both counted from 1). */
export type LaunchPlace =
  | { kind: 'mapping'; layer: number; layer_name: string; trigger: Combo }
  | { kind: 'radial'; menu: number; name: string; sector: number; label: string };

/** A program that a config being imported would start (crates/dp-config/src/launches.rs). */
export interface Launch {
  /** The config file, for a game bundle's configs. */
  file: string | null;
  /** The config's name. */
  config: string;
  place: LaunchPlace;
  path: string;
  args: string;
}

/** What of reWASD is running; it keeps the controllers to itself. */
export interface RewasdStatus {
  /** Display name of its service while that runs. */
  service: string | null;
  /** Its other running programs. */
  programs: string[];
}

// ----- controller hotkeys --------------------------------------------------------------

/** Controller hotkeys (app settings): pad buttons held together; empty = off. */
export interface Hotkeys {
  hold_ms: number;
  slot1: PadButtonName[];
  slot2: PadButtonName[];
  slot3: PadButtonName[];
  slot4: PadButtonName[];
  remap: PadButtonName[];
  stop_macros: PadButtonName[];
  emergency_stop: PadButtonName[];
  pad_overlay: PadButtonName[];
  mappings_overlay: PadButtonName[];
  app_navigation: PadButtonName[];
  /** Confirm each hotkey with a short rumble pattern on the controller. */
  rumble: boolean;
}

export type HotkeyField = Exclude<keyof Hotkeys, 'hold_ms' | 'rumble'>;

/** One running profile: the shared slots' (`owner` null) or a controller's own. */
export interface LiveSession {
  owner: string | null;
  /** The controller it reads now. */
  device: string | null;
  profile: string;
  profile_path: string | null;
  layer: number;
  virtual_pad: VirtualPadKind | null;
  /** What the session sends right now. */
  output: SessionOutput;
}

/** A session's output right now: its virtual pad's state, and the keys and mouse buttons it holds. */
export interface SessionOutput {
  pad: PadState | null;
  keys: string[];
  mouse_buttons: string[];
}
