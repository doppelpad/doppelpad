// The range and look of a settings slider, for editors that list their sliders in a table.
import { t } from '../../lib/i18n/index.svelte';

export interface SliderSpec {
  min: number;
  max: number;
  step: number;
  /** Shown value = value × scale (100 for a percentage). */
  scale?: number;
  unit?: string;
  /** The slider has a hint: `<prefix>.<field>_hint`, or the key given here. */
  hint?: boolean | string;
}

/** A 0–1 value shown as a percentage. */
export const PCT = { scale: 100, unit: '%' } as const;

/** The hint text of a table slider (`prefix` is the field's label namespace), or none. */
export function sliderHint(prefix: string, field: string, s: SliderSpec): string {
  if (s.hint === true) return t(`${prefix}.${field}_hint`);
  return s.hint ? t(s.hint) : '';
}
