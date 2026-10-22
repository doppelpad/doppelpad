// Numbers typed into the editor's fields; a comma counts as the decimal point.

import { clamp } from './util';

/** A decimal number, or `null` when the text does not start with one. */
export function parseDecimal(raw: string): number | null {
  const n = parseFloat(raw.replace(',', '.'));
  return Number.isFinite(n) ? n : null;
}

/** A percentage as a fraction, limited to 0..1, or `null`. */
export function parsePercent(raw: string): number | null {
  const n = parseDecimal(raw);
  return n === null ? null : clamp(n, 0, 100) / 100;
}

/** A whole number of at least 0, or `null`. */
export function parseNonNegInt(raw: string): number | null {
  const n = parseInt(raw, 10);
  return Number.isFinite(n) && n >= 0 ? n : null;
}
