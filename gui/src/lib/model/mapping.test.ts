import { describe, expect, it } from 'vitest';
import { entranceOf, jumpsTo } from './mapping';

describe('layer changes', () => {
  it('a jump to another layer is a way in, with its defaults; a jump to its own layer is none', () => {
    expect(entranceOf({ layer: 2, trigger: 'A', jump: { layer: 1 } })).toEqual({
      from: 2,
      to: 1,
      trigger: 'A',
      activator: 'single',
      mode: 'hold',
      delayMs: 0,
    });
    expect(entranceOf({ layer: 1, trigger: 'A', jump: { layer: 1, mode: 'toggle' } })).toBeNull();
    expect(entranceOf({ trigger: 'A', actions: [] })).toBeNull();
    expect(jumpsTo({ trigger: 'B', activator: 'long', jump: { layer: 3, mode: 'switch', delay_ms: 50 } }, 3)).toBe(true);
    expect(jumpsTo({ trigger: 'B', jump: { layer: 3 } }, 2)).toBe(false);
  });
});
