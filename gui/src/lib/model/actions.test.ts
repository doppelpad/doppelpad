import { describe, expect, it } from 'vitest';
import { hasParameters, hasPress, holdFor, recordedSteps, stickAmount, withStickAmount } from './actions';

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
    expect(recordedSteps(none, new Set(['LT']), 8, 0), 'short gaps are dropped').toEqual([{ trigger: 'left', action: 'down' }]);
  });
});

describe('hold for a time', () => {
  it('turns a step into press, pause and release', () => {
    const steps = [{ key: 'A' }, { mouse: 'left' as const, action: 'tap' as const }, { pause: 20 }];
    expect(holdFor(steps, 1, 300)).toEqual([
      { key: 'A' },
      { mouse: 'left', action: 'down' },
      { pause: 300 },
      { mouse: 'left', action: 'up' },
      { pause: 20 },
    ]);
  });

  it('leaves steps that press nothing alone', () => {
    const steps = [{ pause: 20 }, { command: 'remap_off' as const }];
    expect(holdFor(steps, 0)).toBe(steps);
    expect(holdFor(steps, 1)).toBe(steps);
    expect(hasPress({ wheel: 'up' })).toBe(false);
    expect(hasPress({ trigger: 'left' })).toBe(true);
  });
});
