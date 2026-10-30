// A mapping's fields with their defaults filled in, and the layer change it makes. A leaf
// module: the layer, profile and route helpers all build on it.

import type { Activator, Combo, JumpMode, Mapping, Mode } from '../types';

export const layerOf = (m: Mapping) => m.layer ?? 0;
export const activatorOf = (m: Mapping): Activator => m.activator ?? 'single';
export const modeOf = (m: Mapping): Mode => m.mode ?? 'hold';

/** Whether mapping `m` changes to layer `id` from another layer (a jump to its own layer does nothing). */
export const jumpsTo = (m: Mapping, id: number) => m.jump?.layer === id && layerOf(m) !== id;

/** A mapping that changes to a layer: on which layer it sits, its trigger and how it jumps. */
export interface LayerEntrance {
  from: number;
  trigger: Combo;
  activator: Activator;
  mode: JumpMode;
  delayMs: number;
}

/** The way into another layer that mapping `m` makes, if it makes one. */
export function entranceOf(m: Mapping): (LayerEntrance & { to: number }) | null {
  const jump = m.jump;
  if (!jump || !jumpsTo(m, jump.layer)) return null;
  return {
    from: layerOf(m),
    to: jump.layer,
    trigger: m.trigger,
    activator: activatorOf(m),
    mode: jump.mode ?? 'hold',
    delayMs: jump.delay_ms ?? 0,
  };
}
