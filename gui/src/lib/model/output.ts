// Output settings the output panel edits from typed text: how turns become mouse counts, the
// virtual controller's shake, and the speeds where the physical mouse's zones start.

import type { Light, Output, Profile, Vibration } from '../types';
import { setVirtualDevice } from './layers';
import { setOrDrop, setRising } from './util';

/** Turn settings of the virtual mouse, at their defaults. */
export const TURN_DEFAULTS = { counts_per_turn: 18000, turn_ms: 100, wheel_repeat_ms: 0 };
/** Shake actions: strength (g) and length (ms), at their defaults. */
export const SHAKE_DEFAULTS = { shake_g: 2, shake_ms: 150 };
/** Speeds (counts/s) where the physical mouse's zones start, by default. */
export const MOUSE_ZONE_DEFAULTS: [number, number, number] = [200, 1000, 3000];

/** `values`, or `undefined` when each of `defaults`' fields holds its default (left out then). */
export function pruneDefaults<T extends object>(values: T, defaults: Partial<T>): T | undefined {
  const d = defaults as Record<string, unknown>;
  return Object.keys(d).every((k) => (values as Record<string, unknown>)[k] === d[k]) ? undefined : values;
}

/** One turn setting from typed text; text that is not a usable value restores the default. */
export function withTurn(p: Profile, field: keyof typeof TURN_DEFAULTS, raw: string): Profile {
  const turn = { ...TURN_DEFAULTS, ...p.virtual?.mouse };
  const n = Math.round(Number(raw.replace(',', '.')));
  const ok = field === 'counts_per_turn' ? n > 0 : n >= 0 && n <= 5000;
  const next = { ...turn, [field]: Number.isFinite(n) && ok ? n : TURN_DEFAULTS[field] };
  return setVirtualDevice(p, 'mouse', pruneDefaults(next, TURN_DEFAULTS));
}

/** One shake setting from typed text; text that is not a positive number restores the default. */
export function withShake(p: Profile, field: keyof typeof SHAKE_DEFAULTS, raw: string): Profile {
  const shake = { ...SHAKE_DEFAULTS, ...p.virtual?.motion };
  const n = Number(raw.replace(',', '.'));
  const v = field === 'shake_ms' ? Math.round(n) : n;
  const next = { ...shake, [field]: Number.isFinite(v) && v > 0 ? v : SHAKE_DEFAULTS[field] };
  return setVirtualDevice(p, 'motion', pruneDefaults(next, SHAKE_DEFAULTS));
}

/** The speed where mouse zone `i` starts, from typed text; the others move to stay rising. */
export function withMouseZone(p: Profile, i: number, raw: string): Profile {
  const n = Math.round(Number(raw));
  if (!Number.isFinite(n) || n <= 0) return p;
  const zones = setRising(p.mouse?.zones ?? MOUSE_ZONE_DEFAULTS, i, n);
  return setOrDrop(p, 'mouse', zones.every((v, j) => v === MOUSE_ZONE_DEFAULTS[j]) ? undefined : { zones });
}

/** The virtual controller and input settings with the defaults filled in. */
export function outputConfig(own: Output | undefined, def: Required<Output>): Required<Output> {
  return { ...def, ...own };
}

/** A light bar with the defaults filled in. */
export function lightConfig(own: Light | undefined, def: Required<Light>): Required<Light> {
  return { ...def, ...own };
}

/** Rumble settings with the defaults filled in. */
export function vibrationConfig(own: Vibration | undefined, def: Required<Vibration>): Required<Vibration> {
  return { ...def, ...own };
}
