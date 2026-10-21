// The colours that tell layers apart (`--layer-0` … `--layer-6` in tokens.css).

/** Number of layer colours; higher layers share the last one. */
export const LAYER_COLOURS = 7;

/** The CSS colour of layer `id`. */
export function layerColor(id: number): string {
  return `var(--layer-${Math.min(id, LAYER_COLOURS - 1)})`;
}
