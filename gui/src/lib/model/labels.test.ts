import { describe, expect, it } from 'vitest';
import { setLanguage } from '../i18n/index.svelte';
import type { Profile } from '../types';
import { comboLabel, configName, defaultLayerName, effectiveSummary, layerLabel, overlayRows } from './labels';

setLanguage('en');

const p: Profile = {
  name: 'Test',
  layers: [{ id: 2, name: 'Aim' }],
  mappings: [
    { trigger: 'A', actions: [{ key: 'Space' }], description: 'Jump' },
    { trigger: 'B', actions: [{ key: 'E' }] },
    { trigger: 'B', layer: 1, actions: [{ key: 'R' }] },
    { trigger: 'X', layer: 1, actions: [] },
  ],
};

describe('labels', () => {
  it('names combos and layers', () => {
    expect(comboLabel(['LB', 'A'], 'xbox')).toBe('A + Left bumper');
    expect(defaultLayerName(0)).toBe('Main');
    expect([layerLabel(p, 0), layerLabel(p, 1), layerLabel(p, 2)]).toEqual(['Main', 'Shift 1', 'Aim']);
  });

  it('sums up what a control does and where it comes from', () => {
    expect(effectiveSummary(p, 1, 'A', (id) => layerLabel(p, id))).toEqual({ text: 'Space', inherited: true });
    expect(effectiveSummary(p, 1, 'B', (id) => layerLabel(p, id))).toEqual({ text: 'R', inherited: false });
    expect(effectiveSummary(p, 0, 'Y', (id) => layerLabel(p, id))).toBeNull();
  });

  it('names configs by game, else by their own name or file', () => {
    const lib = {
      games: [
        { dir: 'C:/p/Halo', meta: { name: 'Halo', processes: [] }, configs: [{ path: 'C:/p/Halo/a.toml', name: 'Aim', error: null }] },
      ],
      unsorted: [{ path: 'C:/p/loose.toml', name: 'Loose', error: null }],
      errors: [],
    };
    expect(configName(lib, 'C:/p/Halo/a.toml')).toBe('Halo › Aim');
    expect(configName(lib, 'C:/p/loose.toml')).toBe('Loose');
    expect(configName(lib, 'C:\\x\\other.toml')).toBe('other.toml');
  });

  it("lists a layer's triggers first, then Main's it takes over", () => {
    expect(overlayRows(p, 1, 'xbox')).toEqual([
      { control: 'B', action: 'R', note: '' },
      { control: 'X', action: '⊘', note: '' },
      { control: 'A', action: 'Space', note: 'Jump' },
    ]);
  });
});
