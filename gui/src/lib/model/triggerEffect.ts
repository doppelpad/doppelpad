// DualSense trigger effects as the editor offers them: the kinds, a fresh effect of each,
// presets, and the raw bytes as hex.

import type { TriggerEffect } from '../types';
import { clone } from './util';

export type EffectKind = TriggerEffect['kind'];

/** The kinds in the order the editor lists them. */
export const EFFECT_KINDS: EffectKind[] = ['resistance', 'weapon', 'vibration', 'off', 'raw'];

const FRESH: Record<EffectKind, TriggerEffect> = {
  off: { kind: 'off' },
  resistance: { kind: 'resistance', start: 0, strength: 4 },
  weapon: { kind: 'weapon', start: 4, end: 6, strength: 8 },
  vibration: { kind: 'vibration', start: 0, amplitude: 4, frequency: 20 },
  raw: { kind: 'raw', bytes: [0x05, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
};

/** A new effect of `kind` with values that are felt at once. */
export const freshEffect = (kind: EffectKind): TriggerEffect => clone(FRESH[kind]);

export const EFFECT_PRESETS: { id: string; effect: TriggerEffect }[] = [
  { id: 'soft', effect: { kind: 'resistance', start: 0, strength: 2 } },
  { id: 'medium', effect: { kind: 'resistance', start: 0, strength: 4 } },
  { id: 'hard', effect: { kind: 'resistance', start: 0, strength: 7 } },
  { id: 'half', effect: { kind: 'resistance', start: 5, strength: 6 } },
  { id: 'rifle', effect: { kind: 'weapon', start: 4, end: 6, strength: 8 } },
  { id: 'pulse', effect: { kind: 'vibration', start: 0, amplitude: 4, frequency: 10 } },
  { id: 'rumble', effect: { kind: 'vibration', start: 0, amplitude: 6, frequency: 40 } },
];

/** Whether two effects are the same, field by field. */
export function sameEffect(a: TriggerEffect | undefined, b: TriggerEffect | undefined): boolean {
  if (!a || !b) return a === b;
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  const keys = Object.keys(x);
  return (
    keys.length === Object.keys(y).length &&
    keys.every((k) => {
      const u = x[k];
      const v = y[k];
      return Array.isArray(u) && Array.isArray(v) ? u.length === v.length && u.every((n, i) => n === v[i]) : u === v;
    })
  );
}

/** The effect starting at `start`; a weapon's end stays at least one step behind it. */
export function withEffectStart(e: TriggerEffect, start: number): TriggerEffect {
  if (e.kind === 'weapon') return { ...e, start, end: Math.max(start + 1, e.end) };
  return 'start' in e ? { ...e, start } : e;
}

/** Bytes as hex pairs ("05 00 1f"). */
export const effectHex = (bytes: number[]) => bytes.map((b) => b.toString(16).padStart(2, '0')).join(' ');

/** Eleven bytes typed as hex (separated by spaces or commas), or `null` when the text is not that. */
export function parseEffectHex(text: string): number[] | null {
  const bytes = text
    .trim()
    .split(/[\s,]+/)
    .map((h) => parseInt(h, 16));
  return bytes.length === 11 && bytes.every((b) => Number.isInteger(b) && b >= 0 && b <= 255) ? bytes : null;
}
