// Finding mappings in a config: by what they output, by their control or by their note,
// across all layers. Labels are the ones the app shows, so the search speaks the UI language.

import { actionLabel, comboLabel, mappingSummary } from './labels';
import { activatorOf, comboList, layerOf } from './profile';
import type { Activator, Combo, PadFamily, Profile } from '../types';

/** Where to look: everything, or only what mappings output (their actions, also inside macros). */
export type SearchScope = 'all' | 'outputs';

export interface Found {
  /** Position in `profile.mappings`. */
  index: number;
  layer: number;
  trigger: Combo;
  activator: Activator;
  /** What the mapping does, as the editor sums it up. */
  summary: string;
  description?: string;
}

/** Case and accents do not matter. */
const fold = (s: string) => s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();

/** The view that shows a trigger: shortcuts for combos, the keyboard or mouse for theirs, else the controller (as it is shown). */
export function viewFor(trigger: Combo, current: string): 'front' | 'back' | 'keyboard' | 'mouse' | 'shortcuts' {
  const list = comboList(trigger);
  if (list.length > 1) return 'shortcuts';
  const c = list[0] ?? '';
  if (c.startsWith('Key:')) return 'keyboard';
  if (c.startsWith('Mouse:') || c.startsWith('Wheel:') || c.startsWith('MouseMove:')) return 'mouse';
  return current === 'back' ? 'back' : 'front';
}

/** A mapping as a search result. */
function result(p: Profile, index: number, layerLabel: (id: number) => string): Found {
  const m = p.mappings[index]!;
  return {
    index,
    layer: layerOf(m),
    trigger: m.trigger,
    activator: activatorOf(m),
    summary: mappingSummary(m, layerLabel),
    description: m.description,
  };
}

/** Mappings whose actions (also inside macros) press keyboard key `key` (`Space`, `W`), by layer. */
export function mappingsPressing(p: Profile, key: string, layerLabel: (id: number) => string): Found[] {
  return p.mappings
    .map((m, i) => ((m.actions ?? []).some((a) => 'key' in a && a.key === key) ? result(p, i, layerLabel) : null))
    .filter((f): f is Found => f !== null)
    .sort((a, b) => a.layer - b.layer || a.index - b.index);
}

/** Mappings matching the query (every word somewhere in the mapping's texts), by layer. */
export function searchMappings(
  p: Profile,
  query: string,
  family: PadFamily | null,
  layerLabel: (id: number) => string,
  scope: SearchScope = 'all',
): Found[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return [];
  const found: Found[] = [];
  p.mappings.forEach((m, index) => {
    const outputs = (m.actions ?? []).filter((a) => !('pause' in a)).map(actionLabel);
    const texts =
      scope === 'outputs'
        ? outputs
        : [comboLabel(m.trigger, family), ...comboList(m.trigger), ...outputs, m.jump ? layerLabel(m.jump.layer) : '', m.description ?? ''];
    const hay = fold(texts.join('\n'));
    if (words.every((w) => hay.includes(w))) found.push(result(p, index, layerLabel));
  });
  return found.sort((a, b) => a.layer - b.layer || a.index - b.index);
}
