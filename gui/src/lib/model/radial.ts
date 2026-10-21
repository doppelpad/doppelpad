// Radial menus: the icon set sectors pick from, the ring geometry, and edits of a profile's
// menus.

import {
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Axe,
  Backpack,
  Bomb,
  Camera,
  Car,
  Check,
  Circle,
  Coins,
  Compass,
  Crosshair,
  Eye,
  Flag,
  Flame,
  Footprints,
  Gamepad2,
  Gem,
  Ghost,
  Hammer,
  Hand,
  Heart,
  HeartPulse,
  House,
  Key,
  Keyboard,
  Lock,
  Map as MapIcon,
  MessageCircle,
  Mic,
  Mouse,
  Package,
  Pickaxe,
  Pill,
  Plane,
  Repeat,
  Rocket,
  RotateCcw,
  Settings,
  Shield,
  ShieldPlus,
  ShoppingCart,
  Skull,
  Sparkles,
  Square,
  Star,
  Sword,
  Swords,
  Syringe,
  Target,
  Triangle,
  Trophy,
  User,
  Users,
  Volume2,
  WandSparkles,
  Wrench,
  X,
  Zap,
} from '@lucide/svelte';
import type { Component } from 'svelte';
import { t } from '../i18n/index.svelte';
import type { Profile, RadialMenu, RadialSector } from '../types';

/** Icons a sector can show, by the name a profile stores. */
export const RADIAL_ICONS: Record<string, Component> = {
  sword: Sword,
  swords: Swords,
  shield: Shield,
  'shield-plus': ShieldPlus,
  crosshair: Crosshair,
  target: Target,
  bomb: Bomb,
  flame: Flame,
  zap: Zap,
  heart: Heart,
  'heart-pulse': HeartPulse,
  pill: Pill,
  syringe: Syringe,
  map: MapIcon,
  compass: Compass,
  backpack: Backpack,
  package: Package,
  car: Car,
  plane: Plane,
  rocket: Rocket,
  footprints: Footprints,
  eye: Eye,
  camera: Camera,
  mic: Mic,
  'message-circle': MessageCircle,
  flag: Flag,
  star: Star,
  trophy: Trophy,
  key: Key,
  lock: Lock,
  wrench: Wrench,
  hammer: Hammer,
  pickaxe: Pickaxe,
  axe: Axe,
  'wand-sparkles': WandSparkles,
  sparkles: Sparkles,
  skull: Skull,
  ghost: Ghost,
  gem: Gem,
  coins: Coins,
  'shopping-cart': ShoppingCart,
  'volume-2': Volume2,
  settings: Settings,
  house: House,
  hand: Hand,
  'gamepad-2': Gamepad2,
  keyboard: Keyboard,
  mouse: Mouse,
  users: Users,
  user: User,
  'arrow-up': ArrowUp,
  'arrow-down': ArrowDown,
  'arrow-left': ArrowLeft,
  'arrow-right': ArrowRight,
  'rotate-ccw': RotateCcw,
  repeat: Repeat,
  x: X,
  check: Check,
  circle: Circle,
  square: Square,
  triangle: Triangle,
};

/** An icon that is a file rather than one of the set. */
export function isImageIcon(icon: string | undefined): boolean {
  return !!icon && !(icon in RADIAL_ICONS) && /[\/.]/.test(icon);
}

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

export function updateMenu(p: Profile, i: number, patch: Partial<RadialMenu>): Profile {
  const all = [...(p.radial_menus ?? [])];
  all[i] = { ...all[i], ...patch };
  for (const k of Object.keys(all[i]) as (keyof RadialMenu)[]) if (all[i][k] === undefined) delete all[i][k];
  return { ...p, radial_menus: all };
}

export function updateSector(p: Profile, i: number, k: number, patch: Partial<RadialSector>): Profile {
  const sectors = [...(p.radial_menus?.[i]?.sectors ?? [])];
  const next: RadialSector = { ...sectors[k], ...patch };
  for (const key of Object.keys(next) as (keyof RadialSector)[]) if (next[key] === undefined) delete next[key];
  sectors[k] = next;
  return updateMenu(p, i, { sectors });
}

/** A new menu with four sectors, opened by layer `layer`. */
export function addMenu(p: Profile, layer: number): Profile {
  const all = p.radial_menus ?? [];
  const sectors: RadialSector[] = [{ label: '1' }, { label: '2' }, { label: '3' }, { label: '4' }];
  return { ...p, radial_menus: [...all, { name: t('radial.default_name', { n: all.length + 1 }), layer, sectors }] };
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
  const next: Profile = { ...p, radial_menus };
  if (radial_menus.length === 0) delete next.radial_menus;
  return next;
}
