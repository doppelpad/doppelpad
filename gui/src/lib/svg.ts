// Small helpers for SVG markup.

/** Points as an SVG `points` attribute: "x,y x,y …". */
export const polylinePoints = (points: readonly (readonly [number, number])[]) => points.map((p) => p.join(',')).join(' ');
