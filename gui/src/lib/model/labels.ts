// Texts for controls, layers and mappings as the views, the overlays and the bindings sheet
// show them.

import { has, t } from '../i18n/index.svelte';
import type { Action, Combo, Library, Mapping, PadFamily, Profile, Template } from '../types';
import { controlLabel } from './controls';
import { comboKey, comboList, effectiveMappings, isMacroMode, layerName, layerOf, modeOf } from './profile';

/** *Game › Config* for a config in a game, a loose config's name, else the file name. */
export function configName(lib: Library, path: string): string {
  for (const g of lib.games) {
    const c = g.configs.find((c) => c.path === path);
    if (c) return `${g.meta.name} › ${c.name}`;
  }
  const loose = lib.unsorted.find((c) => c.path === path);
  return loose?.name ?? path.split(/[\\/]/).pop() ?? path;
}

/** The config templates in the order the app offers them. */
export const TEMPLATES: Template[] = ['virtual_xbox', 'virtual_ds4', 'desktop', 'keyboard_mouse'];

/** A combo as the user reads it, e.g. "LB + A". */
export function comboLabel(c: Combo, family: PadFamily | null): string {
  return comboList(c)
    .map((x) => controlLabel(x, family))
    .join(' + ');
}

/** The name of a layer without one of its own: "Main", "Shift 2". */
export function defaultLayerName(id: number): string {
  return id === 0 ? t('layers.main') : t('layers.shift', { n: id });
}

/** A layer's name in `p`. */
export function layerLabel(p: Profile, id: number): string {
  return layerName(p, id, defaultLayerName(0), defaultLayerName);
}

/** What a control does on `layer` and whether that comes from Main; `null` when nothing. */
export function effectiveSummary(
  p: Profile,
  layer: number,
  combo: Combo,
  label: (id: number) => string,
): { text: string; inherited: boolean } | null {
  const eff = effectiveMappings(p, layer, combo);
  if (eff.mappings.length === 0) return null;
  return { text: eff.mappings.map((m) => mappingSummary(m, label)).join(' · '), inherited: eff.inherited };
}

/** One trigger in the mappings overlay: its control, what it does, and the notes. */
export interface OverlayRow {
  control: string;
  action: string;
  note: string;
}

/** The mappings overlay's rows for `layer`: the layer's own triggers first, then Main's it takes over. */
export function overlayRows(p: Profile, layer: number, family: PadFamily | null): OverlayRow[] {
  const label = (id: number) => layerLabel(p, id);
  const seen = new Set<string>();
  const triggers: Combo[] = [];
  const own = p.mappings.filter((m) => layerOf(m) === layer);
  const base = p.mappings.filter((m) => layerOf(m) === 0);
  for (const m of [...own, ...base]) {
    const k = comboKey(m.trigger);
    if (!seen.has(k)) {
      seen.add(k);
      triggers.push(m.trigger);
    }
  }
  return triggers.flatMap((trigger) => {
    const eff = effectiveMappings(p, layer, trigger);
    if (eff.mappings.length === 0) return [];
    return [
      {
        control: comboLabel(trigger, family),
        action: eff.mappings.map((m) => mappingSummary(m, label)).join(' · '),
        note: eff.mappings
          .map((m) => m.description ?? '')
          .filter(Boolean)
          .join(' · '),
      },
    ];
  });
}

// ----- actions and mappings ------------------------------------------------------------------

const ARROWS: Record<string, string> = { up: '↑', down: '↓', left: '←', right: '→' };

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
  if ('pad' in a) return t('labels.pad', { name: a.pad }) + pressSuffix(a.action);
  if ('mouse' in a) return t('labels.mouse', { name: t(`labels.mouse_button.${a.mouse}`) }) + pressSuffix(a.action);
  if ('wheel' in a) return t('labels.wheel', { dir: ARROWS[a.wheel] ?? a.wheel });
  if ('stick' in a) {
    const dir = a.y > 0.5 ? '↑' : a.y < -0.5 ? '↓' : a.x < -0.5 ? '←' : a.x > 0.5 ? '→' : '•';
    return t(`labels.${a.stick}_stick`, { dir }) + pressSuffix(a.action);
  }
  if ('trigger' in a)
    return `${a.trigger === 'left' ? 'LT' : 'RT'}${a.value !== undefined && a.value < 1 ? ` ${Math.round(a.value * 100)}%` : ''}`;
  if ('mouse_move' in a) return t('labels.mouse_move', { dir: ARROWS[a.mouse_move] ?? a.mouse_move });
  if ('pause' in a) return `${a.pause} ms`;
  if ('rumble' in a) return t('labels.rumble', { ms: a.rumble });
  if ('shake' in a) return t('picker.shake_axis', { axis: a.shake.toUpperCase() });
  if ('turn' in a) return t('labels.turn', { deg: `${a.turn > 0 ? '+' : ''}${a.turn}` });
  if ('touch' in a) return t('labels.touch', { gesture: t(`picker.touch_out.${a.touch}`).toLowerCase() });
  if ('mouse_to' in a) return t('labels.mouse_to', { x: Math.round(a.mouse_to[0] * 100), y: Math.round(a.mouse_to[1] * 100) });
  if ('mouse_clip' in a) return t('labels.mouse_clip');
  if ('break' in a) return '⦀';
  if ('command' in a) {
    const c = a.command;
    if (typeof c === 'string' && has(`labels.commands.${c}`)) return t(`labels.commands.${c}`);
    if (typeof c === 'object' && 'launch' in c) return t('labels.launch', { name: c.launch.path.split(/[\\/]/).pop() ?? '' });
    if (typeof c === 'object' && 'crosshair' in c) return t('labels.crosshair', { n: c.crosshair });
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
  if (isMacroMode(mode)) s = `▶ ${s}`;
  if (m.jump) s = s ? `${s} · → ${layerLabel(m.jump.layer)}` : `→ ${layerLabel(m.jump.layer)}`;
  if (!s) s = '⊘';
  return s;
}
