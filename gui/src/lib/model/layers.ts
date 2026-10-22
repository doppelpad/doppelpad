// Declared layers and the profile-wide virtual devices: finding and changing a layer's
// declaration, declaring it on first change.

import type { Layer, Profile, VirtualDevices } from '../types';
import { nonEmpty, setOrDrop } from './util';

/** The declaration of layer `id`, if the profile has one. */
export function declaredLayer(p: Profile, id: number): Layer | undefined {
  return p.layers?.find((l) => l.id === id);
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
export function setVirtualDevice<K extends keyof VirtualDevices>(
  p: Profile,
  key: K,
  value: VirtualDevices[K] | undefined,
): Profile {
  return setOrDrop(p, 'virtual', nonEmpty(setOrDrop({ ...p.virtual }, key, value)));
}
