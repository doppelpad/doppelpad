import { describe, expect, it } from 'vitest';
import { emptyProfile as empty } from './fixtures';
import { MOUSE_ZONE_DEFAULTS, pruneDefaults, vibrationConfig, withMouseZone, withShake, withTurn } from './output';

describe('output settings from typed text', () => {
  it('leave values at their defaults out', () => {
    expect(pruneDefaults({ a: 1, b: 2 }, { a: 1, b: 2 })).toBeUndefined();
    expect(pruneDefaults({ a: 1, b: 3 }, { a: 1, b: 2 })).toEqual({ a: 1, b: 3 });
  });

  it('write the turn settings in full, or not at all', () => {
    const p = withTurn(empty(), 'turn_ms', '250');
    expect(p.virtual?.mouse).toEqual({ counts_per_turn: 18000, turn_ms: 250, wheel_repeat_ms: 0 });
    expect(withTurn(p, 'turn_ms', 'x').virtual).toBeUndefined();
    expect(withTurn(empty(), 'counts_per_turn', '0').virtual).toBeUndefined();
    expect(withTurn(empty(), 'wheel_repeat_ms', '6000').virtual).toBeUndefined();
  });

  it('read shake values with a decimal comma', () => {
    expect(withShake(empty(), 'shake_g', '1,5').virtual?.motion).toEqual({ shake_g: 1.5, shake_ms: 150 });
    expect(withShake(empty(), 'shake_ms', '-3').virtual).toBeUndefined();
  });

  it('keep the mouse zones rising', () => {
    expect(withMouseZone(empty(), 1, '5000').mouse).toEqual({ zones: [200, 5000, 5001] });
    expect(withMouseZone(empty(), 1, String(MOUSE_ZONE_DEFAULTS[1])).mouse).toBeUndefined();
    const p = empty();
    expect(withMouseZone(p, 0, 'abc')).toBe(p);
  });
});

describe('settings with defaults', () => {
  it('keep what is set and fill in the rest', () => {
    const def = { left: 1, right: 1, swap: false, min_delay_ms: 20 };
    expect(vibrationConfig({ left: 0.5 }, def)).toEqual({ left: 0.5, right: 1, swap: false, min_delay_ms: 20 });
    expect(vibrationConfig(undefined, def)).toEqual(def);
  });
});
