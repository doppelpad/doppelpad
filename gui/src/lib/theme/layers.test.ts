import { describe, expect, it } from 'vitest';
import { layerColor } from './layers';

describe('layer colours', () => {
  it('give each of the first seven layers its own and the rest the last one', () => {
    expect(layerColor(0)).toBe('var(--layer-0)');
    expect(layerColor(6)).toBe('var(--layer-6)');
    expect(layerColor(7)).toBe('var(--layer-6)');
    expect(layerColor(12)).toBe('var(--layer-6)');
  });
});
