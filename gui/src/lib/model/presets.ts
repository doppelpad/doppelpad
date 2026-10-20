// Layer presets: one layer's settings, mappings and routes, kept to insert into other configs
// as a new layer.
import type { LayerPreset, Mapping, Profile } from '../types';
import { addLayer, copyLayer, layerIds, pasteLayer } from './profile';

/** In a preset, a jump to the preset's own layer (which gets a new id when inserted). */
export const SELF = 255;

export function makePreset(p: Profile, layer: number, name: string): LayerPreset {
  const { mappings, analog } = copyLayer(p, layer);
  const decl = layer === 0 ? undefined : (p.layers ?? []).find((l) => l.id === layer);
  const own = (m: Mapping): Mapping => {
    const { layer: _, ...rest } = m;
    return m.jump?.layer === layer ? { ...rest, jump: { ...m.jump, layer: SELF } } : rest;
  };
  return {
    name,
    ...(decl ? { layer: { ...structuredClone(decl), id: 0, name } } : {}),
    mappings: mappings.map(own),
    analog: analog.map(({ layer: _, ...r }) => r),
  };
}

/** The preset as a new layer of `p`; jumps to layers `p` does not have are dropped (counted). */
export function insertPreset(p: Profile, preset: LayerPreset): { profile: Profile; layer: number; dropped: number } {
  const { profile: withLayer, id } = addLayer(p);
  const known = new Set(layerIds(withLayer));
  let dropped = 0;
  const mappings = (preset.mappings ?? []).map((m): Mapping => {
    if (!m.jump) return m;
    const target = m.jump.layer === SELF ? id : m.jump.layer;
    if (!known.has(target)) {
      dropped += 1;
      const { jump: _, ...rest } = m;
      return rest;
    }
    return { ...m, jump: { ...m.jump, layer: target } };
  });
  const pasted = pasteLayer(withLayer, { mappings, analog: preset.analog ?? [] }, id);
  const layers = (pasted.layers ?? []).map((l) => (l.id === id ? { ...(preset.layer ?? { inherit: true }), id, name: preset.name } : l));
  return { profile: { ...pasted, layers }, layer: id, dropped };
}
