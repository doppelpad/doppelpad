import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import { SELF, insertPreset, makePreset } from './presets';

const source: Profile = {
  name: 'Racing <1>',
  layers: [{ id: 2, name: 'Drive', inherit: false }],
  mappings: [
    { trigger: 'A', actions: [{ key: 'Space' }] },
    { trigger: 'B', layer: 2, actions: [{ key: 'W' }], description: 'gas' },
    { trigger: 'Back', layer: 2, actions: [], jump: { layer: 2, mode: 'toggle' } },
    { trigger: 'Start', layer: 2, actions: [], jump: { layer: 0 } },
    { trigger: 'Y', layer: 2, actions: [], jump: { layer: 3 } },
  ],
  analog: [{ layer: 2, source: 'right_stick', target: 'mouse' }],
};

describe('layer presets', () => {
  it('keep a layer with its settings and own jumps marked', () => {
    const p = makePreset(source, 2, 'Drive');
    expect(p.layer).toEqual({ id: 0, name: 'Drive', inherit: false });
    expect(p.mappings?.map((m) => m.layer)).toEqual([undefined, undefined, undefined, undefined]);
    expect(p.mappings?.[1].jump?.layer).toBe(SELF);
    expect(p.mappings?.[2].jump?.layer).toBe(0);
    expect(p.analog).toEqual([{ source: 'right_stick', target: 'mouse' }]);
  });

  it('insert as a new layer, dropping jumps that lead nowhere', () => {
    const target: Profile = { mappings: [{ trigger: 'A', actions: [{ key: 'E' }] }] };
    const { profile, layer, dropped } = insertPreset(target, makePreset(source, 2, 'Drive'));
    expect(layer).toBe(1);
    expect(dropped).toBe(1);
    expect(profile.layers).toEqual([{ id: 1, name: 'Drive', inherit: false }]);
    const moved = profile.mappings.filter((m) => m.layer === 1);
    expect(moved).toHaveLength(4);
    expect(moved.find((m) => m.trigger === 'Back')?.jump?.layer).toBe(1);
    expect(moved.find((m) => m.trigger === 'Y')?.jump).toBeUndefined();
    expect(profile.analog).toEqual([{ layer: 1, source: 'right_stick', target: 'mouse' }]);
    expect(profile.mappings[0]).toEqual({ trigger: 'A', actions: [{ key: 'E' }] });
  });

  it('a preset of Main has no layer settings', () => {
    expect(makePreset(source, 0, 'Base').layer).toBeUndefined();
  });
});
