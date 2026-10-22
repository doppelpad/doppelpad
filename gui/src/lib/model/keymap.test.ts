import { describe, expect, it } from 'vitest';
import { keyGroups, keyLabel } from './keymap';

describe('key picker', () => {
  it('groups keys in a fixed order and filters by search', () => {
    const keys = ['Space', 'A', 'Digit1', 'F5', 'LeftCtrl', 'Numpad2', 'VolumeUp', 'Up', 'B'];
    expect(keyGroups(keys, '')).toEqual([
      { id: 'letters', keys: ['A', 'B'] },
      { id: 'digits', keys: ['Digit1'] },
      { id: 'modifiers', keys: ['LeftCtrl'] },
      { id: 'function', keys: ['F5'] },
      { id: 'navigation', keys: ['Up'] },
      { id: 'numpad', keys: ['Numpad2'] },
      { id: 'media', keys: ['VolumeUp'] },
      { id: 'other', keys: ['Space'] },
    ]);
    expect(keyGroups(keys, 'up')).toEqual([
      { id: 'navigation', keys: ['Up'] },
      { id: 'media', keys: ['VolumeUp'] },
    ]);
  });

  it('labels digits and the number pad shortly', () => {
    expect(['Digit7', 'Numpad3', 'Space'].map(keyLabel)).toEqual(['7', 'Num 3', 'Space']);
  });
});
