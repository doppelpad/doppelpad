import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import { isSame, profileChanges } from './changes';

const base: Profile = {
  name: 'T',
  mappings: [
    { trigger: 'A', actions: [{ key: 'Space' }] },
    { trigger: 'B', actions: [{ key: 'E' }] },
    { layer: 1, trigger: 'X', activator: 'long', actions: [{ key: 'R' }] },
  ],
  sticks: { left: { deadzone: 0.1 } },
};

describe('changes between versions', () => {
  it('lists mappings added, removed and changed, and the settings that differ', () => {
    const next: Profile = {
      ...base,
      name: 'T2',
      mappings: [
        { trigger: 'A', actions: [{ key: 'Space' }] },
        { trigger: 'B', actions: [{ key: 'F' }] },
        { trigger: 'Y', actions: [{ key: 'Q' }] },
      ],
      sticks: { left: { deadzone: 0.2 } },
    };
    const c = profileChanges(base, next);
    expect(c.mappings).toEqual([
      { kind: 'changed', layer: 0, trigger: 'B', activator: 'single' },
      { kind: 'added', layer: 0, trigger: 'Y', activator: 'single' },
      { kind: 'removed', layer: 1, trigger: 'X', activator: 'long' },
    ]);
    expect(c.sections).toEqual(['name', 'sticks']);
  });

  it('the same config has no changes, whatever the order of its mappings', () => {
    expect(isSame(profileChanges(base, { ...base, mappings: [...base.mappings].reverse() }))).toBe(true);
  });
});
