// Whole-stick mappings: one choice that makes a stick a mouse, WASD, the arrow keys or a
// flick stick on a layer. The result is ordinary analog routes and direction mappings.

import { type Side } from './analog';
import { MOUSE_PRESETS, addRoute, newRoute, routeLayer, sourceOf, targetOf } from './routes';
import { CARDINALS, DIAGONALS, SIDES, zoneFamily } from './controls';
import { activatorOf, comboList, layerOf, newMapping, ownMappings, upsertMapping } from './profile';
import type { AnalogMapping, Mapping, Profile } from '../types';
import { nonEmpty, setOrDrop } from './util';

export type StickGroup = 'mouse' | 'wasd' | 'arrows' | 'flick';
export const STICK_GROUPS: StickGroup[] = ['mouse', 'wasd', 'arrows', 'flick'];

type Cardinal = (typeof CARDINALS)[number];

const GROUP_KEYS: Record<'wasd' | 'arrows', Record<Cardinal, string>> = {
  wasd: { Up: 'W', Left: 'A', Down: 'S', Right: 'D' },
  arrows: { Up: 'Up', Left: 'Left', Down: 'Down', Right: 'Right' },
};

/** A stick's direction controls: cardinals, diagonals and their zones. */
function directionControls(s: Side): Set<string> {
  return new Set([...CARDINALS, ...DIAGONALS].flatMap((d) => zoneFamily(SIDES[s].stick + d)));
}

/** A mapping of one of the stick's directions on the layer (not part of a shortcut). */
function isDirectionMapping(m: Mapping, layer: number, dirs: Set<string>): boolean {
  const list = comboList(m.trigger);
  return layerOf(m) === layer && list.length === 1 && dirs.has(list[0]);
}

const isStickRoute = (r: AnalogMapping, layer: number, s: Side) => routeLayer(r) === layer && sourceOf(r) === SIDES[s].source;

/** Index of the layer's own route from a stick, or -1. */
export function stickRouteIndex(p: Profile, layer: number, s: Side): number {
  return (p.analog ?? []).findIndex((r) => isStickRoute(r, layer, s));
}

/** What the whole stick does on a layer: its route's target, or the keys its four directions send. */
export function stickGroupOf(p: Profile, layer: number, s: Side): StickGroup | null {
  const r = p.analog?.[stickRouteIndex(p, layer, s)];
  if (r) {
    const target = targetOf(r);
    return target === 'mouse' || target === 'flick' ? target : null;
  }
  for (const g of ['wasd', 'arrows'] as const) {
    const sends = (d: Cardinal) => {
      const ms = ownMappings(p, layer, SIDES[s].stick + d);
      const a = ms.length === 1 && activatorOf(ms[0]) === 'single' ? ms[0].actions : undefined;
      return a?.length === 1 && 'key' in a[0] && a[0].key === GROUP_KEYS[g][d];
    };
    if (CARDINALS.every(sends)) return g;
  }
  return null;
}

/** How many direction mappings and routes of the stick a new choice would replace on the layer. */
export function stickGroupConflicts(p: Profile, layer: number, s: Side): number {
  const dirs = directionControls(s);
  return (
    p.mappings.filter((m) => isDirectionMapping(m, layer, dirs)).length +
    (p.analog ?? []).filter((r) => isStickRoute(r, layer, s)).length
  );
}

/**
 * The profile with the whole stick doing one thing on a layer (`null`: nothing). The stick's
 * direction mappings and routes on that layer go; a mouse route starts with the standard preset.
 */
export function setStickGroup(
  p: Profile,
  layer: number,
  s: Side,
  group: StickGroup | null,
  def: Required<AnalogMapping>,
): Profile {
  const dirs = directionControls(s);
  const analog = (p.analog ?? []).filter((r) => !isStickRoute(r, layer, s));
  let out: Profile = setOrDrop(
    { ...p, mappings: p.mappings.filter((m) => !isDirectionMapping(m, layer, dirs)), analog },
    'analog',
    nonEmpty(analog),
  );
  const source = SIDES[s].source;
  if (group === 'mouse') return addRoute(out, newRoute(def, layer, { source, target: 'mouse', ...MOUSE_PRESETS.standard }));
  if (group === 'flick') return addRoute(out, newRoute(def, layer, { source, target: 'flick', deadzone: 0.6 }));
  if (group) {
    for (const d of CARDINALS) {
      out = upsertMapping(out, { ...newMapping(layer, SIDES[s].stick + d, 'single'), actions: [{ key: GROUP_KEYS[group][d] }] });
    }
  }
  return out;
}
