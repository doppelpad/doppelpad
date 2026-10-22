import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import { CROSSHAIR_SHAPES, addCrosshair, crosshairConfig, crosshairParts, removeCrosshair, updateCrosshair } from './crosshair';
import { defaults } from './fixtures';

describe('crosshairs', () => {
  it('draws every shape inside its size', () => {
    for (const shape of CROSSHAIR_SHAPES) {
      const parts = crosshairParts(shape, 40, 3);
      expect(parts.length, shape).toBeGreaterThan(0);
      for (const p of parts) {
        if ('points' in p) for (const [x, y] of p.points) expect(Math.max(Math.abs(x), Math.abs(y))).toBeLessThanOrEqual(20);
        else expect(p.r).toBeLessThanOrEqual(20);
      }
    }
  });

  it('removing one keeps commands on the crosshairs that stay', () => {
    let p: Profile = { name: 't', mappings: [] };
    p = addCrosshair(addCrosshair(addCrosshair(p)));
    p.mappings = [{ trigger: 'A', actions: [{ command: { crosshair: 1 } }, { command: { crosshair: 2 } }, { command: { crosshair: 3 } }] }];
    p = removeCrosshair(p, 1);
    expect(p.crosshairs?.map((c) => c.name)).toEqual(['Crosshair 1', 'Crosshair 3']);
    expect(p.mappings[0].actions).toEqual([{ command: { crosshair: 1 } }, { command: { crosshair: 2 } }]);
  });

  it('drops fields reset to the default', () => {
    let p = addCrosshair({ name: 't', mappings: [] });
    p = updateCrosshair(p, 0, { size: 50 });
    p = updateCrosshair(p, 0, { size: undefined, shape: 'dot' });
    expect(p.crosshairs).toEqual([{ name: 'Crosshair 1', shape: 'dot' }]);
  });

  it('a crosshair without layers shows on none of them', () => {
    // The catalog's default leaves `layers` out, like every crosshair the editor adds.
    const def = defaults.crosshair;
    expect(def.layers).toBeUndefined();
    const p = addCrosshair({ name: 't', mappings: [] });
    const f = crosshairConfig(p.crosshairs![0], def);
    expect(f.layers).toEqual([]);
    expect(f.size).toBe(def.size);
    expect(crosshairConfig({ layers: [2] }, def).layers).toEqual([2]);
  });
});
