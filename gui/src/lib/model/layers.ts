// Declared layers and the profile-wide virtual devices: finding and changing a layer's
// declaration, declaring it on first change.

import type { Activator, Combo, JumpMode, Layer, Profile, VirtualDevices } from '../types';
import { nonEmpty, setOrDrop } from './util';

/** The declaration of layer `id`, if the profile has one. */
export function declaredLayer(p: Profile, id: number): Layer | undefined {
  return p.layers?.find((l) => l.id === id);
}

/** A mapping that changes to a layer: on which layer it sits, its trigger and how it jumps. */
export interface LayerEntrance {
  from: number;
  trigger: Combo;
  activator: Activator;
  mode: JumpMode;
  delayMs: number;
}

/**
 * What reaches layer `id`: every mapping that jumps there, from any layer (a jump of Main also
 * works on the layers that inherit it), Main's first.
 */
export function layerEntrances(p: Profile, id: number): LayerEntrance[] {
  return p.mappings
    .filter((m) => m.jump?.layer === id && (m.layer ?? 0) !== id)
    .map((m) => ({
      from: m.layer ?? 0,
      trigger: m.trigger,
      activator: m.activator ?? 'single',
      mode: m.jump!.mode ?? 'hold',
      delayMs: m.jump!.delay_ms ?? 0,
    }))
    .sort((a, b) => a.from - b.from);
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
