// Declared layers and the profile-wide virtual devices: finding and changing a layer's
// declaration, declaring it on first change.

import type { Layer, Profile, VirtualDevices } from '../types';
import { entranceOf, type LayerEntrance } from './mapping';
import { nonEmpty, setOrDrop } from './util';

/** The declaration of layer `id`, if the profile has one. */
export function declaredLayer(p: Profile, id: number): Layer | undefined {
  return p.layers?.find((l) => l.id === id);
}

/**
 * What reaches layer `id`: every mapping that jumps there, from any layer (a jump of Main also
 * works on the layers that inherit it), Main's first.
 */
export function layerEntrances(p: Profile, id: number): LayerEntrance[] {
  return p.mappings
    .flatMap((m) => {
      const e = entranceOf(m);
      if (e?.to !== id) return [];
      const { to: _to, ...entrance } = e;
      return [entrance];
    })
    .sort((a, b) => a.from - b.from);
}

/** A layer in the layer map, at its place on the circle (`x`, `y` in -1…1, Main at the top). */
export interface MapNode {
  id: number;
  x: number;
  y: number;
}

/** A mapping that changes layer, as an arrow of the layer map. */
export interface MapEdge extends LayerEntrance {
  to: number;
  /** Position of the mapping in `profile.mappings`. */
  index: number;
  /** Sideways offset of the arrow's middle, so arrows between the same layers do not overlap. */
  bend: number;
}

/**
 * The layer map: every layer in use on a circle, Main first at the top, and an arrow for each
 * mapping that changes layer (from the layer it sits on to its target).
 */
export function layerGraph(p: Profile, ids: number[]): { nodes: MapNode[]; edges: MapEdge[] } {
  const n = ids.length;
  const nodes = ids.map((id, i) => {
    const a = -Math.PI / 2 + (2 * Math.PI * i) / Math.max(n, 1);
    return { id, x: n === 1 ? 0 : Math.cos(a), y: n === 1 ? 0 : Math.sin(a) };
  });
  const edges: MapEdge[] = [];
  p.mappings.forEach((m, index) => {
    const entrance = entranceOf(m);
    if (entrance) edges.push({ ...entrance, index, bend: 0 });
  });
  // Arrows between the same two layers fan out; the two directions go to opposite sides.
  const pairs = new Map<string, MapEdge[]>();
  for (const e of edges) {
    const key = `${Math.min(e.from, e.to)}-${Math.max(e.from, e.to)}`;
    pairs.set(key, [...(pairs.get(key) ?? []), e]);
  }
  for (const list of pairs.values()) {
    list.forEach((e, k) => {
      const spread = (k - (list.length - 1) / 2) * 0.32;
      e.bend = (e.from < e.to ? 1 : -1) * (spread + (list.length > 1 ? 0 : 0.08));
    });
  }
  return { nodes, edges };
}

/** Names (or numbers, from 1) of the radial menus a layer opens. */
export function layerRadialMenus(p: Profile, id: number): string[] {
  return (p.radial_menus ?? []).flatMap((r, i) => (id !== 0 && r.layer === id ? [r.name || String(i + 1)] : []));
}

/** Change a layer's declaration, declaring it first if needed. */
export function patchLayer(p: Profile, id: number, patch: (l: Layer) => Layer): Profile {
  const layers = [...(p.layers ?? [])];
  const i = layers.findIndex((l) => l.id === id);
  const next = patch(i >= 0 ? { ...layers[i] } : { id, inherit: true });
  if (next.not_inherited && next.not_inherited.length === 0) delete next.not_inherited;
  if (i >= 0) layers[i] = next;
  else layers.push(next);
  return { ...p, layers };
}

/** Set (or with `undefined` remove) one virtual device's settings, keeping the others. */
export function setVirtualDevice<K extends keyof VirtualDevices>(p: Profile, key: K, value: VirtualDevices[K] | undefined): Profile {
  return setOrDrop(p, 'virtual', nonEmpty(setOrDrop({ ...p.virtual }, key, value)));
}
