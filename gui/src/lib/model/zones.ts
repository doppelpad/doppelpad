// Zone thresholds of sticks and triggers: the effective values the engine uses, the zone a
// live value is in, and edits to them. Mirrors dp-core's zone rule.

import type { Catalog, Profile, StickZones, TriggerZones, ZoneRadius } from '../types';
import { setLayerPart, type Side } from './analog';
import { ZONES, type PairSource, type ZoneName } from './controls';
import { declaredLayer } from './layers';
import { nonEmpty, setOrDrop } from './util';

export type Level = 'low' | 'med' | 'high';
export const LEVELS: Level[] = ['low', 'med', 'high'];
/** `[low, med, high]` along one axis. */
export type Triple = [number, number, number];

/** Defaults the engine falls back to (from the catalog). */
export interface ZoneDefaults {
  stickLow: ZoneRadius;
  triggerLow: number;
  fullPull: number;
}

/** The defaults from the catalog; the engine's own values until the catalog has arrived. */
export function zoneDefaults(d: Catalog['defaults'] | undefined): ZoneDefaults {
  return {
    stickLow: d?.stick.zones.low ?? 0.24,
    triggerLow: d?.trigger.zones.low ?? 0.03,
    fullPull: d?.trigger.full_pull ?? 0.99,
  };
}

/** One axis of a threshold (`0` = x, `1` = y). */
export function axisOf(r: ZoneRadius | undefined, axis: 0 | 1): number | undefined {
  if (r === undefined) return undefined;
  return Array.isArray(r) ? r[axis] : r;
}

/** Effective thresholds along one axis: unset med/high split the range above low into thirds. */
export function thresholds(low: number, med: number | undefined, high: number | undefined, max: number): Triple {
  return [low, med ?? low + (max - low) / 3, high ?? low + ((max - low) * 2) / 3];
}

/** Effective stick thresholds per axis. */
export function stickThresholds(z: StickZones | undefined, d: ZoneDefaults): { x: Triple; y: Triple } {
  const along = (axis: 0 | 1): Triple => thresholds(axisOf(z?.low ?? d.stickLow, axis)!, axisOf(z?.med, axis), axisOf(z?.high, axis), 1);
  return { x: along(0), y: along(1) };
}

/** Effective trigger thresholds. */
export function triggerThresholds(z: TriggerZones | undefined, fullPull: number | undefined, d: ZoneDefaults): Triple {
  return thresholds(z?.low ?? d.triggerLow, z?.med, z?.high, fullPull ?? d.fullPull);
}

/** Whether any stick threshold differs between the axes. */
export function isElliptic(z: StickZones | undefined): boolean {
  return LEVELS.some((l) => {
    const r = z?.[l];
    return Array.isArray(r) && r[0] !== r[1];
  });
}

/** Thresholds count as at least this, so a zero one does not take everything (dp-core
 * engine/analog.rs `MIN_THRESHOLD`). */
const MIN_THRESHOLD = 0.001;

/** The zone a trigger pull is in (a value on a threshold is in the higher zone), or null. */
export function triggerZoneAt(v: number, t: Triple): ZoneName | null {
  for (let i = 2; i >= 0; i--) if (v >= Math.max(t[i], MIN_THRESHOLD)) return ZONES[i];
  return null;
}

/** The zone a stick vector is in: each threshold is an ellipse with radii (x, y). */
export function stickZoneAt(x: number, y: number, t: { x: Triple; y: Triple }): ZoneName | null {
  for (let i = 2; i >= 0; i--) {
    const mx = x / Math.max(t.x[i], MIN_THRESHOLD);
    const my = y / Math.max(t.y[i], MIN_THRESHOLD);
    if (mx * mx + my * my >= 1) return ZONES[i];
  }
  return null;
}

/** A threshold as stored: a single number when both axes agree. */
function radius(x: number, y: number): ZoneRadius {
  return x === y ? x : [x, y];
}

/**
 * Set one level of a stick's zones. `axis` 'both' sets a round threshold; 0 / 1 sets one axis
 * and keeps the other. `undefined` clears the level (low falls back to the default, med/high to
 * even spacing).
 */
export function setStickLevel(
  z: StickZones | undefined,
  level: Level,
  axis: 'both' | 0 | 1,
  value: number | undefined,
  d: ZoneDefaults,
): StickZones {
  if (value === undefined || axis === 'both') return setOrDrop({ ...z }, level, value);
  // Setting one axis starts from the effective value of the other.
  const eff = stickThresholds(z, d);
  const i = LEVELS.indexOf(level);
  const x = axis === 0 ? value : eff.x[i];
  const y = axis === 1 ? value : eff.y[i];
  return { ...z, [level]: radius(x, y) };
}

/** Turn separate axes on (each level keeps its value on both) or off (the x value wins). */
export function setElliptic(z: StickZones | undefined, on: boolean): StickZones {
  const out: StickZones = { ...(z ?? {}) };
  if (on) return out;
  for (const l of LEVELS) {
    const r = out[l];
    if (Array.isArray(r)) out[l] = r[0];
  }
  return out;
}

export function setTriggerLevel(z: TriggerZones | undefined, level: Level, value: number | undefined): TriggerZones {
  return setOrDrop({ ...z }, level, value);
}

/** The zones stored for one stick or trigger as they are on `layer`: its own, else Main's. */
export function storedZones(p: Profile, source: PairSource, layer = 0): StickZones | TriggerZones | undefined {
  const key = source.kind === 'stick' ? 'sticks' : 'triggers';
  const own = layer === 0 ? undefined : declaredLayer(p, layer)?.[key]?.[source.side]?.zones;
  return own ?? p[key]?.[source.side]?.zones;
}

/**
 * The profile with new zones for one stick or trigger (empty zones are dropped). On a layer
 * other than Main only that layer changes; zones equal to Main's are dropped from it.
 */
export function withZones(p: Profile, source: PairSource, zones: StickZones | TriggerZones, layer = 0): Profile {
  const key = source.kind === 'stick' ? 'sticks' : 'triggers';
  if (layer !== 0) {
    const main = storedZones(p, source) ?? {};
    return setLayerPart(p, layer, key, source.side, { zones }, { zones: main });
  }
  const group = { ...(p[key] ?? {}) } as Record<Side, Record<string, unknown> | undefined>;
  const cfg: Record<string, unknown> = { ...group[source.side] };
  group[source.side] = setOrDrop(cfg, 'zones', nonEmpty(zones));
  return { ...p, [key]: group };
}

/** Problem with effective thresholds, as an i18n key, or null when they are usable. */
export function zoneProblem([low, med, high]: Triple): string | null {
  if (!(low > 0 && high <= 1)) return 'zones.err_range';
  if (!(low < med && med < high)) return 'zones.err_order';
  return null;
}
