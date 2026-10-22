// Small helpers shared by the model modules. Profile data is plain JSON by contract (it travels
// as JSON to and from Rust), which `clone` and `sameJson` rely on.

/** `v` limited to `lo`..`hi`. */
export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** `v` rounded to `digits` decimals. */
export function roundTo(v: number, digits: number): number {
  const f = 10 ** digits;
  return Math.round(v * f) / f;
}

/** Deep copy of JSON data; unlike `structuredClone` it also works on Svelte state proxies. */
export const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));

/** Whether two JSON values are the same, field order included. */
export const sameJson = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** A copy of `o` without the fields that are `undefined`. */
export function dropUndefined<T extends object>(o: T): T {
  const out = { ...o };
  for (const k of Object.keys(out) as (keyof T)[]) if (out[k] === undefined) delete out[k];
  return out;
}

/** A copy of `o` with field `k` set to `v`, or without it when `v` is `undefined`. */
export function setOrDrop<T extends object, K extends keyof T>(o: T, k: K, v: T[K] | undefined): T {
  const out = { ...o };
  if (v === undefined) delete out[k];
  else out[k] = v;
  return out;
}

/** `v`, or `undefined` for an empty object or list (which profiles leave out, like Rust). */
export function nonEmpty<T extends object>(v: T): T | undefined {
  return Object.keys(v).length > 0 ? v : undefined;
}
