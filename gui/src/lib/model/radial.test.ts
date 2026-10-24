import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import {
  MAX_SECTORS,
  RING_MARGIN,
  addMenu,
  addSector,
  createMenu,
  deleteMenu,
  openers,
  removeMenu,
  removeSector,
  ringCentre,
  sectorCentre,
  sectorPath,
  setOpener,
  updateSector,
} from './radial';

describe('radial menus', () => {
  it('puts sector 0 at the top and goes clockwise', () => {
    const [x0, y0] = sectorCentre(0, 4, 100);
    const [x1, y1] = sectorCentre(1, 4, 100);
    expect([Math.round(x0) + 0, Math.round(y0)]).toEqual([0, -100]);
    expect([Math.round(x1), Math.round(y1) + 0]).toEqual([100, 0]);
    expect(sectorPath(0, 4, 50, 100)).toMatch(/^M -70\.\d+ -70\.\d+ A 100 100 0 0 1 70\.\d+ -70\.\d+/);
  });

  it('keeps sub-menu links when a menu goes', () => {
    let p: Profile = { name: 't', mappings: [] };
    p = addMenu(addMenu(addMenu(p, 1), 0), 0);
    p = updateSector(p, 0, 0, { submenu: 3 });
    p = updateSector(p, 0, 1, { submenu: 2 });
    p = removeMenu(p, 1);
    expect(p.radial_menus?.length).toBe(2);
    expect(p.radial_menus?.[0].sectors[0].submenu).toBe(2);
    expect(p.radial_menus?.[0].sectors[1].submenu).toBeUndefined();
  });
});

describe('radial sectors and layers', () => {
  it('adds numbered sectors up to the maximum and keeps at least one', () => {
    let p: Profile = addMenu({ name: 't', mappings: [] }, 1);
    p = addSector(p, 0);
    expect(p.radial_menus?.[0].sectors.at(-1)).toEqual({ label: '5' });
    while ((p.radial_menus?.[0].sectors.length ?? 0) < MAX_SECTORS) p = addSector(p, 0);
    expect(addSector(p, 0)).toBe(p);
    p = removeSector(p, 0, 0);
    expect(p.radial_menus?.[0].sectors[0].label).toBe('2');
    let one: Profile = addMenu({ name: 't', mappings: [] }, 1);
    for (let k = 0; k < 3; k++) one = removeSector(one, 0, 0);
    expect(removeSector(one, 0, 0)).toBe(one);
  });
});

describe('setting a menu up', () => {
  const base: Profile = { name: 't', layers: [{ id: 1 }], mappings: [{ trigger: 'LB', actions: [{ key: 'Q' }] }] };

  it('gives a new menu a layer of its own', () => {
    const { profile, index, layer } = createMenu(base);
    expect([index, layer]).toEqual([0, 2]);
    expect(profile.radial_menus?.[0]).toMatchObject({ layer: 2 });
    expect(profile.layers?.find((l) => l.id === 2)?.name).toBe(profile.radial_menus?.[0].name);
  });

  it('opens with one button held on Main', () => {
    const { profile, layer } = createMenu(base);
    let p = setOpener(profile, layer, 'LB');
    expect(openers(p, layer)).toEqual([{ trigger: 'LB', jump: { layer, mode: 'hold' } }]);
    // Another button takes over; the first one did nothing else, so it goes.
    p = setOpener(p, layer, 'RB');
    expect(openers(p, layer).map((m) => m.trigger)).toEqual(['RB']);
    expect(p.mappings.some((m) => m.trigger === 'LB')).toBe(false);
  });

  it('takes its layer and openers along when it goes, unless something else uses the layer', () => {
    const { profile, layer } = createMenu(base);
    const p = setOpener(profile, layer, 'RB');
    const gone = deleteMenu(p, 0);
    expect(gone.radial_menus).toBeUndefined();
    expect(gone.layers?.map((l) => l.id)).toEqual([1]);
    expect(gone.mappings).toEqual(base.mappings);
    const used = { ...p, mappings: [...p.mappings, { layer, trigger: 'A', actions: [{ key: 'E' }] }] };
    expect(deleteMenu(used, 0).layers?.map((l) => l.id)).toEqual([1, layer]);
  });
});

describe('where the ring sits', () => {
  const screen: [number, number] = [1000, 800];
  const near = RING_MARGIN + 100;

  it('goes to the middle, an edge or a corner, plus the offset', () => {
    expect(ringCentre('center', [0, 0], screen, 100, null)).toEqual([500, 400]);
    expect(ringCentre('top', [0, 10], screen, 100, null)).toEqual([500, near + 10]);
    expect(ringCentre('bottom_right', [0, 0], screen, 100, null)).toEqual([1000 - near, 800 - near]);
    expect(ringCentre('left', [-5, 0], screen, 100, null)).toEqual([near - 5, 400]);
  });

  it('follows the mouse pointer but stays on the screen', () => {
    expect(ringCentre('cursor', [0, 0], screen, 100, [300, 300])).toEqual([300, 300]);
    expect(ringCentre('cursor', [0, 0], screen, 100, [10, 790])).toEqual([near, 800 - near]);
    expect(ringCentre('cursor', [0, 0], screen, 100, null)).toEqual([500, 400]);
  });
});
