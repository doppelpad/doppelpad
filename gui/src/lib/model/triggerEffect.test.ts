import { describe, expect, it } from 'vitest';
import type { TriggerEffect } from '../types';
import { EFFECT_KINDS, EFFECT_PRESETS, effectHex, freshEffect, parseEffectHex, sameEffect, withEffectStart } from './triggerEffect';

describe('trigger effects', () => {
  it('starts every kind fresh, as a copy', () => {
    for (const k of EFFECT_KINDS) expect(freshEffect(k).kind).toBe(k);
    const raw = freshEffect('raw');
    if (raw.kind === 'raw') raw.bytes[0] = 9;
    expect(freshEffect('raw')).toEqual({ kind: 'raw', bytes: [5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] });
  });

  it('compares field by field, whatever the order', () => {
    const rifle = EFFECT_PRESETS.find((p) => p.id === 'rifle')!.effect;
    const loaded = { strength: 8, end: 6, start: 4, kind: 'weapon' } as TriggerEffect;
    expect(sameEffect(rifle, loaded)).toBe(true);
    expect(sameEffect(rifle, { ...loaded, end: 7 } as TriggerEffect)).toBe(false);
    expect(sameEffect(undefined, undefined)).toBe(true);
    expect(sameEffect(rifle, undefined)).toBe(false);
    expect(sameEffect({ kind: 'raw', bytes: [1, 2] }, { kind: 'raw', bytes: [1, 2] })).toBe(true);
  });

  it("keeps a weapon's end behind its start", () => {
    expect(withEffectStart({ kind: 'weapon', start: 4, end: 6, strength: 8 }, 6)).toEqual({ kind: 'weapon', start: 6, end: 7, strength: 8 });
    expect(withEffectStart({ kind: 'weapon', start: 4, end: 6, strength: 8 }, 2)).toEqual({ kind: 'weapon', start: 2, end: 6, strength: 8 });
    expect(withEffectStart({ kind: 'resistance', start: 0, strength: 4 }, 3)).toEqual({ kind: 'resistance', start: 3, strength: 4 });
  });

  it('reads and writes the raw bytes as hex', () => {
    const bytes = [5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x1f];
    expect(effectHex(bytes)).toBe('05 00 00 00 00 00 00 00 00 00 1f');
    expect(parseEffectHex('05,0 0 0 0 0 0 0 0 0 1F')).toEqual(bytes);
    expect(parseEffectHex('05 00')).toBeNull();
    expect(parseEffectHex('05 00 00 00 00 00 00 00 00 00 zz')).toBeNull();
  });
});
