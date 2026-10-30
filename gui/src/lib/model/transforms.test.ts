import { describe, expect, it } from 'vitest';
import type { Mapping, Profile } from '../types';
import { defaults } from './fixtures';
import { transform } from './transforms';

const def = defaults.stick;

const on = (p: Profile, trigger: string, layer = 0): Mapping[] =>
  p.mappings.filter((m) => m.trigger === trigger && (m.layer ?? 0) === layer).map(({ layer: _l, trigger: _t, ...rest }) => rest as Mapping);

describe('face buttons', () => {
  it('controls nothing maps take their own output along', () => {
    const p: Profile = { name: 'T', mappings: [] };
    const out = transform(p, 'face', 0, def);
    expect(on(out, 'A')).toEqual([{ actions: [{ pad: 'B' }] }]);
    expect(on(out, 'B')).toEqual([{ actions: [{ pad: 'A' }] }]);
    expect(on(out, 'X')).toEqual([{ actions: [{ pad: 'Y' }] }]);
    expect(on(out, 'Y')).toEqual([{ actions: [{ pad: 'X' }] }]);
  });

  it('twice is the same as never', () => {
    const p: Profile = { name: 'T', mappings: [{ trigger: 'A', activator: 'long', actions: [{ key: 'Space' }] }] };
    const twice = transform(transform(p, 'face', 0, def), 'face', 0, def);
    expect(twice.mappings).toEqual(p.mappings);
  });

  it('mappings move with every activator; the partner gets the moved control’s output', () => {
    const p: Profile = {
      name: 'T',
      mappings: [
        { trigger: 'A', actions: [{ key: 'Space' }] },
        { trigger: 'A', activator: 'double', actions: [{ key: 'E' }] },
      ],
    };
    const out = transform(p, 'face', 0, def);
    expect(on(out, 'B')).toEqual([{ actions: [{ key: 'Space' }] }, { activator: 'double', actions: [{ key: 'E' }] }]);
    expect(on(out, 'A')).toEqual([{ actions: [{ pad: 'B' }] }]);
  });

  it('without passthrough, unmapped controls stay unmapped', () => {
    const p: Profile = { name: 'T', output: { passthrough: false }, mappings: [{ trigger: 'A', actions: [{ key: 'Space' }] }] };
    const out = transform(p, 'face', 0, def);
    expect(on(out, 'B')).toEqual([{ actions: [{ key: 'Space' }] }]);
    expect(on(out, 'A')).toEqual([]);
  });
});

describe('layers', () => {
  it('one shift layer takes what it inherits over, Main stays', () => {
    const p: Profile = { name: 'T', mappings: [{ trigger: 'A', actions: [{ key: 'Space' }] }] };
    const out = transform(p, 'face', 1, def);
    expect(on(out, 'A')).toEqual([{ actions: [{ key: 'Space' }] }]);
    expect(on(out, 'B', 1)).toEqual([{ actions: [{ key: 'Space' }] }]);
    expect(on(out, 'A', 1)).toEqual([{ actions: [{ pad: 'B' }] }]);
  });

  it('a control that does nothing on the layer now stops taking Main’s mapping', () => {
    const p: Profile = { name: 'T', mappings: [{ trigger: 'LeftStickUp', actions: [{ key: 'W' }] }] };
    const out = transform(p, 'sticks', 1, def);
    expect(on(out, 'RightStickUp', 1)).toEqual([{ actions: [{ key: 'W' }] }]);
    expect(out.layers?.find((l) => l.id === 1)?.not_inherited).toEqual(['LeftStickUp']);
  });

  it('the whole config: inheriting layers read Main as it was', () => {
    const p: Profile = {
      name: 'T',
      mappings: [
        { trigger: 'A', actions: [{ key: '1' }] },
        { trigger: 'B', actions: [{ key: '2' }] },
        { layer: 1, trigger: 'A', actions: [{ key: '3' }] },
        { trigger: 'LB', jump: { layer: 1 } },
      ],
    };
    const out = transform(p, 'face', 'all', def);
    expect(on(out, 'A')).toEqual([{ actions: [{ key: '2' }] }]);
    expect(on(out, 'B')).toEqual([{ actions: [{ key: '1' }] }]);
    // On layer 1, B now does what A did there, and A what B did there (Main's 2).
    expect(on(out, 'B', 1)).toEqual([{ actions: [{ key: '3' }] }]);
    expect(on(out, 'A', 1)).toEqual([{ actions: [{ key: '2' }] }]);
  });
});

describe('sticks', () => {
  const p: Profile = {
    name: 'T',
    sticks: { left: { deadzone: 0.12 } },
    mappings: [{ trigger: 'LeftStickUp', actions: [{ key: 'W' }] }],
    analog: [{ source: 'right_stick', target: 'mouse', sensitivity: 2 }],
  };

  it('directions and routes trade places; a stick that passed through feeds the other virtual stick', () => {
    const out = transform(p, 'sticks', 0, def);
    expect(on(out, 'RightStickUp')).toEqual([{ actions: [{ key: 'W' }] }]);
    expect(on(out, 'LeftStickUp')).toEqual([]);
    expect(out.analog).toEqual([
      { source: 'left_stick', target: 'mouse', sensitivity: 2 },
      { source: 'right_stick', target: 'left_stick', sensitivity: 1, curve: 1, deadzone: 0.12, outer: 1, anti_deadzone: 0, consume: true },
    ]);
  });

  it('a stick that passed through is shaped by the settings it has on the layer', () => {
    const q: Profile = { ...p, layers: [{ id: 1, sticks: { left: { deadzone: 0.3, sensitivity: 1.5 } } }] };
    const out = transform(q, 'sticks', 1, def);
    expect(out.analog?.find((r) => r.layer === 1 && r.target === 'left_stick')).toEqual({
      layer: 1,
      source: 'right_stick',
      target: 'left_stick',
      sensitivity: 1.5,
      curve: 1,
      deadzone: 0.3,
      outer: 1,
      anti_deadzone: 0,
      consume: true,
    });
  });

  it('twice brings the sticks back', () => {
    const twice = transform(transform(p, 'sticks', 0, def), 'sticks', 0, def);
    expect(twice.mappings).toEqual(p.mappings);
    expect(twice.analog).toEqual(p.analog);
  });

  it('mirror swaps sides: bumpers, triggers with their zones, D-pad with the face buttons', () => {
    const q: Profile = {
      name: 'T',
      mappings: [
        { trigger: 'LB', actions: [{ key: 'Q' }] },
        { trigger: 'LTHigh', actions: [{ key: 'R' }] },
        { trigger: 'DpadUp', actions: [{ key: '1' }] },
      ],
    };
    const out = transform(q, 'mirror', 0, def);
    expect(on(out, 'RB')).toEqual([{ actions: [{ key: 'Q' }] }]);
    expect(on(out, 'RTHigh')).toEqual([{ actions: [{ key: 'R' }] }]);
    expect(on(out, 'Y')).toEqual([{ actions: [{ key: '1' }] }]);
    expect(on(out, 'DpadUp')).toEqual([{ actions: [{ pad: 'Y' }] }]);
    expect(on(out, 'LB')).toEqual([{ actions: [{ pad: 'RB' }] }]);
  });
});
