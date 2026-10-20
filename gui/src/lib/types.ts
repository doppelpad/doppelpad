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
  | 'A' | 'B' | 'X' | 'Y' | 'LB' | 'RB' | 'Back' | 'Start' | 'LS' | 'RS'
  | 'DpadUp' | 'DpadDown' | 'DpadLeft' | 'DpadRight' | 'Guide' | 'Touchpad' | 'Misc'
  | 'PaddleUL' | 'PaddleUR' | 'PaddleLL' | 'PaddleLR' | 'Misc2';
export type VirtualPadType = 'xbox360' | 'dual_shock4' | 'none';
export type JumpMode = 'hold' | 'toggle' | 'switch';

export type Command =
  | 'remap_off'
  | 'toggle_gyro'
  | 'gyro_on'
  | 'gyro_off'
  | 'shutdown'
  | 'restart'
  | 'sleep'
  | 'hibernate'
  | 'log_off'
  | 'screenshot'
  | 'close_window'
  | { launch: { path: string; args?: string } }
  | { unsupported: string };

/** Accelerometer axis and the direction a shake starts in. */
export type ShakeAxis = '+x' | '-x' | '+y' | '-y' | '+z' | '-z';

export type Action =
  | { key: string; action?: Press }
  | { pad: PadButtonName; action?: Press }
  | { mouse: MouseButtonName; action?: Press }
  | { wheel: WheelDir; clicks?: number }
  | { stick: StickName; x: number; y: number; action?: Press }
  | { trigger: TriggerName; value?: number; action?: Press }
  | { mouse_move: Dir; speed?: number }
  | { pause: number }
  | { rumble: number; low?: number; high?: number }
  | { shake: ShakeAxis }
  | { command: Command }
  | { break: true };

export interface Jump {
  layer: number;
  mode?: JumpMode;
  delay_ms?: number;
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

export interface TriggerConfig {
  deadzone?: number;
  zones?: TriggerZones;
  full_pull?: number;
}

export interface Sticks {
  left?: StickConfig;
  right?: StickConfig;
}

export interface Triggers {
  left?: TriggerConfig;
  right?: TriggerConfig;
}

export type AnalogSource = 'left_stick' | 'right_stick' | 'mouse' | 'gyro' | 'touchpad';
export type AnalogTarget = 'mouse' | 'left_stick' | 'right_stick';

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
  anti_deadzone?: number;
  smoothing_ms?: number;
  decay_ms?: number;
  trackball_friction?: number;
  while_held?: ControlName[];
  consume?: boolean;
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

export interface VirtualSticks {
  left?: VirtualStick;
  right?: VirtualStick;
}

export interface VirtualDevices {
  keyboard?: { repeat?: KeyRepeat };
  sticks?: VirtualSticks;
  motion?: { shake_g?: number; shake_ms?: number };
}

export interface Profile {
  name?: string;
  description?: string;
  processes?: string[];
  output?: Output;
  timing?: Timing;
  sticks?: Sticks;
  triggers?: Triggers;
  layers?: Layer[];
  mappings: Mapping[];
  analog?: AnalogMapping[];
  virtual?: VirtualDevices;
}

// ----- live state ---------------------------------------------------------------------

export interface PadState {
  /** Bitset; bit index = position in `Catalog.pad_buttons`. */
  buttons: number;
  /** LeftX, LeftY, RightX, RightY (−1..1, up positive), LeftTrigger, RightTrigger (0..1). */
  axes: number[];
  motion: { gyro: number[]; accel: number[] } | null;
  touch: ({ x: number; y: number } | null)[];
}

export type PadFamily =
  | 'xbox'
  | 'dual_shock4'
  | 'dual_sense'
  | 'switch_pro'
  | 'joy_con'
  | 'game_cube'
  | 'generic';

export interface LiveDevice {
  id: string;
  name: string;
  family: PadFamily;
  backend: string;
  vendor_id: number;
  product_id: number;
  state: PadState;
}

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
  slots: (string | null)[];
  active_slot: number;
  remap_on: boolean;
  auto_switch: boolean;
  suspended: boolean;
  foreground: string | null;
  virtual_pad: string | null;
  hidden_devices: number;
  warnings: string[];
  drivers: { virtual_pads: DriverState; device_hiding: DriverState };
  /** Controllers with slots of their own. */
  device_slots: { id: string; slots: (string | null)[]; active_slot: number }[];
  /** Running profiles, the default session (`owner` null) first. */
  sessions: LiveSession[];
}

// ----- library --------------------------------------------------------------------------

export interface GameMeta {
  name: string;
  processes: string[];
  image?: string;
  default_config?: string;
  order?: number;
}

export interface ConfigEntry {
  path: string;
  name: string;
  error: string | null;
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
  mouse_buttons: string[];
  defaults: {
    profile: Profile;
    mapping: Mapping;
    output: Required<Output>;
    timing: Required<Timing>;
    stick: Required<StickConfig>;
    trigger: Required<TriggerConfig>;
    analog: Required<AnalogMapping>;
    virtual_stick: Required<VirtualStick>;
  };
}

/** What to do when reWASD runs as the app starts. */
export type RewasdChoice = 'ask' | 'quit' | 'ignore';

export interface GuiSettings {
  language: string;
  stay_live: boolean;
  hook_buttons: boolean;
  rewasd_at_start: RewasdChoice;
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
}

export type HotkeyField = Exclude<keyof Hotkeys, 'hold_ms'>;

/** One running profile: the shared slots' (`owner` null) or a controller's own. */
export interface LiveSession {
  owner: string | null;
  /** The controller it reads now. */
  device: string | null;
  profile: string;
  profile_path: string | null;
  layer: number;
  virtual_pad: string | null;
}
