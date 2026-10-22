import { describe, expect, it } from 'vitest';
import { has } from '../../lib/i18n/index.svelte';
import { RADIAL_ICONS, isImageIcon } from './icons';

describe('radial icons', () => {
  it('tells image files from icon names', () => {
    expect(isImageIcon('sword')).toBe(false);
    expect(isImageIcon('C:\icons\potion.png')).toBe(true);
    expect(isImageIcon(undefined)).toBe(false);
  });

  it('has a name for every icon', () => {
    for (const id of Object.keys(RADIAL_ICONS)) expect(has(`radial.icons.${id}`), id).toBe(true);
  });
});
