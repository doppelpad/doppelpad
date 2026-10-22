import { describe, expect, it } from 'vitest';
import { analogTabOf, fullPullBase } from './controls';

describe('control names', () => {
  it('finds the analog settings page of a stick or trigger control', () => {
    expect(['LS', 'LeftStickUpHigh', 'RS', 'RightStickLow', 'LT', 'RTFull', 'LTMed', 'A'].map(analogTabOf)).toEqual([
      'left',
      'left',
      'right',
      'right',
      'triggers',
      'triggers',
      'triggers',
      null,
    ]);
  });

  it('finds the trigger of a full pull', () => {
    expect([fullPullBase('LTFull'), fullPullBase('RTFull'), fullPullBase('LT'), fullPullBase('LTHigh')]).toEqual(['LT', 'RT', null, null]);
  });
});
