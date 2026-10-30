// What the app adds to the validator's diagnostics, and where they point: the controls and
// layers that get a warning mark, and the mapping a diagnostic leads to.

import { t } from '../i18n/index.svelte';
import { HOTKEY_FIELDS, hotkeyLabel } from './hotkeys';
import { activatorOf, jumpsTo, layerOf } from './mapping';
import { comboKey, comboList, effectiveMappings } from './profile';
import type { Place } from './views';
import type { Combo, DiagMsg, Hotkeys, PadFamily, Profile } from '../types';

/** A mapping's diagnostic path, as the validator writes it: `mappings[3] (LB+RB)`. */
export const mappingPath = (i: number, trigger: Combo) => `mappings[${i}] (${comboList(trigger).join('+')})`;

/**
 * Mappings whose trigger holds every button of a controller hotkey: once those are down, the
 * hotkey takes them, so the mapping never fires while hotkeys are on. The buttons are named as
 * `family` names them, like the hotkey settings do.
 */
export function hotkeyClashes(p: Profile, hk: Hotkeys | null, family: PadFamily | null): DiagMsg[] {
  if (!hk) return [];
  const out: DiagMsg[] = [];
  p.mappings.forEach((m, i) => {
    const members = new Set(comboList(m.trigger));
    for (const field of HOTKEY_FIELDS) {
      const buttons = hk[field];
      if (buttons.length > 0 && buttons.every((b) => members.has(b))) {
        out.push({
          severity: 'warning',
          path: mappingPath(i, m.trigger),
          message: t('lints.hotkey', { hotkey: t(`hotkeys.${field}`), keys: hotkeyLabel(buttons, family) }),
        });
      }
    }
  });
  return out;
}

/** Where a diagnostic points: a mapping, a layer, or a radial menu (and a sector of it). */
export function diagTarget(p: Profile, d: DiagMsg): Place | null {
  const m = /^mappings\[(\d+)\]/.exec(d.path);
  if (m) {
    const mapping = p.mappings[Number(m[1])];
    return mapping ? { kind: 'mapping', layer: layerOf(mapping), trigger: mapping.trigger, activator: activatorOf(mapping) } : null;
  }
  const r = /^radial_menus\[(\d+)\](?:\.sectors\[(\d+)\])?/.exec(d.path);
  if (r) {
    const menu = Number(r[1]);
    return p.radial_menus?.[menu] ? { kind: 'radial', menu, sector: r[2] === undefined ? null : Number(r[2]) } : null;
  }
  const l = /^layers\[(\d+)\]$/.exec(d.path);
  const layer = l ? p.layers?.[Number(l[1])] : undefined;
  return layer ? { kind: 'layer', layer: layer.id } : null;
}

/** Whether a mapping on another layer changes to layer `id` (Main is always there). */
export function isReached(p: Profile, id: number): boolean {
  return id === 0 || p.mappings.some((m) => jumpsTo(m, id));
}

/**
 * Whether a diagnostic waits until the config is saved: the warning that nothing changes to a
 * layer (the only warning about a whole layer, `layers[i]`) for a layer that is not in the
 * saved config (`saved`). A layer just added has nothing changing to it yet, so it would warn
 * at once while it is being set up.
 */
export function waitsForSave(p: Profile, d: DiagMsg, saved: ReadonlySet<number>): boolean {
  if (d.severity !== 'warning') return false;
  const target = diagTarget(p, d);
  return target?.kind === 'layer' && !saved.has(target.layer);
}

/** What gets a warning mark: per layer the `comboKey`s of flagged triggers, and flagged layers. */
export interface Flags {
  triggers: Map<number, Set<string>>;
  layers: Set<number>;
}

/** The marks for the warnings and errors (not the notes) among the diagnostics. */
export function flagsOf(p: Profile | null, diags: DiagMsg[]): Flags {
  const flags: Flags = { triggers: new Map(), layers: new Set() };
  if (!p) return flags;
  for (const d of diags) {
    if (d.severity === 'info') continue;
    const target = diagTarget(p, d);
    if (target?.kind === 'layer') flags.layers.add(target.layer);
    if (target?.kind === 'mapping') {
      const set = flags.triggers.get(target.layer) ?? new Set<string>();
      set.add(comboKey(target.trigger));
      flags.triggers.set(target.layer, set);
    }
  }
  return flags;
}

/** Whether a trigger has a mark on a layer: its own mappings there, or Main's it takes over. */
export function isFlagged(f: Flags, p: Profile | null, layer: number, trigger: Combo): boolean {
  if (!p || f.triggers.size === 0) return false;
  const from = layer !== 0 && effectiveMappings(p, layer, trigger).inherited ? 0 : layer;
  return f.triggers.get(from)?.has(comboKey(trigger)) ?? false;
}
