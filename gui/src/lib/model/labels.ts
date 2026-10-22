// Texts for controls, layers and mappings as the views, the overlays and the bindings sheet
// show them.

import { t } from '../i18n/index.svelte';
import type { Combo, PadFamily, Profile, Template } from '../types';
import { controlLabel } from './controls';
import { comboKey, comboList, effectiveMappings, layerName, layerOf, mappingSummary } from './profile';

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
