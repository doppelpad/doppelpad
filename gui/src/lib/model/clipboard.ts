// Copy and paste of what controls do on a layer: one control (or shortcut) with all of its
// activators, or a direction group (a stick's, the D-pad's, the touchpad's, a trackpad's, the
// gyro's or the mouse's directions and zones, with the analog routes it is the source of).

import { CARDINALS, DIAGONALS, DIRS8, GYRO_DIRS, SIDES, TOUCH_REGIONS, TRACKPAD_PARTS, parseControl, zonedParts } from './controls';
import { comboList, effectiveMappings, layerOf, ownMappings, sameCombo, tidyMapping } from './profile';
import { layerRoutes, routeLayer, routeTargets, sourceOf, targetOf } from './routes';
import { clone, nonEmpty, setOrDrop } from './util';
import type { AnalogMapping, AnalogSource, Combo, ControlName, Mapping, Profile } from '../types';

/** A group of direction-like controls that share their part names (`Up`, `UpLeftHigh`, `Center` …). */
export type GroupId = 'LeftStick' | 'RightStick' | 'Dpad' | 'Touch' | 'TrackpadLeft' | 'TrackpadRight' | 'Gyro' | 'MouseMove';

/** Per group: the prefix of its control names, its parts, and the route source it is (if any). */
export const GROUPS: Record<GroupId, { prefix: string; parts: ReadonlySet<string>; source?: AnalogSource }> = {
  LeftStick: { prefix: 'LeftStick', parts: new Set(zonedParts(DIRS8)), source: 'left_stick' },
  RightStick: { prefix: 'RightStick', parts: new Set(zonedParts(DIRS8)), source: 'right_stick' },
  Dpad: { prefix: 'Dpad', parts: new Set(CARDINALS) },
  Touch: { prefix: 'Touch', parts: new Set([...TOUCH_REGIONS, ...DIAGONALS]), source: 'touchpad' },
  TrackpadLeft: { prefix: 'TrackpadLeft', parts: new Set(TRACKPAD_PARTS) },
  TrackpadRight: { prefix: 'TrackpadRight', parts: new Set(TRACKPAD_PARTS) },
  Gyro: { prefix: 'Gyro', parts: new Set(zonedParts(GYRO_DIRS)), source: 'gyro' },
  MouseMove: { prefix: 'MouseMove:', parts: new Set(zonedParts(DIRS8)), source: 'mouse' },
};

/** A direction, a zone, or a direction in a zone, as a part name (`UpLeftHigh`). */
const zonedPart = (dir: string | null, zone: string | null) => (dir ?? '') + (zone ?? '');

/** The group a control belongs to and its part in it; `null` for other controls. */
export function groupOf(c: ControlName): { group: GroupId; part: string } | null {
  const p = parseControl(c);
  switch (p?.kind) {
    case 'stick_dir':
      return { group: SIDES[p.side].stick, part: p.dir };
    case 'stick_zone':
      return { group: SIDES[p.side].stick, part: zonedPart(p.dir, p.zone) };
    case 'gyro':
      return { group: 'Gyro', part: p.dir };
    case 'gyro_zone':
      return { group: 'Gyro', part: zonedPart(p.dir, p.zone) };
    case 'mouse_move':
      return { group: 'MouseMove', part: p.dir };
    case 'mouse_zone':
      return { group: 'MouseMove', part: zonedPart(p.dir, p.zone) };
    case 'touch':
      return { group: 'Touch', part: p.region };
    case 'trackpad':
      return { group: p.half === 'Left' ? 'TrackpadLeft' : 'TrackpadRight', part: p.part };
    case 'button': {
      // The D-pad's directions are buttons to the engine.
      const part = c.slice(GROUPS.Dpad.prefix.length);
      return c.startsWith(GROUPS.Dpad.prefix) && GROUPS.Dpad.parts.has(part) ? { group: 'Dpad', part } : null;
    }
    default:
      return null;
  }
}

/** A mapping as it is copied: without the layer and the trigger it had. */
type Detached = Omit<Mapping, 'layer' | 'trigger'>;

/** What was copied. */
export type Copied =
  | { kind: 'control'; source: Combo; mappings: Detached[] }
  | { kind: 'group'; group: GroupId; parts: { part: string; mappings: Detached[] }[]; routes: AnalogMapping[] };

/** Result of a paste: the new profile and how many mappings or routes the target had no room for. */
export interface Pasted {
  profile: Profile;
  dropped: number;
}

const detach = (ms: Mapping[]): Detached[] =>
  ms.map((m) => {
    const { layer: _layer, trigger: _trigger, ...rest } = clone(m);
    return rest;
  });

const attach = (ms: Detached[], layer: number, trigger: Combo): Mapping[] => ms.map((m) => tidyMapping({ ...clone(m), layer, trigger }));

const isOn = (m: Mapping, layer: number, trigger: Combo) => layerOf(m) === layer && sameCombo(m.trigger, trigger);

/** What a control or shortcut does on a layer, all activators (Main's when the layer inherits them). */
export function copyControl(p: Profile, layer: number, trigger: Combo): Copied {
  return { kind: 'control', source: clone(trigger), mappings: detach(effectiveMappings(p, layer, trigger).mappings) };
}

/** The layer's own mappings of a control or shortcut removed (Main's apply again where inherited). */
export function clearControl(p: Profile, layer: number, trigger: Combo): Profile {
  return { ...p, mappings: p.mappings.filter((m) => !isOn(m, layer, trigger)) };
}

/** Whether the layer has mappings of its own for the control or shortcut. */
export function hasOwn(p: Profile, layer: number, trigger: Combo): boolean {
  return ownMappings(p, layer, trigger).length > 0;
}

/** A copied control onto another control or shortcut: its own mappings on the layer are replaced. */
export function pasteControl(p: Profile, layer: number, target: Combo, copied: Copied & { kind: 'control' }): Profile {
  const rest = clearControl(p, layer, target);
  return { ...rest, mappings: [...rest.mappings, ...attach(copied.mappings, layer, target)] };
}

/** Control names of a group's parts. */
const controlOf = (group: GroupId, part: string): ControlName => GROUPS[group].prefix + part;

/** A single-control mapping of the group on the layer. */
function isGroupMapping(m: Mapping, layer: number, group: GroupId): boolean {
  const list = comboList(m.trigger);
  return layerOf(m) === layer && list.length === 1 && groupOf(list[0]!)?.group === group;
}

const isGroupRoute = (r: AnalogMapping, layer: number, group: GroupId) => {
  const source = GROUPS[group].source;
  return !!source && routeLayer(r) === layer && sourceOf(r) === source;
};

/** What a group's parts do on a layer (Main's where inherited), and the layer's routes from it. */
export function copyGroup(p: Profile, layer: number, group: GroupId): Copied {
  const parts = [...GROUPS[group].parts]
    .map((part) => ({ part, mappings: detach(effectiveMappings(p, layer, controlOf(group, part)).mappings) }))
    .filter((x) => x.mappings.length > 0);
  const source = GROUPS[group].source;
  const routes = layerRoutes(p, layer)
    .filter((r) => !!source && sourceOf(r.route) === source)
    .map((r) => clone(r.route));
  return { kind: 'group', group, parts, routes };
}

/** Whether the layer has mappings or routes of its own for the group. */
export function groupHasOwn(p: Profile, layer: number, group: GroupId): boolean {
  return p.mappings.some((m) => isGroupMapping(m, layer, group)) || (p.analog ?? []).some((r) => isGroupRoute(r, layer, group));
}

/** The layer's own mappings of the group's parts, and its routes from the group, removed. */
export function clearGroup(p: Profile, layer: number, group: GroupId): Profile {
  const analog = (p.analog ?? []).filter((r) => !isGroupRoute(r, layer, group));
  return setOrDrop({ ...p, mappings: p.mappings.filter((m) => !isGroupMapping(m, layer, group)), analog }, 'analog', nonEmpty(analog));
}

/**
 * A copied group onto another group: the target's own mappings and routes on the layer are
 * replaced, parts move by name (left stick ↑ → right stick ↑). Parts the target lacks (diagonals
 * on the D-pad, zones on the touchpad) and routes the target cannot be the source of are dropped.
 */
export function pasteGroup(p: Profile, layer: number, target: GroupId, copied: Copied & { kind: 'group' }): Pasted {
  const into = GROUPS[target];
  let dropped = 0;
  const mappings: Mapping[] = [];
  for (const { part, mappings: ms } of copied.parts) {
    if (into.parts.has(part)) mappings.push(...attach(ms, layer, controlOf(target, part)));
    else dropped += ms.length;
  }
  const routes: AnalogMapping[] = [];
  for (const r of copied.routes) {
    const ok = !!into.source && routeTargets(into.source).includes(targetOf(r));
    if (ok) routes.push(setOrDrop({ ...clone(r), source: into.source }, 'layer', layer || undefined));
    else dropped++;
  }
  const rest = clearGroup(p, layer, target);
  const analog = [...(rest.analog ?? []), ...routes];
  return { profile: setOrDrop({ ...rest, mappings: [...rest.mappings, ...mappings] }, 'analog', nonEmpty(analog)), dropped };
}
