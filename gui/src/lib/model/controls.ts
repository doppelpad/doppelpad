// Display names for controls, per controller family.

import { has, t } from '../i18n/index.svelte';
import type { ControlName, PadFamily } from '../types';

const DIRS = ['Up', 'Down', 'Left', 'Right'] as const;

/** "LeftStickUp" → ["LeftStick", "Up"], else null. */
export function splitStickDir(c: ControlName): [string, string] | null {
  for (const stick of ['LeftStick', 'RightStick']) {
    for (const d of DIRS) if (c === `${stick}${d}`) return [stick, d];
  }
  return null;
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

/** Human-readable name of a control for the given controller family. */
export function controlLabel(c: ControlName, family: PadFamily | null): string {
  if (c.startsWith('Key:')) return c.slice(4);
  if (c.startsWith('Mouse:')) return t(`controls.mouse.${c.slice(6).toLowerCase()}`);
  if (c.startsWith('Wheel:')) return `${t('picker.mouse_wheel')} ${t(`dirs.${c.slice(6).toLowerCase()}`)}`;
  if (c.startsWith('MouseMove:')) return `${t('picker.mouse_move')} ${t(`dirs.${c.slice(10).toLowerCase()}`)}`;
  const sd = splitStickDir(c);
  if (sd) return `${t(`controls.${sd[0]}`)} ${t(`dirs.${sd[1].toLowerCase()}`)}`;
  if (c.startsWith('Touch') && c !== 'Touchpad') return `Touchpad ${c.slice(5)}`;
  if (c.startsWith('Gyro')) return `Gyro ${t(`dirs.${c.slice(4).toLowerCase()}`)}`;
  const named = familyName(c, family);
  if (named) return named;
  if (has(`controls.${c}`)) return t(`controls.${c}`);
  return c;
}

/** Short name for compact labels (e.g. on the controller drawing). */
export function controlShort(c: ControlName, family: PadFamily | null): string {
  return controlLabel(c, family);
}
