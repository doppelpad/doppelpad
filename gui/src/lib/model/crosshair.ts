// Crosshair shapes as SVG parts, and edits of a profile's crosshairs. Coordinates are logical
// pixels around the centre (0, 0).

import { t } from '../i18n/index.svelte';
import type { Crosshair, CrosshairFull, CrosshairShape, Profile } from '../types';

export const CROSSHAIR_SHAPES: CrosshairShape[] = [
  'cross',
  'cross_gap',
  'dot',
  'circle',
  'circle_dot',
  'circle_cross',
  'x',
  't',
  'chevron',
  'square',
  'brackets',
];

export const MAX_CROSSHAIRS = 10;

/** One stroke of a shape: a polyline, or a circle (`fill`: a dot). */
export type Part = { points: [number, number][] } | { r: number; fill: boolean };

/** A crosshair with the defaults filled in, and its layers as a list (empty when none are set). */
export function crosshairConfig(c: Crosshair, def: CrosshairFull | undefined): CrosshairFull & { layers: number[] } {
  return { ...(def as CrosshairFull), ...c, layers: c.layers ?? [] };
}

/** Line width of a crosshair: `thickness` percent of a fifth of its size, at least 1 px. */
export function lineWidth(c: CrosshairFull): number {
  return Math.max(1, (c.size / 5) * (c.thickness / 100));
}

/** The strokes that draw `shape` at `size` with lines `w` wide. */
export function crosshairParts(shape: CrosshairShape, size: number, w: number): Part[] {
  const r = size / 2;
  const gap = r * 0.35;
  const line = (x1: number, y1: number, x2: number, y2: number): Part => ({ points: [[x1, y1], [x2, y2]] });
  const dot: Part = { r: Math.max(w, size / 12), fill: true };
  const ring: Part = { r: r - w / 2, fill: false };
  const arms = (from: number, top = true): Part[] => [
    line(-r, 0, -from, 0),
    line(from, 0, r, 0),
    line(0, from, 0, r),
    ...(top ? [line(0, -r, 0, -from)] : []),
  ];
  switch (shape) {
    case 'cross':
      return [line(-r, 0, r, 0), line(0, -r, 0, r)];
    case 'cross_gap':
      return arms(gap);
    case 'dot':
      return [dot];
    case 'circle':
      return [ring];
    case 'circle_dot':
      return [ring, dot];
    case 'circle_cross':
      return [ring, ...arms(gap)];
    case 'x': {
      const d = r * Math.SQRT1_2;
      return [line(-d, -d, d, d), line(-d, d, d, -d)];
    }
    case 't':
      return arms(gap, false);
    case 'chevron':
      return [{ points: [[-r * 0.7, r * 0.5], [0, 0], [r * 0.7, r * 0.5]] }];
    case 'square': {
      const s = r * 0.6;
      return [{ points: [[-s, -s], [s, -s], [s, s], [-s, s], [-s, -s]] }];
    }
    case 'brackets': {
      const s = r * 0.7;
      const l = r * 0.35;
      return [
        { points: [[-s, -s + l], [-s, -s], [-s + l, -s]] },
        { points: [[s - l, -s], [s, -s], [s, -s + l]] },
        { points: [[s, s - l], [s, s], [s - l, s]] },
        { points: [[-s + l, s], [-s, s], [-s, s - l]] },
      ];
    }
  }
}

/** The profile with crosshair `i` changed (fields set to `undefined` go back to the default). */
export function updateCrosshair(p: Profile, i: number, patch: Partial<Crosshair>): Profile {
  const all = [...(p.crosshairs ?? [])];
  const next: Crosshair = { ...all[i], ...patch };
  for (const k of Object.keys(next) as (keyof Crosshair)[]) if (next[k] === undefined) delete next[k];
  all[i] = next;
  return { ...p, crosshairs: all };
}

export function addCrosshair(p: Profile): Profile {
  const all = p.crosshairs ?? [];
  if (all.length >= MAX_CROSSHAIRS) return p;
  return { ...p, crosshairs: [...all, { name: t('crosshairs.default_name', { n: all.length + 1 }) }] };
}

/** Remove crosshair `i`; commands showing a later one keep pointing at the same crosshair. */
export function removeCrosshair(p: Profile, i: number): Profile {
  const crosshairs = (p.crosshairs ?? []).filter((_, j) => j !== i);
  const mappings = p.mappings.map((m) => {
    if (!m.actions?.some((a) => 'command' in a && typeof a.command === 'object' && 'crosshair' in a.command)) return m;
    const actions = m.actions.flatMap((a) => {
      if (!('command' in a) || typeof a.command !== 'object' || !('crosshair' in a.command)) return [a];
      const n = a.command.crosshair;
      if (n === i + 1) return [];
      return [n > i + 1 ? { command: { crosshair: n - 1 } } : a];
    });
    return { ...m, actions };
  });
  const next: Profile = { ...p, crosshairs, mappings };
  if (crosshairs.length === 0) delete next.crosshairs;
  return next;
}
