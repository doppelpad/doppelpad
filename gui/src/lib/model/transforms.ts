// One-click layout transforms: swap A/B and X/Y, swap the sticks (southpaw), swap bumpers and
// triggers, or mirror left and right. A transform moves what each control does to its partner,
// on one layer or in the whole config:
// - the mappings of each pair are exchanged; a button without a mapping takes its own output
//   along (A that passes A through becomes a mapping B → A), so the swap also works for controls
//   nothing maps;
// - on a shift layer, a control that now does nothing there no longer takes Main's mapping over;
// - for sticks, their direction mappings and the routes they are the source of are exchanged,
//   and a stick that only passed through gets a route to the virtual stick it fed (consuming the
//   physical one), so the physical sticks really trade places.

import { DIRS8, SIDE_NAMES, SIDES, TRACKPAD_PARTS, ZONES, parseControl, zonedParts } from './controls';
import { declaredLayer, patchLayer } from './layers';
import { comboList, effectiveMappings, layerIds, layerOf, nativeAction, sameCombo, tidyMapping } from './profile';
import { layerRoutes, routeLayer, sourceOf } from './routes';
import { clone, nonEmpty, sameJson, setOrDrop } from './util';
import type { AnalogMapping, ControlName, Mapping, Profile, StickConfig } from '../types';
import { stickConfig, type Side } from './analog';

export type TransformKind = 'face' | 'sticks' | 'bumpers_triggers' | 'mirror';
export const TRANSFORMS: TransformKind[] = ['face', 'sticks', 'bumpers_triggers', 'mirror'];

type Pair = [ControlName, ControlName];

const STICK_PAIRS: Pair[] = [
  [SIDES.left.click, SIDES.right.click],
  ...zonedParts(DIRS8).map((p): Pair => [`${SIDES.left.stick}${p}`, `${SIDES.right.stick}${p}`]),
];
const TRIGGER_PAIRS: Pair[] = [['LT', 'RT'], ['LTFull', 'RTFull'], ...ZONES.map((z): Pair => [`LT${z}`, `RT${z}`])];

/**
 * The control pairs of each transform, and whether the sticks trade places. Directions are
 * names, not positions: a mirror keeps up up and left left; it swaps the sides of the pad.
 */
function pairsOf(kind: TransformKind): { pairs: Pair[]; sticks: boolean } {
  switch (kind) {
    case 'face':
      return {
        pairs: [
          ['A', 'B'],
          ['X', 'Y'],
        ],
        sticks: false,
      };
    case 'bumpers_triggers':
      return {
        pairs: [
          ['LB', 'LT'],
          ['RB', 'RT'],
        ],
        sticks: false,
      };
    case 'sticks':
      return { pairs: STICK_PAIRS, sticks: true };
    case 'mirror':
      return {
        pairs: [
          ['LB', 'RB'],
          ...TRIGGER_PAIRS,
          ['Back', 'Start'],
          ['PaddleUL', 'PaddleUR'],
          ['PaddleLL', 'PaddleLR'],
          ['DpadUp', 'Y'],
          ['DpadDown', 'A'],
          ['DpadLeft', 'X'],
          ['DpadRight', 'B'],
          ...TRACKPAD_PARTS.map((p): Pair => [`TrackpadLeft${p}`, `TrackpadRight${p}`]),
          ...STICK_PAIRS,
        ],
        sticks: true,
      };
  }
}

/** Whether a control passes through by itself when nothing maps it (buttons and trigger pulls). */
function passesThrough(p: Profile, c: ControlName): boolean {
  if (p.output?.passthrough === false) return false;
  const kind = parseControl(c)?.kind;
  return kind === 'button' || kind === 'trigger_press';
}

/** What a control does on a layer: its own mappings, Main's it takes over, or (`materialize`) its own output. */
function behaviour(p: Profile, layer: number, c: ControlName, materialize: boolean): Mapping[] {
  const eff = effectiveMappings(p, layer, c);
  if (eff.mappings.length > 0) return eff.mappings.map((m) => clone(m));
  if (!materialize || !passesThrough(p, c)) return [];
  const native = nativeAction(c);
  return native ? [{ trigger: c, actions: [native] }] : [];
}

/** Mappings that only reproduce what the control does by itself (nothing to write down). */
function isOwnOutput(ms: Mapping[], c: ControlName): boolean {
  const native = nativeAction(c);
  if (!native || ms.length !== 1) return false;
  const m = tidyMapping({ ...ms[0]!, layer: 0, trigger: c });
  return Object.keys(m).sort().join(',') === 'actions,trigger' && sameJson(m.actions, [native]);
}

const singleOf = (m: Mapping) => {
  const list = comboList(m.trigger);
  return list.length === 1 ? list[0]! : null;
};

/** Exchange what the controls of each pair do on one layer: read from `p`, written into `into`. */
function swapMappings(p: Profile, into: Profile, layer: number, pairs: Pair[], materialize: boolean): Profile {
  const touched = new Set(pairs.flat());
  const next: Mapping[] = [];
  /** Controls that inherited Main's mapping and do nothing on the layer now. */
  const silenced: ControlName[] = [];
  for (const [x, y] of pairs) {
    const moves: [Mapping[], ControlName][] = [
      [behaviour(p, layer, x, materialize), y],
      [behaviour(p, layer, y, materialize), x],
    ];
    for (const [ms, to] of moves) {
      if (layer !== 0 && ms.length === 0 && effectiveMappings(p, layer, to).inherited) silenced.push(to);
      if (isOwnOutput(ms, to)) continue;
      for (const m of ms) next.push(tidyMapping({ ...m, layer, trigger: to }));
    }
  }
  const rest = into.mappings.filter((m) => !(layerOf(m) === layer && touched.has(singleOf(m) ?? '')));
  let out: Profile = { ...into, mappings: [...rest, ...next] };
  if (silenced.length) {
    out = patchLayer(out, layer, (l) => ({
      ...l,
      not_inherited: [...(l.not_inherited ?? []).filter((c) => !silenced.some((s) => sameCombo(c, s))), ...silenced],
    }));
  }
  return out;
}

/**
 * A route from stick `source` to virtual stick `target` that behaves as passing the `target`
 * side through did on `layer`: the deadzone, outer, anti-deadzone and sensitivity it has there,
 * linear.
 */
function passthroughRoute(p: Profile, source: Side, target: Side, layer: number, def: Required<StickConfig>): AnalogMapping {
  const cfg = stickConfig(p, target, def, layer);
  return {
    source: SIDES[source].source,
    target: SIDES[target].source,
    sensitivity: cfg.sensitivity,
    curve: 1,
    deadzone: cfg.deadzone,
    outer: cfg.outer,
    anti_deadzone: cfg.anti_deadzone,
    consume: true,
  };
}

const stripLayer = ({ layer: _layer, ...r }: AnalogMapping) => r;
const otherSide = (s: Side): Side => (s === 'left' ? 'right' : 'left');
const isStickSource = (r: AnalogMapping) => sourceOf(r) === 'left_stick' || sourceOf(r) === 'right_stick';

/**
 * The routes of one layer with the sticks trading places: a route from one stick now comes from
 * the other, and a stick that only passed through gets a route to the virtual stick it fed,
 * shaped like the stick settings shaped it (the response curve is not carried over).
 */
function swapStickRoutes(p: Profile, into: Profile, layer: number, def: Required<StickConfig>): Profile {
  const effective = layerRoutes(p, layer).map((r) => r.route);
  const moved: AnalogMapping[] = [];
  for (const s of SIDE_NAMES) {
    const from = otherSide(s);
    const was = effective.filter((r) => sourceOf(r) === SIDES[from].source);
    if (was.length) {
      for (const r of was) {
        const route = { ...clone(r), source: SIDES[s].source };
        // A route that feeds a stick's own virtual stick as passing through would: drop it.
        if (sameJson(stripLayer(route), passthroughRoute(p, s, s, layer, def))) continue;
        moved.push(setOrDrop(route, 'layer', layer || undefined));
      }
      continue;
    }
    moved.push(setOrDrop(passthroughRoute(p, s, from, layer, def), 'layer', layer || undefined));
  }
  const rest = (into.analog ?? []).filter((r) => !(routeLayer(r) === layer && isStickSource(r)));
  return setOrDrop(into, 'analog', nonEmpty([...rest, ...moved]));
}

/** The layers a transform works on, and whether their untouched controls take their own output along. */
function layersOf(p: Profile, scope: number | 'all'): { layer: number; materialize: boolean }[] {
  if (scope !== 'all') return [{ layer: scope, materialize: true }];
  // Main carries the swap for the layers that inherit it; those swap only what they map themselves.
  return layerIds(p).map((id) => ({ layer: id, materialize: id === 0 || declaredLayer(p, id)?.inherit === false }));
}

/**
 * Apply a transform to one layer or (`'all'`) to the whole config. `def`: the stick settings'
 * defaults, for a stick that only passed through.
 */
export function transform(p: Profile, kind: TransformKind, scope: number | 'all', def: Required<StickConfig>): Profile {
  const { pairs, sticks } = pairsOf(kind);
  // Every layer reads what controls did before the transform, so a shift layer does not see
  // Main already swapped.
  let out = p;
  for (const { layer, materialize } of layersOf(p, scope)) {
    const own = (c: ControlName) => p.mappings.some((m) => layerOf(m) === layer && sameCombo(m.trigger, c));
    const use = materialize ? pairs : pairs.filter(([x, y]) => own(x) || own(y));
    out = swapMappings(p, out, layer, use, materialize);
    const ownStickRoutes = (p.analog ?? []).some((r) => routeLayer(r) === layer && isStickSource(r));
    if (sticks && (materialize || ownStickRoutes)) out = swapStickRoutes(p, out, layer, def);
  }
  return out;
}
