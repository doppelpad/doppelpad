import { describe, expect, it } from 'vitest';
import type { LayoutControl } from '../../assets/devices/layouts';
import { setLanguage } from '../i18n/index.svelte';
import type { Profile } from '../types';
import { buildChips, clickSide, gyroChipOf, type ChipView } from './chips';
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
  pressed: new Set(),
  layerLabel: (id) => layerLabel(p, id),
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
    const [ls] = buildChips([at('LS')], view({ selectedKey: 'LeftStickUp', pressed: new Set(['LeftStickLeft']) }));
    expect(ls).toMatchObject({ selected: true, pressed: true });
    const [rs] = buildChips([at('RS')], view({ stick: 'right' }));
    expect(rs.selected).toBe(true);
  });

  it('works without a config', () => {
    expect(buildChips([at('A')], view({ profile: null }))[0]).toMatchObject({ summary: '', mapped: false });
  });

  it('sum up the gyro directions', () => {
    expect(gyroChipOf(view({ selectedKey: 'GyroUp' }))).toEqual({ summary: '← Z', selected: true });
    expect([clickSide('LS'), clickSide('RS'), clickSide('A')]).toEqual(['left', 'right', null]);
  });
});
