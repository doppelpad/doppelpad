import { describe, expect, it } from 'vitest';
import type { Mapping, Profile } from '../types';
import {
  addLayer,
  comboKey,
  effectiveMappings,
  findMapping,
  layerIds,
  mappingSummary,
  pasteLayer,
  removeLayer,
  removeMapping,
  shortcutMembers,
  shortcuts,
  tidyMapping,
  toCombo,
  upsertMapping,
} from './profile';

const base = (): Profile => ({
  name: 'Test',
  mappings: [
    { trigger: 'A', actions: [{ key: 'Space' }] },
    { trigger: 'B', activator: 'long', actions: [{ key: 'Escape' }] },
    { trigger: 'LB', jump: { layer: 1 } },
    { layer: 1, trigger: 'A', actions: [{ key: 'E' }] },
    { trigger: ['LB', 'RB'], actions: [{ key: 'Tab' }] },
  ],
});

const lbl = (id: number) => (id === 0 ? 'Main' : `Shift ${id}`);

describe('combos', () => {
  it('sort and collapse', () => {
    expect(toCombo(['RB', 'LB'])).toEqual(['LB', 'RB']);
    expect(toCombo(['A', 'A'])).toBe('A');
    expect(comboKey(['RB', 'LB'])).toBe('LB+RB');
  });
});

describe('mapping edits', () => {
  it('finds by layer, trigger and activator', () => {
    const p = base();
    expect(findMapping(p, 0, 'B', 'long')?.actions).toEqual([{ key: 'Escape' }]);
    expect(findMapping(p, 0, 'B', 'single')).toBeUndefined();
    expect(findMapping(p, 0, ['RB', 'LB'], 'single')?.actions).toEqual([{ key: 'Tab' }]);
  });

  it('upsert replaces the same slot and appends new ones', () => {
    let p = upsertMapping(base(), { trigger: 'A', actions: [{ key: 'Enter' }] });
    expect(p.mappings.filter((m) => m.trigger === 'A' && !m.layer)).toHaveLength(1);
    expect(findMapping(p, 0, 'A', 'single')?.actions).toEqual([{ key: 'Enter' }]);
    p = upsertMapping(p, { trigger: 'A', activator: 'double', actions: [{ key: 'R' }] });
    expect(p.mappings).toHaveLength(6);
  });

  it('remove only touches one activator', () => {
    const p = removeMapping(base(), 0, 'B', 'long');
    expect(p.mappings).toHaveLength(4);
    expect(findMapping(p, 0, 'A', 'single')).toBeDefined();
  });

  it('tidy drops defaults', () => {
    const m: Mapping = { layer: 0, trigger: 'A', activator: 'single', mode: 'hold', actions: [], description: '' };
    expect(tidyMapping(m)).toEqual({ trigger: 'A' });
  });
});

describe('layers', () => {
  it('collects ids from layers, mappings and jumps', () => {
    expect(layerIds(base())).toEqual([0, 1]);
  });

  it('inherits base mappings unless overridden or disabled', () => {
    const p = base();
    expect(effectiveMappings(p, 1, 'A')).toMatchObject({ inherited: false });
    expect(effectiveMappings(p, 1, 'B')).toMatchObject({ inherited: true });
    const noInherit: Profile = { ...p, layers: [{ id: 1, inherit: false }] };
    expect(effectiveMappings(noInherit, 1, 'B').mappings).toHaveLength(0);
    const notB: Profile = { ...p, layers: [{ id: 1, not_inherited: ['B'] }] };
    expect(effectiveMappings(notB, 1, 'B').mappings).toHaveLength(0);
  });

  it('adds, removes and pastes layers', () => {
    const { profile, id } = addLayer(base());
    expect(id).toBe(2);
    const removed = removeLayer(base(), 1);
    expect(layerIds(removed)).toEqual([0]);
    expect(removed.mappings.find((m) => m.trigger === 'LB')).toEqual({ trigger: 'LB' });
    const copied = base().mappings.filter((m) => !m.layer);
    const pasted = pasteLayer(profile, copied, 2);
    expect(pasted.mappings.filter((m) => m.layer === 2)).toHaveLength(4);
  });
});

describe('shortcuts and labels', () => {
  it('lists multi-control triggers', () => {
    expect(shortcuts(base(), 0)).toHaveLength(1);
    expect([...shortcutMembers(base(), 0)].sort()).toEqual(['LB', 'RB']);
  });

  it('summarises mappings', () => {
    expect(mappingSummary({ trigger: 'A', actions: [{ key: 'LeftCtrl' }, { key: 'C' }] }, lbl)).toBe('LeftCtrl + C');
    expect(mappingSummary({ trigger: 'A', mode: 'turbo', actions: [{ mouse: 'left' }] }, lbl)).toBe('⚡ Mouse left');
    expect(mappingSummary({ trigger: 'LB', jump: { layer: 1 } }, lbl)).toBe('→ Shift 1');
    expect(mappingSummary({ trigger: 'X' }, lbl)).toBe('⊘');
  });
});
