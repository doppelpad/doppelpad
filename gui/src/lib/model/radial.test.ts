import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import { addMenu, isImageIcon, removeMenu, sectorCentre, sectorPath, updateSector } from './radial';

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

  it('tells image files from icon names', () => {
    expect(isImageIcon('sword')).toBe(false);
    expect(isImageIcon('C:\icons\potion.png')).toBe(true);
    expect(isImageIcon(undefined)).toBe(false);
  });
});
