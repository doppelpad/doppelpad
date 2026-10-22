import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import { layerEntrances, layerGraph, layerRadialMenus } from './layers';

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

describe('layer map', () => {
  it('places Main at the top and draws an arrow per layer change', () => {
    const { nodes, edges } = layerGraph(p, [0, 1, 2]);
    expect(nodes[0]).toMatchObject({ id: 0, x: expect.closeTo(0, 6), y: expect.closeTo(-1, 6) });
    expect(nodes.map((n) => n.id)).toEqual([0, 1, 2]);
    expect(edges.map((e) => [e.from, e.to, e.index, e.mode])).toEqual([
      [2, 1, 0, 'switch'],
      [0, 1, 1, 'hold'],
      [0, 2, 2, 'toggle'],
      [1, 0, 3, 'switch'],
    ]);
    // The two arrows between Main and layer 1 bend to opposite sides: each bends relative to its
    // own direction, so the same sign puts them on different sides of the line between the layers.
    const [main1, back] = [edges[1]!, edges[3]!];
    expect(main1.bend).not.toBe(0);
    expect(Math.sign(main1.bend)).toBe(Math.sign(back.bend));
  });

  it('a single layer sits in the middle', () => {
    expect(layerGraph({ name: 'T', mappings: [] }, [0]).nodes).toEqual([{ id: 0, x: 0, y: 0 }]);
  });
});
