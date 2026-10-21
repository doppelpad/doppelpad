// Pure helpers for the analog settings: stick and trigger shaping (per config, and the
// fields a layer changes) and analog routes (per layer). The shaping functions mirror
// dp-core's engine/analog.rs and the route rules engine/routes.rs and engine/compile.rs,
// so the editors show what the engine does.

import type {
  AnalogMapping,
  AnalogSource,
  AnalogTarget,
  GyroSettings,
  Layer,
  Profile,
  StickConfig,
  Sticks,
  TouchpadSettings,
  TriggerConfig,
  Triggers,
  VirtualDevices,
  VirtualStick,
  VirtualSticks,
} from '../types';

export type Side = 'left' | 'right';
/** A response curve point: `[input, output]`, both in 0..1. */
export type CurvePoint = [number, number];

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const round2 = (v: number) => Math.round(v * 100) / 100;
/** Deep copy that also works on Svelte state proxies (unlike `structuredClone`). */
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));

// ----- stick and trigger settings ----------------------------------------------------------

/**
 * Stick, trigger and virtual stick settings are per config; a layer may change some of their
 * fields (`virtual_sticks` is the layer's `virtual.sticks`).
 */
export type AnalogKind = 'sticks' | 'triggers' | 'virtual_sticks';

type Group = Partial<Record<Side, Record<string, unknown>>>;

function partOf(l: Layer | undefined, kind: AnalogKind): Group | undefined {
  return (kind === 'virtual_sticks' ? l?.virtual?.sticks : l?.[kind]) as Group | undefined;
}

/** The layer with a new group for `kind`; empty groups are dropped. */
function withPart(l: Layer, kind: AnalogKind, group: Group): Layer {
  const next: Layer = { ...l };
  const empty = Object.keys(group).length === 0;
  if (kind === 'virtual_sticks') {
    const v: { sticks?: VirtualSticks } = { ...next.virtual, sticks: group as VirtualSticks };
    if (empty) delete v.sticks;
    if (Object.keys(v).length > 0) next.virtual = v;
    else delete next.virtual;
  } else if (empty) delete next[kind];
  else next[kind] = group as Sticks & Triggers;
  return next;
}

function declared(p: Profile, layer: number): Layer | undefined {
  return layer === 0 ? undefined : p.layers?.find((l) => l.id === layer);
}

/** A stick's settings with the defaults filled in, as they are on `layer` (Main by default). */
export function stickConfig(p: Profile, side: Side, def: Required<StickConfig>, layer = 0): Required<StickConfig> {
  return { ...def, ...p.sticks?.[side], ...declared(p, layer)?.sticks?.[side] };
}

/** A trigger's settings with the defaults filled in, as they are on `layer`. */
export function triggerConfig(p: Profile, side: Side, def: Required<TriggerConfig>, layer = 0): Required<TriggerConfig> {
  return { ...def, ...p.triggers?.[side], ...declared(p, layer)?.triggers?.[side] };
}

/** The fields a layer sets itself for one stick or trigger (none on Main). */
export function layerOverrides(p: Profile, layer: number, kind: AnalogKind, side: Side): string[] {
  return Object.keys(partOf(declared(p, layer), kind)?.[side] ?? {});
}

function isDefault(v: object, def: object): boolean {
  return Object.entries(def).every(([k, d]) => JSON.stringify((v as Record<string, unknown>)[k]) === JSON.stringify(d));
}

/**
 * Change one side of a left/right pair. Like Rust, the pair is written in full (both sides,
 * every field) unless both sides hold their defaults; then it is left out (`undefined`).
 */
function setPair<T extends object>(
  cur: { left?: Partial<T>; right?: Partial<T> } | undefined,
  side: Side,
  patch: Partial<T>,
  def: T,
): { left: T; right: T } | undefined {
  const next = { left: { ...def, ...cur?.left }, right: { ...def, ...cur?.right } };
  next[side] = { ...next[side], ...patch };
  return isDefault(next.left, def) && isDefault(next.right, def) ? undefined : next;
}

/**
 * Change a layer's own fields for one stick or trigger. A field that ends up equal to Main's
 * value (`main`) is dropped, so the layer follows Main for it again. Declares the layer if
 * needed.
 */
export function setLayerPart(
  p: Profile,
  layer: number,
  kind: AnalogKind,
  side: Side,
  patch: object,
  main: object,
): Profile {
  const layers = [...(p.layers ?? [])];
  const i = layers.findIndex((l) => l.id === layer);
  const l: Layer = i >= 0 ? { ...layers[i] } : { id: layer, inherit: true };
  const group: Group = { ...partOf(l, kind) };
  const own: Record<string, unknown> = { ...(group[side] ?? {}) };
  for (const [k, v] of Object.entries(patch)) {
    if (JSON.stringify(v) === JSON.stringify((main as Record<string, unknown>)[k])) delete own[k];
    else own[k] = clone(v);
  }
  if (Object.keys(own).length > 0) group[side] = own;
  else delete group[side];
  if (i >= 0) layers[i] = withPart(l, kind, group);
  else layers.push(withPart(l, kind, group));
  return { ...p, layers };
}

/** Let a layer follow Main again for one stick or trigger. */
export function resetLayerPart(p: Profile, layer: number, kind: AnalogKind, side: Side): Profile {
  if (layerOverrides(p, layer, kind, side).length === 0) return p;
  const layers = (p.layers ?? []).map((l) => {
    if (l.id !== layer) return l;
    const group: Group = { ...partOf(l, kind) };
    delete group[side];
    return withPart(l, kind, group);
  });
  return { ...p, layers };
}

/** Change a stick's settings on Main, or only on `layer`. */
export function setStick(p: Profile, side: Side, patch: Partial<StickConfig>, def: Required<StickConfig>, layer = 0): Profile {
  if (layer !== 0) return setLayerPart(p, layer, 'sticks', side, patch, stickConfig(p, side, def));
  const sticks: Sticks | undefined = setPair(p.sticks, side, patch, def);
  const out = { ...p, sticks };
  if (!sticks) delete out.sticks;
  return out;
}

/** Change a trigger's settings on Main, or only on `layer`. */
export function setTrigger(
  p: Profile,
  side: Side,
  patch: Partial<TriggerConfig>,
  def: Required<TriggerConfig>,
  layer = 0,
): Profile {
  if (layer !== 0) return setLayerPart(p, layer, 'triggers', side, patch, triggerConfig(p, side, def));
  const triggers: Triggers | undefined = setPair(p.triggers, side, patch, def);
  const out = { ...p, triggers };
  if (!triggers) delete out.triggers;
  return out;
}

/** How mappings push a virtual stick, with the defaults filled in, as on `layer`. */
export function virtualStickConfig(
  p: Profile,
  side: Side,
  def: Required<VirtualStick>,
  layer = 0,
): Required<VirtualStick> {
  return { ...def, ...p.virtual?.sticks?.[side], ...declared(p, layer)?.virtual?.sticks?.[side] };
}

/** Change how mappings push a virtual stick, on Main or only on `layer`. */
export function setVirtualStick(
  p: Profile,
  side: Side,
  patch: Partial<VirtualStick>,
  def: Required<VirtualStick>,
  layer = 0,
): Profile {
  if (layer !== 0) return setLayerPart(p, layer, 'virtual_sticks', side, patch, virtualStickConfig(p, side, def));
  const sticks: VirtualSticks | undefined = setPair(p.virtual?.sticks, side, patch, def);
  const v: VirtualDevices = { ...p.virtual, sticks };
  if (!sticks) delete v.sticks;
  const out: Profile = { ...p, virtual: v };
  if (Object.keys(v).length === 0) delete out.virtual;
  return out;
}

/** The touchpad settings with the defaults filled in. */
export function touchpadConfig(p: Profile, def: Required<TouchpadSettings>): Required<TouchpadSettings> {
  return { ...def, ...p.touchpad };
}

/** Change the touchpad settings; written in full unless all are defaults, like Rust. */
export function setTouchpad(p: Profile, patch: Partial<TouchpadSettings>, def: Required<TouchpadSettings>): Profile {
  const touchpad = { ...touchpadConfig(p, def), ...patch };
  const out: Profile = { ...p, touchpad };
  if (isDefault(touchpad, def)) delete out.touchpad;
  return out;
}

/** The gyro settings with the defaults filled in. */
export function gyroConfig(p: Profile, def: Required<GyroSettings>): Required<GyroSettings> {
  return { ...def, ...p.gyro };
}

/** Change the gyro settings; like Rust, they are written in full unless all are defaults. */
export function setGyro(p: Profile, patch: Partial<GyroSettings>, def: Required<GyroSettings>): Profile {
  const gyro = { ...gyroConfig(p, def), ...patch };
  const out: Profile = { ...p, gyro };
  if (isDefault(gyro, def)) delete out.gyro;
  return out;
}

/** Deflection a button pushing along one axis has reached after `heldS` seconds (engine rule). */
export function virtualRamp(cfg: Required<VirtualStick>, heldS: number, multiplier: number): number {
  const grown = (cfg.speed + 0.5 * cfg.acceleration * heldS) * heldS * multiplier;
  return clamp(Math.min(cfg.initial + grown, cfg.max), 0, 1);
}

// ----- shaping (engine/analog.rs) -----------------------------------------------------------

/** Rotate a stick vector by `deg` degrees (counter-clockwise, y up). */
export function rotate([x, y]: [number, number], deg: number): [number, number] {
  if (deg === 0) return [x, y];
  const r = (deg * Math.PI) / 180;
  const [s, c] = [Math.sin(r), Math.cos(r)];
  return [clamp(x * c - y * s, -1, 1), clamp(x * s + y * c, -1, 1)];
}

/** Physical stick after inversion and rotation (what direction controls see). */
export function orient(raw: [number, number], cfg: Required<StickConfig>): [number, number] {
  return rotate([cfg.invert_x ? -raw[0] : raw[0], cfg.invert_y ? -raw[1] : raw[1]], cfg.rotation);
}

/** Piecewise-linear response curve through (0,0), the points, and (1,1). */
export function applyCurve(points: CurvePoint[], t: number): number {
  if (points.length === 0) return t;
  let prev: CurvePoint = [0, 0];
  for (const raw of [...points, [1, 1] as CurvePoint]) {
    const p: CurvePoint = [clamp(raw[0], 0, 1), clamp(raw[1], 0, 1)];
    if (t <= p[0]) {
      const span = p[0] - prev[0];
      if (span <= Number.EPSILON) return p[1];
      return prev[1] + ((p[1] - prev[1]) * (t - prev[0])) / span;
    }
    prev = p;
  }
  return 1;
}

const outerOf = (cfg: Required<StickConfig>) => Math.max(cfg.outer, cfg.deadzone + 0.01);
const lift = (v: number, anti: number) => (v > 0 && anti > 0 ? anti + (1 - anti) * v : v);

/** Output deflection for a stick deflection: deadzone, outer, curve, anti-deadzone, sensitivity. */
export function stickResponse(mag: number, cfg: Required<StickConfig>): number {
  if (mag <= cfg.deadzone || mag === 0) return 0;
  const t = clamp((mag - cfg.deadzone) / (outerOf(cfg) - cfg.deadzone), 0, 1);
  return Math.min(lift(applyCurve(cfg.curve, t), cfg.anti_deadzone) * cfg.sensitivity, 1);
}

/** Shape an oriented stick vector for output, keeping its direction. */
export function shapeStick([x, y]: [number, number], cfg: Required<StickConfig>): [number, number] {
  const mag = Math.hypot(x, y);
  const out = stickResponse(mag, cfg);
  return out === 0 ? [0, 0] : [(x / mag) * out, (y / mag) * out];
}

/** Trigger output after deadzone and full-pull rescaling. */
export function shapeTrigger(v: number, cfg: Required<TriggerConfig>): number {
  if (v <= cfg.deadzone) return 0;
  const top = Math.max(cfg.full_pull, cfg.deadzone + 0.01);
  return clamp((v - cfg.deadzone) / (top - cfg.deadzone), 0, 1);
}

// ----- response curve editing -----------------------------------------------------------------

export const MAX_CURVE_POINTS = 8;

export const CURVE_PRESETS: { id: string; points: CurvePoint[] }[] = [
  { id: 'linear', points: [] },
  { id: 'precise', points: [[0.3, 0.15], [0.7, 0.55]] },
  { id: 'very_precise', points: [[0.25, 0.06], [0.5, 0.25], [0.75, 0.56]] },
  { id: 'fast', points: [[0.3, 0.45], [0.7, 0.85]] },
];

/** The preset these points are, if any. */
export function curvePreset(points: CurvePoint[]): string | null {
  return CURVE_PRESETS.find((p) => JSON.stringify(p.points) === JSON.stringify(points))?.id ?? null;
}

/**
 * Where a curve point lies on the response graph (stick deflection → output). The curve
 * works between deadzone and outer, before anti-deadzone and sensitivity.
 */
export function curveToGraph([t, v]: CurvePoint, cfg: Required<StickConfig>): CurvePoint {
  const x = cfg.deadzone + t * (outerOf(cfg) - cfg.deadzone);
  return [x, Math.min(lift(v, cfg.anti_deadzone) * cfg.sensitivity, 1)];
}

/** The curve point at a position of the response graph (inverse of `curveToGraph`), rounded. */
export function graphToCurve([x, y]: CurvePoint, cfg: Required<StickConfig>): CurvePoint {
  const t = (x - cfg.deadzone) / (outerOf(cfg) - cfg.deadzone);
  let v = y / Math.max(cfg.sensitivity, 0.01);
  if (cfg.anti_deadzone > 0) v = (v - cfg.anti_deadzone) / Math.max(1 - cfg.anti_deadzone, 0.01);
  return [round2(clamp(t, 0, 1)), round2(clamp(v, 0, 1))];
}

/** Insert a point, keeping the list sorted by input; a point at the same input is replaced. */
export function addCurvePoint(points: CurvePoint[], pt: CurvePoint): CurvePoint[] {
  const p: CurvePoint = [round2(clamp(pt[0], 0.01, 0.99)), round2(clamp(pt[1], 0, 1))];
  const rest = points.filter((q) => Math.abs(q[0] - p[0]) >= 0.01);
  if (rest.length >= MAX_CURVE_POINTS) return points;
  return [...rest, p].sort((a, b) => a[0] - b[0]);
}

/** Move point `i`, keeping it between its neighbours so the list stays sorted. */
export function moveCurvePoint(points: CurvePoint[], i: number, pt: CurvePoint): CurvePoint[] {
  const lo = i > 0 ? points[i - 1][0] + 0.01 : 0.01;
  const hi = i < points.length - 1 ? points[i + 1][0] - 0.01 : 0.99;
  const next = [...points];
  next[i] = [round2(clamp(pt[0], lo, hi)), round2(clamp(pt[1], 0, 1))];
  return next;
}

export function removeCurvePoint(points: CurvePoint[], i: number): CurvePoint[] {
  return points.filter((_, j) => j !== i);
}

// ----- analog routes (engine/routes.rs) -------------------------------------------------------

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
  if (layer === 0 || p.layers?.find((l) => l.id === layer)?.inherit === false) return own;
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
  const out: Profile = { ...p, analog };
  if (analog.length === 0) delete out.analog;
  return out;
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
