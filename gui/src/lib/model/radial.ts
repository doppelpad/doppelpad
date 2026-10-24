// Radial menus: the ring geometry and edits of a profile's menus. The icon set sectors pick
// from is in components/radial/icons.ts.

import { t } from '../i18n/index.svelte';
import type { ControlName, Mapping, Profile, RadialMenu, RadialPosition, RadialSector } from '../types';
import { addLayer, editMappingIn, layerOf, modeOf, removeLayer, renameLayer, tidyMapping } from './profile';
import { routeLayer } from './routes';
import { dropUndefined, nonEmpty, setOrDrop } from './util';

/** A sector's colour until one is picked: the accent, as hex for colour pickers. */
export const DEFAULT_SECTOR_COLOR = '#14b8a6';

/** Outer radius of a menu at scale 1, in logical pixels; the hole is a third of it. */
export const RING = 200;

/** Point at `angle` (radians clockwise from the top) and radius `r`. */
function at(angle: number, r: number): [number, number] {
  return [Math.sin(angle) * r, -Math.cos(angle) * r];
}

/** SVG path of sector `k` of `n` between radii `inner` and `outer` (sector 0 at the top). */
export function sectorPath(k: number, n: number, inner: number, outer: number): string {
  const w = (2 * Math.PI) / n;
  const a0 = k * w - w / 2;
  const a1 = a0 + w;
  const large = w > Math.PI ? 1 : 0;
  if (n === 1) {
    return `M 0 ${-outer} A ${outer} ${outer} 0 1 1 0 ${outer} A ${outer} ${outer} 0 1 1 0 ${-outer} M 0 ${-inner} A ${inner} ${inner} 0 1 0 0 ${inner} A ${inner} ${inner} 0 1 0 0 ${-inner} Z`;
  }
  const [x0, y0] = at(a0, outer);
  const [x1, y1] = at(a1, outer);
  const [x2, y2] = at(a1, inner);
  const [x3, y3] = at(a0, inner);
  return `M ${x0} ${y0} A ${outer} ${outer} 0 ${large} 1 ${x1} ${y1} L ${x2} ${y2} A ${inner} ${inner} 0 ${large} 0 ${x3} ${y3} Z`;
}

/** Centre of sector `k` of `n` at radius `r`. */
export function sectorCentre(k: number, n: number, r: number): [number, number] {
  return at((k * 2 * Math.PI) / n, r);
}

/** Space between a ring at an edge (or kept inside the screen) and the screen's edge, in logical pixels. */
export const RING_MARGIN = 24;

/** The places a menu can sit, as rows of the 3 × 3 grid the editor shows (and `cursor`). */
export const POSITION_GRID: RadialPosition[][] = [
  ['top_left', 'top', 'top_right'],
  ['left', 'center', 'right'],
  ['bottom_left', 'bottom', 'bottom_right'],
];

/**
 * Where the centre of a ring of radius `r` goes on a screen of `size` (logical pixels): at
 * `position` plus `offset`. `cursor` is where the mouse pointer was as the menu opened; a ring
 * there stays inside the screen.
 */
export function ringCentre(
  position: RadialPosition,
  offset: [number, number],
  size: [number, number],
  r: number,
  cursor: [number, number] | null,
): [number, number] {
  const [w, h] = size;
  const near = RING_MARGIN + r;
  const keep = (v: number, max: number) => (max < 2 * near ? max / 2 : Math.min(Math.max(v, near), max - near));
  let x = w / 2;
  let y = h / 2;
  if (position === 'cursor') {
    if (cursor) [x, y] = [keep(cursor[0], w), keep(cursor[1], h)];
  } else {
    if (position.endsWith('left')) x = near;
    if (position.endsWith('right')) x = w - near;
    if (position.startsWith('top')) y = near;
    if (position.startsWith('bottom')) y = h - near;
  }
  return [x + offset[0], y + offset[1]];
}

export function updateMenu(p: Profile, i: number, patch: Partial<RadialMenu>): Profile {
  const all = [...(p.radial_menus ?? [])];
  all[i] = dropUndefined({ ...all[i], ...patch });
  return { ...p, radial_menus: all };
}

export function updateSector(p: Profile, i: number, k: number, patch: Partial<RadialSector>): Profile {
  const sectors = [...(p.radial_menus?.[i]?.sectors ?? [])];
  sectors[k] = dropUndefined({ ...sectors[k], ...patch });
  return updateMenu(p, i, { sectors });
}

/** A new menu with four sectors, opened by layer `layer`. */
export function addMenu(p: Profile, layer: number): Profile {
  const all = p.radial_menus ?? [];
  const sectors: RadialSector[] = [{ label: '1' }, { label: '2' }, { label: '3' }, { label: '4' }];
  return { ...p, radial_menus: [...all, { name: t('radial.default_name', { n: all.length + 1 }), layer, sectors }] };
}

/**
 * A new menu with four sectors and a layer of its own, named like the menu: the menu is open
 * while that layer is active. Returns the menu's index and its layer.
 */
export function createMenu(p: Profile): { profile: Profile; index: number; layer: number } {
  const { profile, id } = addLayer(p);
  const index = (p.radial_menus ?? []).length;
  const withMenu = addMenu(profile, id);
  const name = withMenu.radial_menus![index]!.name!;
  return { profile: renameLayer(withMenu, id, name), index, layer: id };
}

/** A new menu that only opens from sector `k` of menu `i`. Returns the new menu's index. */
export function createSubmenu(p: Profile, i: number, k: number): { profile: Profile; index: number } {
  const index = (p.radial_menus ?? []).length;
  return { profile: updateSector(addMenu(p, 0), i, k, { submenu: index + 1 }), index };
}

/** Menu `i` opens with a new layer of its own, named like the menu. */
export function giveOwnLayer(p: Profile, i: number): Profile {
  const { profile, id } = addLayer(p);
  const name = p.radial_menus?.[i]?.name || t('radial.default_name', { n: i + 1 });
  return updateMenu(renameLayer(profile, id, name), i, { layer: id });
}

/** The sectors (menu and sector index) that open menu `i` as their sub-menu. */
export function parentsOf(p: Profile, i: number): { menu: number; sector: number }[] {
  return (p.radial_menus ?? []).flatMap((m, j) => m.sectors.flatMap((s, k) => (s.submenu === i + 1 ? [{ menu: j, sector: k }] : [])));
}

/** Whether mapping `m` changes to layer `layer` from another layer. */
const opens = (m: Mapping, layer: number) => m.jump?.layer === layer && layerOf(m) !== layer;

/** The mappings that change to `layer` (so open its menu). */
export function openers(p: Profile, layer: number): Mapping[] {
  return layer === 0 ? [] : p.mappings.filter((m) => opens(m, layer));
}

/** `p` without what changes to `layer`: mappings that did nothing else go, the others lose their jump. */
export function removeOpeners(p: Profile, layer: number): Profile {
  const mappings = p.mappings
    .map((m) => (opens(m, layer) ? tidyMapping({ ...m, jump: undefined }) : m))
    .filter((m, i) => !opens(p.mappings[i]!, layer) || (m.actions ?? []).length > 0 || !!m.description || modeOf(m) !== 'hold');
  return { ...p, mappings };
}

/**
 * Open the menu of `layer` while `control` is held on Main: the control's mapping on Main
 * becomes just that (what it did before goes), and whatever opened the menu before stops.
 */
export function setOpener(p: Profile, layer: number, control: ControlName): Profile {
  return editMappingIn(removeOpeners(p, layer), 0, control, 'single', (m) => {
    m.actions = [];
    m.mode = undefined;
    m.jump = { layer, mode: 'hold' };
  });
}

/**
 * Remove menu `i`. Its layer goes too when it was only the menu's: no other menu opens with it
 * and nothing is mapped or routed on it. What opened it goes with the layer.
 */
export function deleteMenu(p: Profile, i: number): Profile {
  const layer = p.radial_menus?.[i]?.layer ?? 0;
  const out = removeMenu(p, i);
  const shared = (out.radial_menus ?? []).some((m) => m.layer === layer);
  const used = p.mappings.some((m) => layerOf(m) === layer) || (p.analog ?? []).some((r) => routeLayer(r) === layer);
  if (layer === 0 || shared || used) return out;
  return removeLayer(removeOpeners(out, layer), layer);
}

/** Remove menu `i`; sub-menu links to later menus keep pointing at the same menus. */
export function removeMenu(p: Profile, i: number): Profile {
  const n = i + 1;
  const radial_menus = (p.radial_menus ?? [])
    .filter((_, j) => j !== i)
    .map((m) => ({
      ...m,
      sectors: m.sectors.map((s) => {
        if (s.submenu === undefined) return s;
        if (s.submenu === n) {
          const { submenu: _, ...rest } = s;
          return rest;
        }
        return s.submenu > n ? { ...s, submenu: s.submenu - 1 } : s;
      }),
    }));
  return setOrDrop({ ...p, radial_menus }, 'radial_menus', nonEmpty(radial_menus));
}

/** The most sectors a menu has. */
export const MAX_SECTORS = 12;

/** Menu `i` with one more sector, labelled with its number (none past `MAX_SECTORS`). */
export function addSector(p: Profile, i: number): Profile {
  const m = p.radial_menus?.[i];
  if (!m || m.sectors.length >= MAX_SECTORS) return p;
  return updateMenu(p, i, { sectors: [...m.sectors, { label: String(m.sectors.length + 1) }] });
}

/** Menu `i` without sector `k`; a menu keeps at least one. */
export function removeSector(p: Profile, i: number, k: number): Profile {
  const m = p.radial_menus?.[i];
  if (!m || m.sectors.length <= 1) return p;
  return updateMenu(p, i, { sectors: m.sectors.filter((_, j) => j !== k) });
}
