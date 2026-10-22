import { describe, expect, it } from 'vitest';
import type { LayoutControl } from '../../assets/devices/layouts';
import { setLanguage } from '../i18n/index.svelte';
import type { Profile } from '../types';
import { buildChips, clickSide, gyroChipOf, pressedChips, type ChipView } from './chips';
import { layerLabel } from './labels';

setLanguage('en');

const at = (control: string): LayoutControl => ({ control, anchor: [0, 0], side: 'left', labelAt: [10, 0] });
const p: Profile = {
  name: 'Test',
  mappings: [
    { trigger: 'A', actions: [{ key: 'Space' }] },
    { trigger: 'LeftStickUp', actions: [{ key: 'W' }] },
    { trigger: ['LB', 'A'], actions: [{ key: 'Q' }] },
    { trigger: 'GyroLeft', actions: [{ key: 'Z' }] },
  ],
  analog: [{ source: 'right_stick', target: 'mouse' }],
};
const view = (patch: Partial<ChipView> = {}): ChipView => ({
  profile: p,
  layer: 0,
  family: 'xbox',
  selectedKey: '',
  stick: null,
  layerLabel: (id) => layerLabel(p, id),
  notes: false,
  ...patch,
});

describe('drawing chips', () => {
  it('sum up a control, its directions and the whole-stick route', () => {
    const [a, ls, rs, b] = buildChips([at('A'), at('LS'), at('RS'), at('B')], view());
    expect(a).toMatchObject({ label: 'A', summary: 'Space', mapped: true, combo: true });
    expect(ls.summary).toBe('↑ W');
    expect(rs).toMatchObject({ summary: 'Mouse', mapped: true });
    expect(b).toMatchObject({ summary: '', mapped: false, combo: false });
  });

  it('select and highlight a stick through its directions', () => {
    const [ls] = buildChips([at('LS')], view({ selectedKey: 'LeftStickUp' }));
    expect(ls.selected).toBe(true);
    expect(pressedChips([at('LS'), at('A')], new Set(['LeftStickLeft']))).toEqual(new Set(['LS']));
    expect(pressedChips([at('LS'), at('A')], new Set(['A']))).toEqual(new Set(['A']));
    const [rs] = buildChips([at('RS')], view({ stick: 'right' }));
    expect(rs.selected).toBe(true);
  });

  it('works without a config', () => {
    expect(buildChips([at('A')], view({ profile: null }))[0]).toMatchObject({ summary: '', mapped: false });
  });

  it('show notes instead of actions, and mark notes they do not show', () => {
    const noted: Profile = {
      ...p,
      mappings: [
        { trigger: 'A', actions: [{ key: 'Space' }], description: 'Jump' },
        { trigger: 'A', activator: 'long', actions: [{ key: 'C' }], description: 'Crouch' },
        { trigger: 'LeftStickUp', actions: [{ key: 'W' }], description: 'Walk' },
        { trigger: 'B', actions: [{ key: 'E' }] },
      ],
    };
    const v = (notes: boolean) => view({ profile: noted, layerLabel: (id) => layerLabel(noted, id), notes });
    const [a, ls, b] = buildChips([at('A'), at('LS'), at('B')], v(false));
    expect([a.summary, a.hiddenNote, ls.hiddenNote, b.hiddenNote]).toEqual(['Space · C', true, true, false]);
    const [a2, ls2, b2] = buildChips([at('A'), at('LS'), at('B')], v(true));
    expect([a2.summary, ls2.summary, b2.summary]).toEqual(['Jump · Crouch', '↑ Walk', 'E']);
    expect([a2.hiddenNote, ls2.hiddenNote]).toEqual([false, false]);
  });

  it('sum up the gyro directions', () => {
    expect(gyroChipOf(view({ selectedKey: 'GyroUp' }))).toEqual({ summary: '← Z', selected: true });
    expect([clickSide('LS'), clickSide('RS'), clickSide('A')]).toEqual(['left', 'right', null]);
  });
});
