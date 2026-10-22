import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import { layerEntrances, layerRadialMenus } from './layers';

const p: Profile = {
  name: 'T',
  mappings: [
    { layer: 2, trigger: 'X', activator: 'double', jump: { layer: 1, mode: 'switch', delay_ms: 200 } },
    { trigger: 'LB', jump: { layer: 1 } },
    { trigger: ['LB', 'RB'], activator: 'long', jump: { layer: 2, mode: 'toggle' } },
    { layer: 1, trigger: 'B', jump: { layer: 0, mode: 'switch' } },
    { layer: 1, trigger: 'Y', jump: { layer: 1 } },
    { trigger: 'A', actions: [{ key: 'Space' }] },
  ],
  radial_menus: [
    { name: 'Weapons', layer: 1, sectors: [] },
    { layer: 1, sectors: [] },
    { name: 'Sub', layer: 0, sectors: [] },
  ],
};

describe('layer entrances', () => {
  it('lists every mapping that changes to a layer, Main first', () => {
    expect(layerEntrances(p, 1)).toEqual([
      { from: 0, trigger: 'LB', activator: 'single', mode: 'hold', delayMs: 0 },
      { from: 2, trigger: 'X', activator: 'double', mode: 'switch', delayMs: 200 },
    ]);
    expect(layerEntrances(p, 2)).toEqual([{ from: 0, trigger: ['LB', 'RB'], activator: 'long', mode: 'toggle', delayMs: 0 }]);
    expect(layerEntrances(p, 0)).toEqual([{ from: 1, trigger: 'B', activator: 'single', mode: 'switch', delayMs: 0 }]);
    expect(layerEntrances(p, 3)).toEqual([]);
  });

  it('names the radial menus a layer opens', () => {
    expect(layerRadialMenus(p, 1)).toEqual(['Weapons', '2']);
    expect(layerRadialMenus(p, 0)).toEqual([]);
  });
});
