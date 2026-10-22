// Analog routes (per layer): which source drives which target and with what settings,
// mirroring the engine's routes.rs and compile.rs, so the editors show what the engine does.

import type { AnalogMapping, AnalogSource, AnalogTarget, Profile } from '../types';
import { declaredLayer } from './layers';
import { clone, nonEmpty, setOrDrop } from './util';

export const ROUTE_SOURCES: AnalogSource[] = ['right_stick', 'left_stick', 'gyro', 'mouse', 'touchpad'];
export const ROUTE_TARGETS: AnalogTarget[] = ['mouse', 'right_stick', 'left_stick', 'flick'];

/** Route settings that only some routes use; sensitivity, vertical ratio, invert and while-held apply to all. */
export type RouteField =
  | 'curve'
  | 'deadzone'
  | 'outer'
  | 'anti_deadzone'
  | 'smoothing_ms'
  | 'decay_ms'
  | 'trackball_friction'
  | 'snap'
  | 'noise_filter'
  | 'acceleration'
  | 'spring'
  | 'consume';

/** What the engine reads per route; `consume` only acts on stick and mouse sources. */
const ROUTE_FIELDS: Record<string, RouteField[]> = {
  'stick>mouse': ['curve', 'deadzone', 'outer', 'smoothing_ms', 'acceleration', 'consume'],
  'stick>stick': ['deadzone', 'outer', 'anti_deadzone', 'snap', 'consume'],
  'mouse>stick': ['curve', 'anti_deadzone', 'snap', 'smoothing_ms', 'decay_ms', 'noise_filter', 'spring', 'consume'],
  'gyro>mouse': ['smoothing_ms'],
  'gyro>stick': ['anti_deadzone', 'snap', 'smoothing_ms'],
  'touchpad>mouse': ['trackball_friction'],
  'touchpad>stick': ['curve', 'deadzone', 'anti_deadzone', 'snap'],
  'stick>flick': ['deadzone', 'smoothing_ms', 'consume'],
};

/** A route with the defaults filled in. */
export function routeConfig(route: AnalogMapping, def: Required<AnalogMapping>): Required<AnalogMapping> {
  return { ...def, ...route };
}

export const sourceOf = (r: AnalogMapping): AnalogSource => r.source ?? 'right_stick';
export const targetOf = (r: AnalogMapping): AnalogTarget => r.target ?? 'mouse';
export const routeLayer = (r: AnalogMapping) => r.layer ?? 0;

export function routeFields(source: AnalogSource, target: AnalogTarget): RouteField[] {
  const s = source === 'left_stick' || source === 'right_stick' ? 'stick' : source;
  const t = target === 'mouse' || target === 'flick' ? target : 'stick';
  return ROUTE_FIELDS[`${s}>${t}`] ?? [];
}

/** Targets a source can drive (the mouse cannot drive the mouse; flick stick needs a stick). */
export function routeTargets(source: AnalogSource): AnalogTarget[] {
  const stick = source === 'left_stick' || source === 'right_stick';
  return ROUTE_TARGETS.filter((t) => !(source === 'mouse' && t === 'mouse') && (t !== 'flick' || stick));
}

/** The change for a new source; a target the source cannot drive becomes the right stick. */
export function sourcePatch(r: AnalogMapping, source: AnalogSource): Partial<AnalogMapping> {
  return routeTargets(source).includes(targetOf(r)) ? { source } : { source, target: 'right_stick' };
}

export interface RouteRef {
  /** Position in `profile.analog`. */
  index: number;
  route: AnalogMapping;
  /** A route of Main that applies on this layer. */
  inherited: boolean;
}

/**
 * Routes active on a layer: its own, plus Main's for sources the layer does not route
 * itself, unless the layer does not inherit (engine/compile.rs).
 */
export function layerRoutes(p: Profile, layer: number): RouteRef[] {
  const all = (p.analog ?? []).map((route, index) => ({ index, route, inherited: false }));
  const own = all.filter((r) => routeLayer(r.route) === layer);
  if (layer === 0 || declaredLayer(p, layer)?.inherit === false) return own;
  const routed = new Set(own.map((r) => sourceOf(r.route)));
  const base = all.filter((r) => routeLayer(r.route) === 0 && !routed.has(sourceOf(r.route)));
  return [...own, ...base.map((r) => ({ ...r, inherited: true }))];
}

/** Common routes offered when adding one. */
export const ROUTE_PRESETS: { id: string; patch: Partial<AnalogMapping> }[] = [
  { id: 'stick_mouse', patch: { source: 'right_stick', target: 'mouse' } },
  { id: 'gyro_mouse', patch: { source: 'gyro', target: 'mouse' } },
  { id: 'gyro_aim', patch: { source: 'gyro', target: 'right_stick', while_held: ['LT'] } },
  { id: 'mouse_stick', patch: { source: 'mouse', target: 'right_stick' } },
  { id: 'touchpad_mouse', patch: { source: 'touchpad', target: 'mouse' } },
  { id: 'flick_stick', patch: { source: 'right_stick', target: 'flick', deadzone: 0.6 } },
];

/** Stick → mouse response presets: the fields they set; sensitivity and the rest stay. */
export type MousePreset = 'standard' | 'precise' | 'fast' | 'smooth';
type PresetFields = Pick<Required<AnalogMapping>, 'curve' | 'deadzone' | 'outer' | 'smoothing_ms' | 'acceleration'>;
export const MOUSE_PRESETS: Record<MousePreset, PresetFields> = {
  standard: { curve: 1.6, deadzone: 0.1, outer: 0.95, smoothing_ms: 8, acceleration: 1 },
  precise: { curve: 2.2, deadzone: 0.1, outer: 0.95, smoothing_ms: 12, acceleration: 0.5 },
  fast: { curve: 1, deadzone: 0.08, outer: 0.9, smoothing_ms: 4, acceleration: 2 },
  smooth: { curve: 1.6, deadzone: 0.12, outer: 0.95, smoothing_ms: 30, acceleration: 1 },
};

/** The preset a route's values match, or null for custom values. */
export function mousePresetOf(r: Required<AnalogMapping>): MousePreset | null {
  const ids = Object.keys(MOUSE_PRESETS) as MousePreset[];
  return (
    ids.find((id) =>
      (Object.entries(MOUSE_PRESETS[id]) as [keyof PresetFields, number][]).every(([f, v]) => Math.abs(r[f] - v) < 1e-3),
    ) ?? null
  );
}

/** Stick → mouse: cursor speed as a fraction of full speed at a stick deflection (before acceleration). */
export function mouseResponse(mag: number, r: Pick<Required<AnalogMapping>, 'deadzone' | 'outer' | 'curve'>): number {
  if (mag <= r.deadzone) return 0;
  const t = Math.min(Math.max((mag - r.deadzone) / Math.max(r.outer - r.deadzone, 0.01), 0), 1);
  return r.curve > 0 && r.curve !== 1 ? t ** r.curve : t;
}

/** A new route on a layer, with every field written like Rust does. */
export function newRoute(def: Required<AnalogMapping>, layer: number, patch: Partial<AnalogMapping> = {}): AnalogMapping {
  return { ...clone(def), layer, ...patch };
}

/** The profile with these routes; an empty list is left out like Rust does. */
function withRoutes(p: Profile, analog: AnalogMapping[]): Profile {
  return setOrDrop(p, 'analog', nonEmpty(analog));
}

export function addRoute(p: Profile, r: AnalogMapping): Profile {
  return withRoutes(p, [...(p.analog ?? []), r]);
}

export function updateRoute(p: Profile, index: number, patch: Partial<AnalogMapping>): Profile {
  return withRoutes(p, (p.analog ?? []).map((r, i) => (i === index ? { ...r, ...patch } : r)));
}

export function removeRoute(p: Profile, index: number): Profile {
  return withRoutes(p, (p.analog ?? []).filter((_, i) => i !== index));
}

/** Copy a route (usually one inherited from Main) onto a layer, where it replaces Main's. */
export function overrideRoute(p: Profile, index: number, layer: number): Profile {
  const r = p.analog?.[index];
  return r ? addRoute(p, { ...clone(r), layer }) : p;
}

/** Routes of other layers, and these routes moved to `layer` (for removing or pasting layers). */
export function replaceLayerRoutes(p: Profile, layer: number, routes: AnalogMapping[]): Profile {
  const rest = (p.analog ?? []).filter((r) => routeLayer(r) !== layer);
  return withRoutes(p, [...rest, ...routes.map((r) => ({ ...clone(r), layer }))]);
}
