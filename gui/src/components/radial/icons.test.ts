import { describe, expect, it } from 'vitest';
import { isImageIcon } from './icons';

describe('radial icons', () => {
  it('tells image files from icon names', () => {
    expect(isImageIcon('sword')).toBe(false);
    expect(isImageIcon('C:\icons\potion.png')).toBe(true);
    expect(isImageIcon(undefined)).toBe(false);
  });
});
