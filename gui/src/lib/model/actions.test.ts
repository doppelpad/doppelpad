import { describe, expect, it } from 'vitest';
import { hasParameters, recordedSteps, stickAmount, withStickAmount } from './actions';

describe('stick amount', () => {
  it('scales the deflection and keeps the direction', () => {
    const a = { stick: 'left' as const, x: 0, y: 1 };
    expect(stickAmount(a)).toBe(1);
    expect(withStickAmount(a, 0.5)).toEqual({ stick: 'left', x: 0, y: 0.5 });
    const diag = { stick: 'right' as const, x: -0.5, y: 0.5 };
    expect(withStickAmount(diag, 1)).toEqual({ stick: 'right', x: -1, y: 1 });
    expect(withStickAmount(a, 3).y).toBe(1);
  });

  it('knows which actions have parameters', () => {
    expect(hasParameters({ trigger: 'left' })).toBe(true);
    expect(hasParameters({ command: { launch: { path: 'a.exe' } } })).toBe(true);
    expect(hasParameters({ key: 'Space' })).toBe(false);
    expect(hasParameters({ command: 'remap_off' })).toBe(false);
  });
});

describe('controller recorder', () => {
  it('turns changes of held controls into steps with pauses', () => {
    const none = new Set<string>();
    const a = new Set(['A']);
    expect(recordedSteps(none, a, null, 10)).toEqual([{ pad: 'A', action: 'down' }]);
    expect(recordedSteps(a, new Set(['LeftStickUp']), 250, 10)).toEqual([
      { pause: 250 },
      { pad: 'A', action: 'up' },
      { stick: 'left', x: 0, y: 1, action: 'down' },
    ]);
    expect(recordedSteps(none, new Set(['LT']), 8, 0), 'short gaps are dropped').toEqual([
      { trigger: 'left', action: 'down' },
    ]);
  });
});
