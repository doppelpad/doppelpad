// Display names for controls, per controller family.

import { has, t } from '../i18n/index.svelte';
import type { AnalogSource, ControlName, PadFamily } from '../types';
import type { Side } from './analog';

/** The four directions of sticks, mouse motion and the gyro. */
export const CARDINALS = ['Up', 'Down', 'Left', 'Right'] as const;
/** Diagonals; sticks with separate diagonals, the touchpad and mouse motion have them. */
export const DIAGONALS = ['UpLeft', 'UpRight', 'DownLeft', 'DownRight'] as const;
export const DIRS8 = [...CARDINALS, ...DIAGONALS] as const;

export const SIDE_NAMES: readonly Side[] = ['left', 'right'];
/** Per side: its stick's direction controls (`LeftStickUp`), its click, its trigger and the stick as a route source. */
export const SIDES = {
  left: { stick: 'LeftStick', click: 'LS', trigger: 'LT', source: 'left_stick' },
  right: { stick: 'RightStick', click: 'RS', trigger: 'RT', source: 'right_stick' },
} as const satisfies Record<Side, { stick: string; click: ControlName; trigger: ControlName; source: AnalogSource }>;

/** Key of a direction in the `dirs` catalog (`UpLeft` → `up_left`). */
export function dirKey(d: string): string {
  return d.replace(/([a-z])([A-Z])/g, '$1_$2').toLowerCase();
}

/** Touchpad regions; the controls are `TouchUp` … `TouchCenter`. */
export const TOUCH_REGIONS = ['Up', 'Down', 'Left', 'Right', 'Center'] as const;
/** Gyro directions; the controls are `GyroUp` … `GyroLeanRight`, and their zones. */
export const GYRO_DIRS = [...CARDINALS, 'LeanLeft', 'LeanRight'] as const;

/** Touchpad gestures; the controls are `TouchTap` … `TouchZoomOut`. */
export const TOUCH_GESTURES = [
  'Tap',
  'TapLeft',
  'TapCenter',
  'TapRight',
  'MultiTap',
  'ZoomIn',
  'ZoomOut',
  'SwipeUp',
  'SwipeDown',
  'SwipeLeft',
  'SwipeRight',
  'Swipe2Up',
  'Swipe2Down',
  'Swipe2Left',
  'Swipe2Right',
] as const;

/** Touchpad controls: regions (of each half on a split pad), then gestures. */
export const touchControls = (split = false): ControlName[] => [
  ...(split
    ? (['Left', 'Right'] as const).flatMap((h) => [
        ...[...TOUCH_REGIONS, ...DIAGONALS].map((r) => `Trackpad${h}${r}`),
        `Trackpad${h}Tap`,
        `Trackpad${h}Click`,
      ])
    : [...TOUCH_REGIONS, ...DIAGONALS].map((r) => `Touch${r}`)),
  ...TOUCH_GESTURES.map((g) => `Touch${g}`),
];
export const gyroControls = (): ControlName[] => GYRO_DIRS.map((d) => `Gyro${d}`);

/** Zones of a stick's deflection or a trigger's pull, in profile syntax. */
export const ZONES = ['Low', 'Med', 'High'] as const;
export type ZoneName = (typeof ZONES)[number];

/**
 * The parts of a direction control with zones (a stick, the gyro, mouse motion) after its
 * prefix: the directions, the zones in any direction, and every direction in every zone.
 */
export const zonedParts = (dirs: readonly string[]): string[] => [...dirs, ...ZONES, ...dirs.flatMap((d) => ZONES.map((z) => d + z))];

/** A trackpad half's parts: the directions, its centre, a tap and a click (`TrackpadLeftTap`). */
export const TRACKPAD_PARTS = [...DIRS8, 'Center', 'Tap', 'Click'] as const;

export type Dir8 = (typeof DIRS8)[number];
export type GyroDir = (typeof GYRO_DIRS)[number];
export type TouchRegion = (typeof TOUCH_REGIONS)[number] | (typeof DIAGONALS)[number];
export type TouchGesture = (typeof TOUCH_GESTURES)[number];
export type TrackpadPart = (typeof TRACKPAD_PARTS)[number];

/** A control name split into its parts, as the engine's `Control` enum has them (`dp-core`). */
export type ParsedControl =
  /** A pad button, or a name nothing else claims. */
  | { kind: 'button' }
  | { kind: 'stick_dir'; side: Side; dir: Dir8 }
  /** The stick deflected into a zone, in a direction or (`null`) any. */
  | { kind: 'stick_zone'; side: Side; dir: Dir8 | null; zone: ZoneName }
  | { kind: 'trigger_press'; side: Side }
  | { kind: 'trigger_zone'; side: Side; zone: ZoneName }
  | { kind: 'trigger_full'; side: Side }
  | { kind: 'touch'; region: TouchRegion }
  | { kind: 'gesture'; gesture: TouchGesture }
  | { kind: 'trackpad'; half: 'Left' | 'Right'; part: TrackpadPart }
  | { kind: 'gyro'; dir: GyroDir }
  | { kind: 'gyro_zone'; dir: GyroDir | null; zone: ZoneName }
  | { kind: 'key'; key: string }
  | { kind: 'mouse'; button: string }
  | { kind: 'wheel'; dir: string }
  | { kind: 'mouse_move'; dir: Dir8 }
  | { kind: 'mouse_zone'; dir: Dir8 | null; zone: ZoneName };

const alt = (names: readonly string[]) => names.join('|');
const DIR8 = alt(DIRS8);
const ZONE = alt(ZONES);
const STICK = new RegExp(`^(Left|Right)Stick(${DIR8})?(${ZONE})?$`);
const TRIGGER = new RegExp(`^(LT|RT)(Full|${ZONE})?$`);
const GYRO = new RegExp(`^Gyro(${alt(GYRO_DIRS)})?(${ZONE})?$`);
const TOUCH = new RegExp(`^Touch(${alt([...TOUCH_REGIONS, ...DIAGONALS])})$`);
const GESTURE = new RegExp(`^Touch(${alt(TOUCH_GESTURES)})$`);
const TRACKPAD = new RegExp(`^Trackpad(Left|Right)(${alt(TRACKPAD_PARTS)})$`);
const MOUSE_MOVE = new RegExp(`^MouseMove:(${DIR8})?(${ZONE})?$`);

const sideOf = (s: string): Side => (s === 'Left' || s === 'LT' ? 'left' : 'right');

/** Split a control name into its parts; `null` for a `MouseMove:` name without a direction or zone. */
export function parseControl(c: ControlName): ParsedControl | null {
  if (c.startsWith('Key:')) return { kind: 'key', key: c.slice(4) };
  if (c.startsWith('Mouse:')) return { kind: 'mouse', button: c.slice(6) };
  if (c.startsWith('Wheel:')) return { kind: 'wheel', dir: c.slice(6) };
  if (c.startsWith('MouseMove:')) {
    const [, dir, zone] = MOUSE_MOVE.exec(c) ?? [];
    if (zone) return { kind: 'mouse_zone', dir: (dir as Dir8 | undefined) ?? null, zone: zone as ZoneName };
    return dir ? { kind: 'mouse_move', dir: dir as Dir8 } : null;
  }
  let m = STICK.exec(c);
  if (m) {
    const [, stick, dir, zone] = m;
    const side = sideOf(stick);
    if (zone) return { kind: 'stick_zone', side, dir: (dir as Dir8 | undefined) ?? null, zone: zone as ZoneName };
    if (dir) return { kind: 'stick_dir', side, dir: dir as Dir8 };
  }
  if ((m = TRIGGER.exec(c))) {
    const [, trigger, part] = m;
    const side = sideOf(trigger);
    if (!part) return { kind: 'trigger_press', side };
    return part === 'Full' ? { kind: 'trigger_full', side } : { kind: 'trigger_zone', side, zone: part as ZoneName };
  }
  if ((m = GYRO.exec(c))) {
    const [, dir, zone] = m;
    if (zone) return { kind: 'gyro_zone', dir: (dir as GyroDir | undefined) ?? null, zone: zone as ZoneName };
    if (dir) return { kind: 'gyro', dir: dir as GyroDir };
  }
  if ((m = TOUCH.exec(c))) return { kind: 'touch', region: m[1] as TouchRegion };
  if ((m = GESTURE.exec(c))) return { kind: 'gesture', gesture: m[1] as TouchGesture };
  if ((m = TRACKPAD.exec(c))) return { kind: 'trackpad', half: m[1] as 'Left' | 'Right', part: m[2] as TrackpadPart };
  return { kind: 'button' };
}

/** "LeftStickUp" → ["LeftStick", "Up"], else null. */
export function splitStickDir(c: ControlName): [string, string] | null {
  const p = parseControl(c);
  return p?.kind === 'stick_dir' ? [SIDES[p.side].stick, p.dir] : null;
}

/** Where a stick's or trigger's zone thresholds live. */
export interface PairSource {
  kind: 'stick' | 'trigger';
  side: Side;
}

/** A control that has zones, split into the control without its zone and the zone. */
export interface ZoneParts<S = PairSource | { kind: 'gyro' }> {
  /** `LeftStickUp`, `RT`, or `LeftStick` for the stick in any direction (not a control itself). */
  base: string;
  zone: ZoneName | null;
  /** Where the zone thresholds live; the gyro's are in the profile's gyro settings. */
  source: S;
}

/**
 * Zone parts of a stick direction, a trigger, a gyro direction or one of their zone controls
 * (`LeftStickUpHigh`, `RTMed`, `LeftStickLow` = any direction); null for every other control.
 */
export function zoneParts(c: ControlName): ZoneParts | null {
  const p = parseControl(c);
  switch (p?.kind) {
    case 'gyro':
    case 'gyro_zone':
      return { base: `Gyro${p.dir ?? ''}`, zone: p.kind === 'gyro_zone' ? p.zone : null, source: { kind: 'gyro' } };
    case 'stick_dir':
    case 'stick_zone':
      return {
        base: SIDES[p.side].stick + (p.dir ?? ''),
        zone: p.kind === 'stick_zone' ? p.zone : null,
        source: { kind: 'stick', side: p.side },
      };
    case 'trigger_press':
    case 'trigger_zone':
      return {
        base: SIDES[p.side].trigger,
        zone: p.kind === 'trigger_zone' ? p.zone : null,
        source: { kind: 'trigger', side: p.side },
      };
    default:
      return null;
  }
}

/** The base control in a zone, or the base itself for `null`. */
export function withZone(base: string, zone: ZoneName | null): ControlName {
  return base + (zone ?? '');
}

/** Whether a zone base is the stick in any direction, which only exists per zone. */
export function isAnyDirection(base: string): boolean {
  return base === 'LeftStick' || base === 'RightStick' || base === 'Gyro';
}

/** Short tag of a zone for compact summaries: the first letter of its name ("H" for high). */
export function zoneTag(zone: ZoneName): string {
  return t(`zones.${zone.toLowerCase()}`).charAt(0);
}

/** Short tag of a trigger's full pull for compact summaries. */
export function fullPullTag(): string {
  return t('zones.full').charAt(0);
}

/** The base control and all of its zone controls. */
export function zoneFamily(base: string): ControlName[] {
  const zones = ZONES.map((z) => withZone(base, z));
  return isAnyDirection(base) ? zones : [base, ...zones];
}

/** Catalogs with family-specific control names (`controls.<ns>.*`), most specific first. */
function familyNamespaces(family: PadFamily | null): string[] {
  switch (family) {
    case 'dual_sense':
    case 'dual_shock4':
      return ['ps'];
    case 'switch_pro':
      return ['nintendo'];
    case 'joy_con':
      return ['joycon', 'nintendo'];
    case 'game_cube':
      return ['gamecube', 'nintendo'];
    default:
      return [];
  }
}

function familyName(c: ControlName, family: PadFamily | null): string | null {
  for (const ns of familyNamespaces(family)) {
    if (has(`controls.${ns}.${c}`)) return t(`controls.${ns}.${c}`);
  }
  return null;
}

/** Name of a zone base: a control, or the stick in any direction. */
export function zoneBaseLabel(base: string, family: PadFamily | null): string {
  return isAnyDirection(base) ? t('zones.any_direction', { stick: t(`controls.${base}`) }) : controlLabel(base, family);
}

/** Human-readable name of a control for the given controller family. */
export function controlLabel(c: ControlName, family: PadFamily | null): string {
  const p = parseControl(c);
  const zoneIn = (zone: ZoneName) => t(`zones.in.${zone.toLowerCase()}`);
  switch (p?.kind) {
    case 'key':
      return p.key;
    case 'mouse':
      return t(`controls.mouse.${p.button.toLowerCase()}`);
    case 'wheel':
      return `${t('picker.mouse_wheel')} ${t(`dirs.${p.dir.toLowerCase()}`)}`;
    case 'mouse_move':
    case 'mouse_zone': {
      const dir = p.dir ? ` ${t(`dirs.${dirKey(p.dir)}`)}` : '';
      const zone = p.kind === 'mouse_zone' ? ` · ${zoneIn(p.zone)}` : '';
      return `${t('picker.mouse_move')}${dir}${zone}`;
    }
    case 'stick_zone':
    case 'trigger_zone':
    case 'gyro_zone':
      return `${zoneBaseLabel(zoneParts(c)!.base, family)} · ${zoneIn(p.zone)}`;
    case 'stick_dir':
      return `${t(`controls.${SIDES[p.side].stick}`)} ${t(`dirs.${dirKey(p.dir)}`)}`;
    case 'touch':
      return t(`controls.touch.${dirKey(p.region)}`);
    case 'gesture':
      return t(`controls.gesture.${p.gesture}`);
    case 'trackpad': {
      const own = p.part === 'Tap' || p.part === 'Click' || p.part === 'Center';
      const part = own ? t(`controls.trackpad_${p.part.toLowerCase()}`) : t(`dirs.${dirKey(p.part)}`);
      return t('controls.trackpad', { half: t(`binding.half_${p.half.toLowerCase()}`), part });
    }
    case 'gyro':
      return t('controls.gyro', { dir: t(`dirs.${dirKey(p.dir)}`) });
  }
  // A `MouseMove:` name without a direction or zone.
  if (!p) return t('picker.mouse_move');
  const named = familyName(c, family);
  if (named) return named;
  if (has(`controls.${c}`)) return t(`controls.${c}`);
  return c;
}

/** Like `controlLabel`, but mouse buttons say so, for lists that mix devices. */
export function controlLabelMixed(c: ControlName, family: PadFamily | null): string {
  const name = controlLabel(c, family);
  return c.startsWith('Mouse:') ? `${t('picker.mouse')}: ${name}` : name;
}

/** The analog settings page of a stick or trigger control, its zones included. */
export function analogTabOf(c: ControlName): Side | 'triggers' | null {
  for (const side of SIDE_NAMES) if (c === SIDES[side].click || c.startsWith(SIDES[side].stick)) return side;
  if (TRIGGER.test(c)) return 'triggers';
  return null;
}

/** The trigger (`LT`/`RT`) of a full-pull control (`LTFull`), else null. */
export function fullPullBase(c: ControlName): string | null {
  const p = parseControl(c);
  return p?.kind === 'trigger_full' ? SIDES[p.side].trigger : null;
}
