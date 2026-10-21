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
  Light,
  Mapping,
  Mode,
  MouseButtonName,
  PadButtonName,
  Profile,
  Timing,
  Unlock,
  VirtualDevices,
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

const D = Math.SQRT1_2;
const STICK_DIRS: Record<string, [number, number]> = {
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
  const list = comboList(combo);
  if (list.length !== 1) return null;
  const c = list[0];
  if (c.startsWith('Key:')) return { key: c.slice(4) };
  if (c.startsWith('Mouse:')) return { mouse: c.slice(6).toLowerCase() as MouseButtonName };
  if (c.startsWith('Wheel:')) return { wheel: c.slice(6).toLowerCase() as WheelDir };
  if (c.startsWith('MouseMove:')) return null;
  const stick = /^(Left|Right)Stick(UpLeft|UpRight|DownLeft|DownRight|Up|Down|Left|Right)(Low|Med|High)?$/.exec(c);
  if (stick) {
    const [x, y] = STICK_DIRS[stick[2]];
    return { stick: stick[1] === 'Left' ? 'left' : 'right', x, y };
  }
  const trigger = /^(LT|RT)(Full|Low|Med|High)?$/.exec(c);
  if (trigger) return { trigger: trigger[1] === 'LT' ? 'left' : 'right' };
  // A stick zone in any direction has no single direction to reproduce.
  if (/^(Left|Right)Stick(Low|Med|High)$/.test(c)) return null;
  if (/^Touch[A-Z][a-zA-Z0-9]*$/.test(c) && c !== 'Touchpad') return null;
  if (/^Trackpad[A-Z]/.test(c)) return null;
  if (/^Gyro[A-Z]/.test(c)) return null;
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

/** How a layer inherits from Main (layers that are not declared inherit everything). */
export function layerInheritance(p: Profile, id: number): { inherit: boolean; notInherited: Combo[] } {
  const decl = declaredLayer(p, id);
  return { inherit: decl?.inherit ?? true, notInherited: decl?.not_inherited ?? [] };
}

/** Change a layer's declaration, declaring it first if needed. */
function patchLayer(p: Profile, id: number, patch: (l: Layer) => Layer): Profile {
  const layers = [...(p.layers ?? [])];
  const i = layers.findIndex((l) => l.id === id);
  const next = patch(i >= 0 ? { ...layers[i] } : { id, inherit: true });
  if (next.not_inherited && next.not_inherited.length === 0) delete next.not_inherited;
  if (i >= 0) layers[i] = next;
  else layers.push(next);
  return { ...p, layers };
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
  const timing: Timing = { ...(p.timing ?? {}) };
  if (value === undefined) delete timing[field];
  else timing[field] = value;
  const out: Profile = { ...p, timing };
  if (Object.keys(timing).length === 0) delete out.timing;
  return out;
}

/** Set (or with `undefined` remove) one virtual device's settings, keeping the others. */
export function setVirtualDevice<K extends keyof VirtualDevices>(
  p: Profile,
  key: K,
  value: VirtualDevices[K] | undefined,
): Profile {
  const v: VirtualDevices = { ...p.virtual };
  if (value === undefined) delete v[key];
  else v[key] = value;
  const out: Profile = { ...p, virtual: v };
  if (Object.keys(v).length === 0) delete out.virtual;
  return out;
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

const COMMAND_LABELS: Record<string, string> = {
  remap_off: 'Remap off',
  toggle_gyro: 'Gyro on/off',
  gyro_on: 'Gyro on',
  gyro_off: 'Gyro off',
  reset_tilt: 'Reset tilt',
  switch_gyro_axis: 'Switch gyro axis',
  shutdown: 'Shut down',
  restart: 'Restart',
  sleep: 'Sleep',
  hibernate: 'Hibernate',
  log_off: 'Log off',
  screenshot: 'Screenshot',
  close_window: 'Close window',
  toggle_crosshair: 'Crosshair on/off',
  radial_next: 'Radial: next',
  radial_prev: 'Radial: previous',
  radial_up: 'Radial: up',
  radial_down: 'Radial: down',
  radial_left: 'Radial: left',
  radial_right: 'Radial: right',
  radial_apply: 'Radial: select',
  radial_back: 'Radial: back',
  pad_overlay: 'Controller overlay',
  mappings_overlay: 'Mappings overlay',
  power_off: 'Controller off',
};

/** Commands that end the session or put the computer to sleep: the editor asks first. */
export const POWER_COMMANDS = ['shutdown', 'restart', 'sleep', 'hibernate', 'log_off'] as const;

/** Commands without arguments, in the order the action picker lists them. */
export const SIMPLE_COMMANDS = [
  'remap_off',
  'toggle_gyro',
  'gyro_on',
  'gyro_off',
  'reset_tilt',
  'switch_gyro_axis',
  'screenshot',
  'close_window',
  'pad_overlay',
  'mappings_overlay',
  'power_off',
] as const;

/** Commands that steer an open radial menu. */
export const RADIAL_COMMANDS = [
  'radial_next',
  'radial_prev',
  'radial_up',
  'radial_down',
  'radial_left',
  'radial_right',
  'radial_apply',
  'radial_back',
] as const;

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
  if ('shake' in a) return `Shake ${a.shake.toUpperCase()}`;
  if ('turn' in a) return `Turn ${a.turn > 0 ? '+' : ''}${a.turn}°`;
  if ('touch' in a) return `Touch ${a.touch.replace(/_/g, ' ')}`;
  if ('mouse_to' in a) return `Cursor to ${Math.round(a.mouse_to[0] * 100)}%, ${Math.round(a.mouse_to[1] * 100)}%`;
  if ('mouse_clip' in a) return 'Keep cursor in area';
  if ('break' in a) return '⦀';
  if ('command' in a) {
    const c = a.command;
    if (typeof c === 'string' && c in COMMAND_LABELS) return COMMAND_LABELS[c];
    if (typeof c === 'object' && 'launch' in c) return `Run ${c.launch.path.split(/[\\/]/).pop()}`;
    if (typeof c === 'object' && 'crosshair' in c) return `Crosshair ${c.crosshair}`;
    if (typeof c === 'object' && 'unsupported' in c) return c.unsupported;
  }
  return '?';
}

function pressSuffix(p?: string): string {
  return p === 'down' ? ' ▼' : p === 'up' ? ' ▲' : '';
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

/** The light bar of `layer` (0: the profile's own), or `undefined` to drop it. */
export function setLight(p: Profile, layer: number, light: Light | undefined): Profile {
  if (layer === 0) {
    const next: Profile = { ...p, lightbar: light };
    if (!light) delete next.lightbar;
    return next;
  }
  return patchLayer(p, layer, ({ lightbar: _, ...l }) => (light ? { ...l, lightbar: light } : l));
}
