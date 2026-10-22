// The label chips around a controller drawing: per drawn control its short name and what it
// does on a layer; a stick's chip also tells what its directions, zones and whole-stick route
// do, a trigger's chip what its zones do.

import type { LayoutControl } from '../../assets/devices/layouts';
import { t } from '../i18n/index.svelte';
import type { ControlName, PadFamily, Profile } from '../types';
import { layerRoutes, sourceOf, targetOf, type Side } from './analog';
import {
  DIAGONALS,
  DIRS8,
  SIDES,
  SIDE_NAMES,
  TOUCH_REGIONS,
  ZONES,
  controlShort,
  fullPullTag,
  gyroControls,
  withZone,
  zoneFamily,
  zoneParts,
  zoneTag,
} from './controls';
import { effectiveSummary } from './labels';
import { shortcutMembers } from './profile';

/** Arrows for directions and touchpad regions in chip summaries. */
const ARROW: Record<string, string> = {
  Up: '↑',
  Down: '↓',
  Left: '←',
  Right: '→',
  Center: '◎',
  UpLeft: '↖',
  UpRight: '↗',
  DownLeft: '↙',
  DownRight: '↘',
};

/** The side of a stick-click control (`LS` → left), else null. */
export const clickSide = (c: ControlName): Side | null => SIDE_NAMES.find((s) => SIDES[s].click === c) ?? null;

/** What the chips depend on besides the drawing. */
export interface ChipView {
  profile: Profile | null;
  layer: number;
  family: PadFamily | null;
  /** `comboKey` of the selected control, `''` for none. */
  selectedKey: string;
  /** The whole stick the side panel shows. */
  stick: Side | null;
  /** Controls held on the controller in use. */
  pressed: Set<ControlName>;
  layerLabel: (id: number) => string;
}

export interface Chip {
  lc: LayoutControl;
  label: string;
  summary: string;
  mapped: boolean;
  inherited: boolean;
  selected: boolean;
  pressed: boolean;
  combo: boolean;
}

/** Zone tag of a control ("H"), or "" for a control without a zone. */
function zoneSuffix(c: ControlName): string {
  const z = zoneParts(c)?.zone;
  return z ? zoneTag(z) : '';
}

/**
 * Controls a chip stands for besides its own, with a tag for its summary: a stick's
 * directions, their zones and its any-direction zones; a trigger's zones.
 */
function related(c: ControlName): { c: ControlName; tag: string }[] {
  const side = clickSide(c);
  if (side) {
    const stick = SIDES[side].stick;
    return [
      ...DIRS8.flatMap((d) => zoneFamily(stick + d).map((x) => ({ c: x, tag: ARROW[d] + zoneSuffix(x) }))),
      ...zoneFamily(stick).map((x) => ({ c: x, tag: '◎' + zoneSuffix(x) })),
    ];
  }
  if (c === 'Touchpad') return [...TOUCH_REGIONS, ...DIAGONALS].map((r) => ({ c: `Touch${r}`, tag: ARROW[r] }));
  if (zoneParts(c)) {
    return [...ZONES.map((z) => ({ c: withZone(c, z), tag: zoneTag(z) })), { c: `${c}Full`, tag: fullPullTag() }];
  }
  return [];
}

/** What the routes on `layer` make of each whole stick ("Mouse", "Flick stick"). */
function stickRoutes(p: Profile, layer: number): Map<Side, { text: string; inherited: boolean }> {
  const out = new Map<Side, { text: string; inherited: boolean }>();
  for (const ref of layerRoutes(p, layer)) {
    const s = sourceOf(ref.route);
    const side = SIDE_NAMES.find((x) => SIDES[x].source === s);
    if (side && !out.has(side)) out.set(side, { text: t(`analog.routes.targets.${targetOf(ref.route)}`), inherited: ref.inherited });
  }
  return out;
}

const summaryIn = (v: ChipView) => (c: ControlName) => v.profile && effectiveSummary(v.profile, v.layer, c, v.layerLabel);

/** A chip per drawn control. */
export function buildChips(controls: LayoutControl[], v: ChipView): Chip[] {
  const summaryOf = summaryIn(v);
  const routes = v.profile ? stickRoutes(v.profile, v.layer) : new Map<Side, { text: string; inherited: boolean }>();
  const combos = v.profile ? shortcutMembers(v.profile, v.layer) : new Set<string>();
  return controls.map((lc) => {
    const c = lc.control;
    const own = summaryOf(c);
    let summary = own?.text ?? '';
    let mapped = !!own;
    let selected = v.selectedKey === c;
    // The held controls are also what press-to-select reacts to, so highlight and selection agree.
    let isPressed = v.pressed.has(c);
    // Stick chips also show the directions and zones, trigger chips the zones.
    const parts: string[] = [];
    // A whole-stick route comes first: it is what the stick mostly does.
    const side = clickSide(c);
    const route = side ? routes.get(side) : undefined;
    if (route) parts.push(route.text);
    if (side && v.stick === side) selected = true;
    for (const r of related(c)) {
      const s = summaryOf(r.c);
      if (s) parts.push(`${r.tag} ${s.text}`);
      if (v.selectedKey === r.c) selected = true;
      if (v.pressed.has(r.c)) isPressed = true;
    }
    if (parts.length) {
      summary = [own ? `● ${own.text}` : '', ...parts].filter(Boolean).join('  ');
      mapped = true;
    }
    return {
      lc,
      // The touchpad chip stands for its click and its regions.
      label: c === 'Touchpad' ? t('views.groups.touchpad') : controlShort(c, v.family),
      summary,
      mapped,
      inherited: !!own?.inherited,
      selected,
      pressed: isPressed,
      combo: combos.has(c),
    };
  });
}

/** The gyro chip: what the gyro directions do, and whether one of them is selected. */
export function gyroChipOf(v: ChipView): { summary: string; selected: boolean } {
  const summaryOf = summaryIn(v);
  const parts: string[] = [];
  let selected = false;
  for (const c of gyroControls()) {
    const s = summaryOf(c);
    if (s) parts.push(`${ARROW[c.slice(4)]} ${s.text}`);
    if (v.selectedKey === c) selected = true;
  }
  return { summary: parts.join('  '), selected };
}
