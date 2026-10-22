// Pure helpers for the analog settings: stick and trigger shaping (per config, and the
// fields a layer changes) and analog routes (per layer). The shaping functions mirror
// dp-core's engine/analog.rs and the route rules engine/routes.rs and engine/compile.rs,
// so the editors show what the engine does.

import type {
  GyroSettings,
  Layer,
  Profile,
  StickConfig,
  StickName,
  Sticks,
  TouchpadSettings,
  TriggerConfig,
  TriggerFull,
  Triggers,
  VirtualStick,
  VirtualSticks,
} from '../types';
import { declaredLayer, patchLayer, setVirtualDevice } from './layers';
import { clamp, clone, nonEmpty, roundTo, sameJson, setOrDrop, setRising } from './util';

export type Side = StickName;
/** A response curve point: `[input, output]`, both in 0..1. */
export type CurvePoint = [number, number];

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
  const part = nonEmpty(group);
  if (kind !== 'virtual_sticks') return setOrDrop(l, kind, part as Sticks & Triggers | undefined);
  return setOrDrop(l, 'virtual', nonEmpty(setOrDrop({ ...l.virtual }, 'sticks', part as VirtualSticks | undefined)));
}

function declared(p: Profile, layer: number): Layer | undefined {
  return layer === 0 ? undefined : declaredLayer(p, layer);
}

/** A stick's settings with the defaults filled in, as they are on `layer` (Main by default). */
export function stickConfig(p: Profile, side: Side, def: Required<StickConfig>, layer = 0): Required<StickConfig> {
  return { ...def, ...p.sticks?.[side], ...declared(p, layer)?.sticks?.[side] };
}

/** A trigger's settings with the defaults filled in, as they are on `layer`. */
export function triggerConfig(p: Profile, side: Side, def: TriggerFull, layer = 0): TriggerFull {
  return { ...def, ...p.triggers?.[side], ...declared(p, layer)?.triggers?.[side] };
}

/** The fields a layer sets itself for one stick or trigger (none on Main). */
export function layerOverrides(p: Profile, layer: number, kind: AnalogKind, side: Side): string[] {
  return Object.keys(partOf(declared(p, layer), kind)?.[side] ?? {});
}

function isDefault(v: object, def: object): boolean {
  return Object.entries(def).every(([k, d]) => sameJson((v as Record<string, unknown>)[k], d));
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
  return patchLayer(p, layer, (l) => {
    const group: Group = { ...partOf(l, kind) };
    const own: Record<string, unknown> = { ...(group[side] ?? {}) };
    for (const [k, v] of Object.entries(patch)) {
      if (sameJson(v, (main as Record<string, unknown>)[k])) delete own[k];
      else own[k] = clone(v);
    }
    return withPart(l, kind, setOrDrop(group, side, nonEmpty(own)));
  });
}

/** Let a layer follow Main again for one stick or trigger. */
export function resetLayerPart(p: Profile, layer: number, kind: AnalogKind, side: Side): Profile {
  if (layerOverrides(p, layer, kind, side).length === 0) return p;
  return patchLayer(p, layer, (l) => withPart(l, kind, setOrDrop({ ...partOf(l, kind) }, side, undefined)));
}

/** Change a stick's settings on Main, or only on `layer`. */
export function setStick(p: Profile, side: Side, patch: Partial<StickConfig>, def: Required<StickConfig>, layer = 0): Profile {
  if (layer !== 0) return setLayerPart(p, layer, 'sticks', side, patch, stickConfig(p, side, def));
  return setOrDrop(p, 'sticks', setPair(p.sticks, side, patch, def));
}

/** Change a trigger's settings on Main, or only on `layer`. */
export function setTrigger(
  p: Profile,
  side: Side,
  patch: Partial<TriggerConfig>,
  def: TriggerFull,
  layer = 0,
): Profile {
  if (layer !== 0) return setLayerPart(p, layer, 'triggers', side, patch, triggerConfig(p, side, def));
  return setOrDrop(p, 'triggers', setPair(p.triggers, side, patch, def));
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
  return setVirtualDevice(p, 'sticks', setPair(p.virtual?.sticks, side, patch, def));
}

/** The touchpad settings with the defaults filled in. */
export function touchpadConfig(p: Profile, def: Required<TouchpadSettings>): Required<TouchpadSettings> {
  return { ...def, ...p.touchpad };
}

/** Change the touchpad settings; written in full unless all are defaults, like Rust. */
export function setTouchpad(p: Profile, patch: Partial<TouchpadSettings>, def: Required<TouchpadSettings>): Profile {
  const touchpad = { ...touchpadConfig(p, def), ...patch };
  return setOrDrop(p, 'touchpad', isDefault(touchpad, def) ? undefined : touchpad);
}

/** The gyro settings with the defaults filled in. */
export function gyroConfig(p: Profile, def: Required<GyroSettings>): Required<GyroSettings> {
  return { ...def, ...p.gyro };
}

/**
 * Set zone threshold `i` (°/s, or ° when tilting) of the gyro's current mode; the others move
 * so they keep rising, and none goes below 0.5.
 */
export function setGyroZone(p: Profile, i: number, v: number, def: Required<GyroSettings>): Profile {
  const cfg = gyroConfig(p, def);
  const key = cfg.mode === 'tilt' ? 'tilt_zones' : 'motion_zones';
  const zones = setRising(cfg[key], i, v).map((x) => Math.max(x, 0.5)) as [number, number, number];
  return setGyro(p, { [key]: zones }, def);
}

/** Change the gyro settings; like Rust, they are written in full unless all are defaults. */
export function setGyro(p: Profile, patch: Partial<GyroSettings>, def: Required<GyroSettings>): Profile {
  const gyro = { ...gyroConfig(p, def), ...patch };
  return setOrDrop(p, 'gyro', isDefault(gyro, def) ? undefined : gyro);
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
export function shapeTrigger(v: number, cfg: TriggerFull): number {
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
  return [roundTo(clamp(t, 0, 1), 2), roundTo(clamp(v, 0, 1), 2)];
}

/** Insert a point, keeping the list sorted by input; a point at the same input is replaced. */
export function addCurvePoint(points: CurvePoint[], pt: CurvePoint): CurvePoint[] {
  const p: CurvePoint = [roundTo(clamp(pt[0], 0.01, 0.99), 2), roundTo(clamp(pt[1], 0, 1), 2)];
  const rest = points.filter((q) => Math.abs(q[0] - p[0]) >= 0.01);
  if (rest.length >= MAX_CURVE_POINTS) return points;
  return [...rest, p].sort((a, b) => a[0] - b[0]);
}

/** Move point `i`, keeping it between its neighbours so the list stays sorted. */
export function moveCurvePoint(points: CurvePoint[], i: number, pt: CurvePoint): CurvePoint[] {
  const lo = i > 0 ? points[i - 1][0] + 0.01 : 0.01;
  const hi = i < points.length - 1 ? points[i + 1][0] - 0.01 : 0.99;
  const next = [...points];
  next[i] = [roundTo(clamp(pt[0], lo, hi), 2), roundTo(clamp(pt[1], 0, 1), 2)];
  return next;
}

export function removeCurvePoint(points: CurvePoint[], i: number): CurvePoint[] {
  return points.filter((_, j) => j !== i);
}
