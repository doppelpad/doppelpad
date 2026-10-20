// Pure helpers for editing a Profile in the GUI. They mirror the engine's rules
// (one binding per layer + trigger + activator, inheritance from the base layer).

import { replaceLayerRoutes, routeLayer } from './analog';
import type {
  Action,
  Activator,
  AnalogMapping,
  Combo,
  ControlName,
  Layer,
  Mapping,
  Mode,
  MouseButtonName,
  PadButtonName,
  Profile,
  WheelDir,
} from '../types';

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

/** The mapping for exactly this layer, trigger and activator (no inheritance). */
export function findMapping(p: Profile, layer: number, combo: Combo, activator: Activator): Mapping | undefined {
  return p.mappings.find(
    (m) => layerOf(m) === layer && activatorOf(m) === activator && sameCombo(m.trigger, combo),
  );
}

/** All mappings of this trigger defined directly on `layer`. */
export function ownMappings(p: Profile, layer: number, combo: Combo): Mapping[] {
  return p.mappings.filter((m) => layerOf(m) === layer && sameCombo(m.trigger, combo));
}

function declaredLayer(p: Profile, id: number): Layer | undefined {
  return p.layers?.find((l) => l.id === id);
}

/**
 * Mappings that are effective for a trigger on `layer`: the layer's own ones, otherwise the
 * base layer's unless the layer does not inherit or lists the trigger as not inherited.
 */
export function effectiveMappings(p: Profile, layer: number, combo: Combo): { mappings: Mapping[]; inherited: boolean } {
  const own = ownMappings(p, layer, combo);
  if (layer === 0 || own.length > 0) return { mappings: own, inherited: false };
  const decl = declaredLayer(p, layer);
  if (decl && decl.inherit === false) return { mappings: [], inherited: false };
  if (decl?.not_inherited?.some((c) => sameCombo(c, combo))) return { mappings: [], inherited: false };
  const base = ownMappings(p, 0, combo);
  return { mappings: base, inherited: base.length > 0 };
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
    mappings: p.mappings.filter(
      (m) => !(layerOf(m) === layer && activatorOf(m) === activator && sameCombo(m.trigger, combo)),
    ),
  };
}

/** A new, empty mapping in profile form (defaults omitted like Rust does). */
export function newMapping(layer: number, combo: Combo, activator: Activator): Mapping {
  const m: Mapping = { trigger: combo };
  if (layer !== 0) m.layer = layer;
  if (activator !== 'single') m.activator = activator;
  return m;
}

const STICK_DIRS: Record<string, [number, number]> = { Up: [0, 1], Down: [0, -1], Left: [-1, 0], Right: [1, 0] };

/**
 * The output a single control produces when it is not remapped (pad A → pad A, a key →
 * that key, a trigger zone → that trigger), or null where there is none to reproduce (touch
 * regions, gyro, mouse motion, stick zones in any direction, shortcuts of several controls).
 */
export function nativeAction(combo: Combo): Action | null {
  const list = comboList(combo);
  if (list.length !== 1) return null;
  const c = list[0];
  if (c.startsWith('Key:')) return { key: c.slice(4) };
  if (c.startsWith('Mouse:')) return { mouse: c.slice(6).toLowerCase() as MouseButtonName };
  if (c.startsWith('Wheel:')) return { wheel: c.slice(6).toLowerCase() as WheelDir };
  if (c.startsWith('MouseMove:')) return null;
  const stick = /^(Left|Right)Stick(Up|Down|Left|Right)(Low|Med|High)?$/.exec(c);
  if (stick) {
    const [x, y] = STICK_DIRS[stick[2]];
    return { stick: stick[1] === 'Left' ? 'left' : 'right', x, y };
  }
  const trigger = /^(LT|RT)(Full|Low|Med|High)?$/.exec(c);
  if (trigger) return { trigger: trigger[1] === 'LT' ? 'left' : 'right' };
  // A stick zone in any direction has no single direction to reproduce.
  if (/^(Left|Right)Stick(Low|Med|High)$/.test(c)) return null;
  if (/^Touch(Up|Down|Left|Right|Center)$/.test(c) || /^Gyro(Up|Down|Left|Right)$/.test(c)) return null;
  return { pad: c as PadButtonName };
}

export function sameAction(a: Action, b: Action): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Remove fields that equal their default, so saved files stay small and stable. */
export function tidyMapping(m: Mapping): Mapping {
  const out: Mapping = { ...m };
  if (out.layer === 0) delete out.layer;
  if (out.activator === 'single') delete out.activator;
  if (out.mode === 'hold') delete out.mode;
  if (out.actions && out.actions.length === 0) delete out.actions;
  if (out.description === '') delete out.description;
  for (const k of ['turbo_ms', 'repeat', 'repeat_pause_ms', 'long_press_ms', 'double_tap_ms', 'triple_tap_ms', 'key_delay_ms'] as const) {
    if (out[k] === undefined || out[k] === null) delete out[k];
  }
  if (out.jump === undefined || out.jump === null) delete out.jump;
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
  const layers = [...(p.layers ?? [])];
  const i = layers.findIndex((l) => l.id === id);
  if (i >= 0) layers[i] = { ...layers[i], name };
  else layers.push({ id, name, inherit: true });
  return { ...p, layers };
}

/** Remove a layer, its mappings and routes, and jumps that lead to it. */
export function removeLayer(p: Profile, id: number): Profile {
  return replaceLayerRoutes(
    {
      ...p,
      layers: (p.layers ?? []).filter((l) => l.id !== id),
      mappings: p.mappings
        .filter((m) => layerOf(m) !== id)
        .map((m) => (m.jump?.layer === id ? tidyMapping({ ...m, jump: undefined }) : m)),
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
  return structuredClone({
    mappings: p.mappings.filter((m) => layerOf(m) === id),
    analog: (p.analog ?? []).filter((r) => routeLayer(r) === id),
  });
}

/** A copied layer, re-targeted to another layer (replacing its mappings and routes). */
export function pasteLayer(p: Profile, copied: LayerCopy, into: number): Profile {
  const rest = p.mappings.filter((m) => layerOf(m) !== into);
  const moved = copied.mappings.map((m) => tidyMapping({ ...structuredClone(m), layer: into }));
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

// ----- labels -------------------------------------------------------------------------------

const ARROWS: Record<string, string> = { up: '↑', down: '↓', left: '←', right: '→' };

/** Short text for one action, e.g. "Space", "Mouse L", "Pad A", "L-stick ↑", "Pause 50 ms". */
export function actionLabel(a: Action): string {
  if ('key' in a) return a.key + pressSuffix(a.action);
  if ('pad' in a) return `Pad ${a.pad}${pressSuffix(a.action)}`;
  if ('mouse' in a) return `Mouse ${a.mouse}${pressSuffix(a.action)}`;
  if ('wheel' in a) return `Wheel ${ARROWS[a.wheel] ?? a.wheel}`;
  if ('stick' in a) {
    const dir = a.y > 0.5 ? '↑' : a.y < -0.5 ? '↓' : a.x < -0.5 ? '←' : a.x > 0.5 ? '→' : '•';
    return `${a.stick === 'left' ? 'L' : 'R'}-stick ${dir}${pressSuffix(a.action)}`;
  }
  if ('trigger' in a) return `${a.trigger === 'left' ? 'LT' : 'RT'}${a.value !== undefined && a.value < 1 ? ` ${Math.round(a.value * 100)}%` : ''}`;
  if ('mouse_move' in a) return `Mouse ${ARROWS[a.mouse_move] ?? a.mouse_move}`;
  if ('pause' in a) return `${a.pause} ms`;
  if ('rumble' in a) return `Rumble ${a.rumble} ms`;
  if ('command' in a) {
    const c = a.command;
    if (c === 'remap_off') return 'Remap off';
    if (c === 'toggle_gyro') return 'Gyro on/off';
    if (typeof c === 'object' && 'launch' in c) return `Run ${c.launch.path.split(/[\\/]/).pop()}`;
    if (typeof c === 'object' && 'unsupported' in c) return c.unsupported;
  }
  return '?';
}

function pressSuffix(p?: string): string {
  return p === 'down' ? ' ▼' : p === 'up' ? ' ▲' : '';
}

/** Short summary of a mapping's outputs ("Ctrl + C", "Turbo Mouse left", "→ Shift 1"). */
export function mappingSummary(m: Mapping, layerLabel: (id: number) => string): string {
  const acts = (m.actions ?? []).filter((a) => !('pause' in a));
  let s = acts.slice(0, 3).map(actionLabel).join(' + ');
  if (acts.length > 3) s += ' …';
  const mode = modeOf(m);
  if (mode === 'turbo') s = `⚡ ${s}`;
  if (mode === 'toggle') s = `⇄ ${s}`;
  if (mode === 'once' || mode === 'repeat' || mode === 'while_held') s = `▶ ${s}`;
  if (m.jump) s = s ? `${s} · → ${layerLabel(m.jump.layer)}` : `→ ${layerLabel(m.jump.layer)}`;
  if (!s) s = '⊘';
  return s;
}
