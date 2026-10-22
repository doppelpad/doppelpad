// Pure helpers for editing a Profile in the GUI. They mirror the engine's rules
// (one binding per layer + trigger + activator, inheritance from the base layer).

import { replaceLayerRoutes, routeLayer } from './routes';
import { parseControl, type Dir8 } from './controls';
import { declaredLayer, patchLayer } from './layers';
import { clone, dropUndefined, nonEmpty, sameJson, setOrDrop } from './util';
import type {
  Action,
  Activator,
  AnalogMapping,
  Combo,
  ControlName,
  Light,
  Mapping,
  Mode,
  MouseButtonName,
  PadButtonName,
  Profile,
  Timing,
  Unlock,
  WheelDir,
} from '../types';

export { setVirtualDevice } from './layers';

export const ACTIVATORS: Activator[] = ['single', 'long', 'double', 'triple', 'start', 'release'];

/** Controls of a combo as a sorted list. */
export function comboList(c: Combo): ControlName[] {
  return (Array.isArray(c) ? [...c] : [c]).sort();
}

/** Canonical key of a combo: sorted controls joined with "+". */
export function comboKey(c: Combo): string {
  return comboList(c).join('+');
}

/** A combo in profile form: a single string for one control, a list otherwise. */
export function toCombo(controls: ControlName[]): Combo {
  const sorted = [...new Set(controls)].sort();
  return sorted.length === 1 ? sorted[0] : sorted;
}

export function sameCombo(a: Combo, b: Combo): boolean {
  return comboKey(a) === comboKey(b);
}

export const layerOf = (m: Mapping) => m.layer ?? 0;
export const activatorOf = (m: Mapping): Activator => m.activator ?? 'single';
export const modeOf = (m: Mapping): Mode => m.mode ?? 'hold';

/** The control of a combo of exactly one, else null. */
export function singleControl(c: Combo): ControlName | null {
  const list = comboList(c);
  return list.length === 1 ? list[0] : null;
}

/** Modes that play the outputs as a sequence (the macro editor's modes). */
export const isMacroMode = (mode: Mode) => mode === 'once' || mode === 'repeat' || mode === 'while_held';

/** A mapping without outputs or a layer jump: it blocks its control. */
export const isBlocked = (m: Mapping | undefined) => !!m && (m.actions ?? []).length === 0 && !m.jump;

/** The mapping for exactly this layer, trigger and activator (no inheritance). */
export function findMapping(p: Profile, layer: number, combo: Combo, activator: Activator): Mapping | undefined {
  return p.mappings.find((m) => layerOf(m) === layer && activatorOf(m) === activator && sameCombo(m.trigger, combo));
}

/** All mappings of this trigger defined directly on `layer`. */
export function ownMappings(p: Profile, layer: number, combo: Combo): Mapping[] {
  return p.mappings.filter((m) => layerOf(m) === layer && sameCombo(m.trigger, combo));
}

/** Whether an unlock layer gives this physical keyboard/mouse control back. */
export function unlocks(unlock: Unlock | undefined, c: ControlName): boolean {
  if (!unlock) return false;
  if (c.startsWith('Key:')) return unlock !== 'mouse';
  return /^(Mouse|Wheel|MouseMove):/.test(c) && unlock !== 'keyboard';
}

/**
 * Mappings that are effective for a trigger on `layer`: the layer's own ones, otherwise the
 * base layer's unless the layer does not inherit, lists the trigger as not inherited or gives
 * the trigger's keyboard/mouse back.
 */
export function effectiveMappings(p: Profile, layer: number, combo: Combo): { mappings: Mapping[]; inherited: boolean } {
  const own = ownMappings(p, layer, combo);
  if (layer === 0 || own.length > 0) return { mappings: own, inherited: false };
  const decl = declaredLayer(p, layer);
  if (decl && decl.inherit === false) return { mappings: [], inherited: false };
  if (decl?.not_inherited?.some((c) => sameCombo(c, combo))) return { mappings: [], inherited: false };
  if (comboList(combo).some((c) => unlocks(decl?.unlock, c))) return { mappings: [], inherited: false };
  const base = ownMappings(p, 0, combo);
  return { mappings: base, inherited: base.length > 0 };
}

/** A layer's own mapping for a trigger + activator, else the one it inherits from Main. */
export function mappingAt(p: Profile, layer: number, combo: Combo, activator: Activator): { own?: Mapping; inherited?: Mapping } {
  const own = findMapping(p, layer, combo, activator);
  if (own) return { own };
  const eff = effectiveMappings(p, layer, combo);
  return { inherited: eff.inherited ? eff.mappings.find((m) => activatorOf(m) === activator) : undefined };
}

/**
 * `p` with one mapping changed by `change`. It starts from the layer's own mapping, else a copy
 * of the inherited one, else a new mapping. A change that leaves nothing behind removes the
 * mapping, unless `keepEmpty` is set (an empty mapping blocks the control).
 */
export function editMappingIn(
  p: Profile,
  layer: number,
  combo: Combo,
  activator: Activator,
  change: (m: Mapping) => void,
  keepEmpty = false,
): Profile {
  const { own, inherited } = mappingAt(p, layer, combo, activator);
  const from = own ?? inherited;
  const m: Mapping = from ? clone(from) : newMapping(layer, combo, activator);
  change(m);
  const tidy = tidyMapping({ ...m, layer, activator });
  const empty = (tidy.actions ?? []).length === 0 && !tidy.jump && !tidy.description && modeOf(tidy) === 'hold';
  return empty && !keepEmpty ? removeMapping(p, layer, combo, activator) : upsertMapping(p, tidy);
}

/**
 * Switch a mapping to `next`. Turbo and toggle act on the outputs, so a mapping without any
 * starts from its control's own output (`native`: turbo on A pulses A); back in hold mode a
 * mapping that is just that output loses it again.
 */
export function applyMode(m: Mapping, next: Mode, native: Action | null) {
  m.mode = next;
  const acts = m.actions ?? [];
  if (next !== 'hold' && acts.length === 0 && native) m.actions = [{ ...native }];
  const onlyNative = acts.length === 1 && !!native && sameAction(acts[0], native) && !m.jump && !m.description;
  if (next === 'hold' && onlyNative) m.actions = [];
}

/** Insert or replace the mapping with the same layer + trigger + activator. */
export function upsertMapping(p: Profile, m: Mapping): Profile {
  const layer = layerOf(m);
  const act = activatorOf(m);
  const idx = p.mappings.findIndex((x) => layerOf(x) === layer && activatorOf(x) === act && sameCombo(x.trigger, m.trigger));
  const mappings = [...p.mappings];
  if (idx >= 0) mappings[idx] = m;
  else mappings.push(m);
  return { ...p, mappings };
}

export function removeMapping(p: Profile, layer: number, combo: Combo, activator: Activator): Profile {
  return {
    ...p,
    mappings: p.mappings.filter((m) => !(layerOf(m) === layer && activatorOf(m) === activator && sameCombo(m.trigger, combo))),
  };
}

/** A new, empty mapping in profile form (defaults omitted like Rust does). */
export function newMapping(layer: number, combo: Combo, activator: Activator): Mapping {
  const m: Mapping = { trigger: combo };
  if (layer !== 0) m.layer = layer;
  if (activator !== 'single') m.activator = activator;
  return m;
}

const D = Math.SQRT1_2;
const STICK_DIRS: Record<Dir8, [number, number]> = {
  Up: [0, 1],
  Down: [0, -1],
  Left: [-1, 0],
  Right: [1, 0],
  UpLeft: [-D, D],
  UpRight: [D, D],
  DownLeft: [-D, -D],
  DownRight: [D, -D],
};

/**
 * The output a single control produces when it is not remapped (pad A → pad A, a key →
 * that key, a trigger zone → that trigger), or null where there is none to reproduce (touch
 * regions, gyro, mouse motion, stick zones in any direction, shortcuts of several controls).
 */
export function nativeAction(combo: Combo): Action | null {
  const c = singleControl(combo);
  const p = c === null ? null : parseControl(c);
  switch (p?.kind) {
    case 'key':
      return { key: p.key };
    case 'mouse':
      return { mouse: p.button.toLowerCase() as MouseButtonName };
    case 'wheel':
      return { wheel: p.dir.toLowerCase() as WheelDir };
    case 'stick_dir':
    case 'stick_zone': {
      // A stick zone in any direction has no single direction to reproduce.
      if (!p.dir) return null;
      const [x, y] = STICK_DIRS[p.dir];
      return { stick: p.side, x, y };
    }
    case 'trigger_press':
    case 'trigger_zone':
    case 'trigger_full':
      return { trigger: p.side };
    case 'button':
      return { pad: c as PadButtonName };
    default:
      return null;
  }
}

export function sameAction(a: Action, b: Action): boolean {
  return sameJson(a, b);
}

/** Remove fields that equal their default, so saved files stay small and stable. */
export function tidyMapping(m: Mapping): Mapping {
  const out = dropUndefined(m);
  if (out.layer === 0) delete out.layer;
  if (out.activator === 'single') delete out.activator;
  if (out.mode === 'hold') delete out.mode;
  if (out.actions && out.actions.length === 0) delete out.actions;
  if (out.description === '') delete out.description;
  return out;
}

/** Layer ids in use: 0 plus declared layers plus layers referenced by mappings, jumps or routes. */
export function layerIds(p: Profile): number[] {
  const ids = new Set<number>([0]);
  for (const l of p.layers ?? []) ids.add(l.id);
  for (const m of p.mappings) {
    ids.add(layerOf(m));
    if (m.jump) ids.add(m.jump.layer);
  }
  for (const r of p.analog ?? []) ids.add(routeLayer(r));
  return [...ids].sort((a, b) => a - b);
}

export function layerName(p: Profile, id: number, mainName: string, shiftName: (n: number) => string): string {
  if (id === 0) return mainName;
  return declaredLayer(p, id)?.name || shiftName(id);
}

export function addLayer(p: Profile): { profile: Profile; id: number } {
  const id = Math.max(0, ...layerIds(p)) + 1;
  return { profile: { ...p, layers: [...(p.layers ?? []), { id, inherit: true }] }, id };
}

export function renameLayer(p: Profile, id: number, name: string): Profile {
  return patchLayer(p, id, (l) => ({ ...l, name }));
}

/** How a layer inherits from Main (layers that are not declared inherit everything). */
export function layerInheritance(p: Profile, id: number): { inherit: boolean; notInherited: Combo[] } {
  const decl = declaredLayer(p, id);
  return { inherit: decl?.inherit ?? true, notInherited: decl?.not_inherited ?? [] };
}

/** Whether a layer takes over Main's mappings for the controls it does not map itself. */
export function setLayerInherit(p: Profile, id: number, inherit: boolean): Profile {
  return patchLayer(p, id, (l) => ({ ...l, inherit }));
}

/** What physical input a layer gives back while it is active (undeclared layers: none). */
export function layerUnlock(p: Profile, id: number): Unlock | undefined {
  return declaredLayer(p, id)?.unlock;
}

/** Give the physical keyboard and/or mouse back while the layer is active, or stop doing so. */
export function setLayerUnlock(p: Profile, id: number, unlock: Unlock | undefined): Profile {
  return patchLayer(p, id, ({ unlock: _, ...l }) => (unlock ? { ...l, unlock } : l));
}

/** Let one trigger of Main reach a layer (or keep it out). */
export function setTriggerInherited(p: Profile, id: number, combo: Combo, inherited: boolean): Profile {
  return patchLayer(p, id, (l) => {
    const rest = (l.not_inherited ?? []).filter((c) => !sameCombo(c, combo));
    return { ...l, not_inherited: inherited ? rest : [...rest, toCombo(comboList(combo))] };
  });
}

/** Triggers mapped on Main, each once, in a stable order: what a layer can inherit. */
export function inheritableTriggers(p: Profile): Combo[] {
  const seen = new Map<string, Combo>();
  for (const m of p.mappings) {
    if (layerOf(m) === 0 && !seen.has(comboKey(m.trigger))) seen.set(comboKey(m.trigger), m.trigger);
  }
  return [...seen.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([, c]) => c);
}

export type TimingField = keyof Timing;

/** Set (or with `undefined` reset to the default) one profile-wide timing value. */
export function setProfileTiming(p: Profile, field: TimingField, value: number | undefined): Profile {
  const timing: Timing = setOrDrop({ ...p.timing }, field, value);
  return setOrDrop(p, 'timing', nonEmpty(timing));
}

/** Remove a layer, its mappings and routes, and jumps that lead to it. */
export function removeLayer(p: Profile, id: number): Profile {
  return replaceLayerRoutes(
    {
      ...p,
      layers: (p.layers ?? []).filter((l) => l.id !== id),
      mappings: p.mappings.filter((m) => layerOf(m) !== id).map((m) => (m.jump?.layer === id ? tidyMapping({ ...m, jump: undefined }) : m)),
    },
    id,
    [],
  );
}

/** Remove a layer's mappings and routes. */
export function clearLayer(p: Profile, id: number): Profile {
  return replaceLayerRoutes({ ...p, mappings: p.mappings.filter((m) => layerOf(m) !== id) }, id, []);
}

/** What "Copy mappings" keeps of a layer. */
export interface LayerCopy {
  mappings: Mapping[];
  analog: AnalogMapping[];
}

export function copyLayer(p: Profile, id: number): LayerCopy {
  return clone({
    mappings: p.mappings.filter((m) => layerOf(m) === id),
    analog: (p.analog ?? []).filter((r) => routeLayer(r) === id),
  });
}

/** A copied layer, re-targeted to another layer (replacing its mappings and routes). */
export function pasteLayer(p: Profile, copied: LayerCopy, into: number): Profile {
  const rest = p.mappings.filter((m) => layerOf(m) !== into);
  const moved = copied.mappings.map((m) => tidyMapping({ ...clone(m), layer: into }));
  return replaceLayerRoutes({ ...p, mappings: [...rest, ...moved] }, into, copied.analog);
}

/** Mappings whose trigger is a combo of 2+ controls, on a layer. */
export function shortcuts(p: Profile, layer: number): Mapping[] {
  return p.mappings.filter((m) => layerOf(m) === layer && Array.isArray(m.trigger) && m.trigger.length > 1);
}

/** Controls that take part in a shortcut on this layer (for badges). */
export function shortcutMembers(p: Profile, layer: number): Set<ControlName> {
  const s = new Set<ControlName>();
  for (const m of shortcuts(p, layer)) for (const c of comboList(m.trigger)) s.add(c);
  return s;
}

/** Number of segments the break steps cut a sequence into (at least one). */
export function segmentCount(actions: Action[]): number {
  let n = 0;
  let open = false;
  for (const a of actions) {
    if ('break' in a) {
      if (open) n++;
      open = false;
    } else open = true;
  }
  return Math.max(1, n + (open ? 1 : 0));
}

/** The light bar of `layer` (0: the profile's own), or `undefined` to drop it. */
export function setLight(p: Profile, layer: number, light: Light | undefined): Profile {
  if (layer === 0) return setOrDrop(p, 'lightbar', light);
  return patchLayer(p, layer, ({ lightbar: _, ...l }) => (light ? { ...l, lightbar: light } : l));
}
