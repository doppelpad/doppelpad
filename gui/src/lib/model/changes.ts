// What differs between two versions of a config, in words: mappings added, removed or changed
// (by layer, control and activator), and the settings sections that changed.

import { activatorOf, comboKey, layerOf } from './profile';
import type { Activator, Combo, Mapping, Profile } from '../types';

export type MappingChange = { kind: 'added' | 'removed' | 'changed'; layer: number; trigger: Combo; activator: Activator };

export interface Changes {
  mappings: MappingChange[];
  /** Top-level settings that differ (`sticks`, `analog`, `layers`, …). */
  sections: string[];
}

const keyOf = (m: Mapping) => `${layerOf(m)}|${comboKey(m.trigger)}|${activatorOf(m)}`;
const json = (v: unknown) => JSON.stringify(v ?? null);

/** How `to` differs from `from`. */
export function profileChanges(from: Profile, to: Profile): Changes {
  const before = new Map(from.mappings.map((m) => [keyOf(m), m]));
  const after = new Map(to.mappings.map((m) => [keyOf(m), m]));
  const mappings: MappingChange[] = [];
  const entry = (kind: MappingChange['kind'], m: Mapping): MappingChange => ({
    kind,
    layer: layerOf(m),
    trigger: m.trigger,
    activator: activatorOf(m),
  });
  for (const [k, m] of after) {
    const old = before.get(k);
    if (!old) mappings.push(entry('added', m));
    else if (json(old) !== json(m)) mappings.push(entry('changed', m));
  }
  for (const [k, m] of before) if (!after.has(k)) mappings.push(entry('removed', m));
  mappings.sort((a, b) => a.layer - b.layer || comboKey(a.trigger).localeCompare(comboKey(b.trigger)));

  const keys = new Set([...Object.keys(from), ...Object.keys(to)].filter((k) => k !== 'mappings'));
  const sections = [...keys]
    .filter((k) => json((from as unknown as Record<string, unknown>)[k]) !== json((to as unknown as Record<string, unknown>)[k]))
    .sort();
  return { mappings, sections };
}

/** Whether nothing differs. */
export const isSame = (c: Changes) => c.mappings.length === 0 && c.sections.length === 0;
