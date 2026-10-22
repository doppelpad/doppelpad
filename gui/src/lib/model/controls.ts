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
export const TOUCH_GESTURES = ['Tap', 'TapLeft', 'TapCenter', 'TapRight', 'MultiTap', 'SwipeUp', 'SwipeDown', 'SwipeLeft', 'SwipeRight', 'Swipe2Up', 'Swipe2Down', 'Swipe2Left', 'Swipe2Right', 'ZoomIn', 'ZoomOut'] as const;

/** Touchpad controls: regions (of each half on a split pad), then gestures. */
export const touchControls = (split = false): ControlName[] => [
  ...(split
    ? (['Left', 'Right'] as const).flatMap((h) => [...[...TOUCH_REGIONS, ...DIAGONALS].map((r) => `Trackpad${h}${r}`), `Trackpad${h}Tap`, `Trackpad${h}Click`])
    : [...TOUCH_REGIONS, ...DIAGONALS].map((r) => `Touch${r}`)),
  ...TOUCH_GESTURES.map((g) => `Touch${g}`),
];
export const gyroControls = (): ControlName[] => GYRO_DIRS.map((d) => `Gyro${d}`);

/** Zones of a stick's deflection or a trigger's pull, in profile syntax. */
export const ZONES = ['Low', 'Med', 'High'] as const;
export type ZoneName = (typeof ZONES)[number];

/** "LeftStickUp" → ["LeftStick", "Up"], else null. */
export function splitStickDir(c: ControlName): [string, string] | null {
  for (const stick of ['LeftStick', 'RightStick']) {
    for (const d of DIRS8) if (c === `${stick}${d}`) return [stick, d];
  }
  return null;
}

/** A control that has zones, split into the control without its zone and the zone. */
export interface ZoneParts {
  /** `LeftStickUp`, `RT`, or `LeftStick` for the stick in any direction (not a control itself). */
  base: string;
  zone: ZoneName | null;
  /** Where the zone thresholds live. */
  source: { kind: 'stick' | 'trigger' | 'gyro'; side: 'left' | 'right' };
}

const ZONED = /^(?:(Left|Right)Stick(UpLeft|UpRight|DownLeft|DownRight|Up|Down|Left|Right)?|(LT|RT))(Low|Med|High)?$/;
const GYRO_ZONED = /^Gyro(LeanLeft|LeanRight|Up|Down|Left|Right)?(Low|Med|High)?$/;

/**
 * Zone parts of a stick direction, a trigger or one of their zone controls (`LeftStickUpHigh`,
 * `RTMed`, `LeftStickLow` = any direction); null for every other control.
 */
export function zoneParts(c: ControlName): ZoneParts | null {
  const g = GYRO_ZONED.exec(c);
  if (g) {
    const [, dir, zone] = g;
    if (!dir && !zone) return null;
    // Gyro zones live in the profile's gyro settings, not per side.
    return { base: `Gyro${dir ?? ''}`, zone: (zone as ZoneName | undefined) ?? null, source: { kind: 'gyro', side: 'left' } };
  }
  const m = ZONED.exec(c);
  if (!m) return null;
  const [, stick, dir, trigger, zone] = m;
  if (stick && !dir && !zone) return null;
  const side = (stick ?? (trigger === 'LT' ? 'Left' : 'Right')).toLowerCase() as 'left' | 'right';
  return {
    base: stick ? `${stick}Stick${dir ?? ''}` : trigger,
    zone: (zone as ZoneName | undefined) ?? null,
    source: { kind: stick ? 'stick' : 'trigger', side },
  };
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
  if (c.startsWith('Key:')) return c.slice(4);
  if (c.startsWith('Mouse:')) return t(`controls.mouse.${c.slice(6).toLowerCase()}`);
  if (c.startsWith('Wheel:')) return `${t('picker.mouse_wheel')} ${t(`dirs.${c.slice(6).toLowerCase()}`)}`;
  if (c.startsWith('MouseMove:')) {
    const m = /^MouseMove:(UpLeft|UpRight|DownLeft|DownRight|Up|Down|Left|Right)?(Low|Med|High)?$/.exec(c);
    const dir = m?.[1] ? ` ${t(`dirs.${dirKey(m[1])}`)}` : '';
    const zone = m?.[2] ? ` · ${t(`zones.in.${m[2].toLowerCase()}`)}` : '';
    return `${t('picker.mouse_move')}${dir}${zone}`;
  }
  const zp = zoneParts(c);
  if (zp?.zone) return `${zoneBaseLabel(zp.base, family)} · ${t(`zones.in.${zp.zone.toLowerCase()}`)}`;
  const sd = splitStickDir(c);
  if (sd) return `${t(`controls.${sd[0]}`)} ${t(`dirs.${dirKey(sd[1])}`)}`;
  const touch = /^Touch(UpLeft|UpRight|DownLeft|DownRight|Up|Down|Left|Right|Center)$/.exec(c);
  if (touch) return t(`controls.touch.${dirKey(touch[1])}`);
  const gesture = /^Touch(Tap|TapLeft|TapCenter|TapRight|MultiTap|Swipe2?(?:Up|Down|Left|Right)|ZoomIn|ZoomOut)$/.exec(c);
  if (gesture) return t(`controls.gesture.${gesture[1]}`);
  const pad = /^Trackpad(Left|Right)(UpLeft|UpRight|DownLeft|DownRight|Up|Down|Left|Right|Center|Tap|Click)$/.exec(c);
  if (pad) {
    const own = ['Tap', 'Click', 'Center'].includes(pad[2]);
    const part = own ? t(`controls.trackpad_${pad[2].toLowerCase()}`) : t(`dirs.${dirKey(pad[2])}`);
    return t('controls.trackpad', { half: t(`binding.half_${pad[1].toLowerCase()}`), part });
  }
  const gyro = /^Gyro(LeanLeft|LeanRight|Up|Down|Left|Right)$/.exec(c);
  if (gyro) return t('controls.gyro', { dir: t(`dirs.${dirKey(gyro[1])}`) });
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

/** Short name for compact labels (e.g. on the controller drawing). */
export function controlShort(c: ControlName, family: PadFamily | null): string {
  return controlLabel(c, family);
}
