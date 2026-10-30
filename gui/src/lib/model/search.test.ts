import { describe, expect, it } from 'vitest';
import { setLanguage } from '../i18n/index.svelte';
import type { Profile } from '../types';
import { mappingsPressing, searchMappings } from './search';

const p: Profile = {
  name: 'T',
  mappings: [
    { trigger: 'A', actions: [{ key: 'Space' }], description: 'Jump' },
    { layer: 1, trigger: 'B', actions: [{ key: 'Backspace' }] },
    { trigger: ['LB', 'RB'], mode: 'once', actions: [{ key: 'Q' }, { pause: 50 }, { key: 'Space' }] },
    { trigger: 'Key:W', actions: [{ pad: 'A' }] },
    { trigger: 'X', jump: { layer: 1 } },
  ],
};
const label = (id: number) => (id === 0 ? 'Main' : `Shift ${id}`);
const find = (q: string, scope?: 'all' | 'outputs') => searchMappings(p, q, null, label, scope).map((f) => f.index);

describe('search', () => {
  setLanguage('en');

  it('finds by output, control, layer change and description, case-free', () => {
    expect(find('space')).toEqual([0, 2, 1]);
    expect(find('jump')).toEqual([0]);
    expect(find('LB')).toEqual([2]);
    expect(find('shift 1')).toEqual([4]);
    expect(find('nothing like this')).toEqual([]);
    expect(find('   ')).toEqual([]);
  });

  it('every word must match somewhere', () => {
    expect(find('space jump')).toEqual([0]);
  });

  it('outputs only leaves controls and notes out', () => {
    expect(find('jump', 'outputs')).toEqual([]);
    expect(find('W', 'outputs')).toEqual([]);
  });

  it('lists exactly the mappings that press a key, also inside macros', () => {
    expect(mappingsPressing(p, 'Space', label).map((f) => f.index)).toEqual([0, 2]);
    expect(mappingsPressing(p, 'Backspace', label)).toMatchObject([{ index: 1, layer: 1, trigger: 'B' }]);
  });
});
