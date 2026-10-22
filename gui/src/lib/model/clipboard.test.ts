import { describe, expect, it } from 'vitest';
import type { Profile } from '../types';
import {
  clearControl,
  clearGroup,
  copyControl,
  copyGroup,
  groupHasOwn,
  groupOf,
  hasOwn,
  pasteControl,
  pasteGroup,
  type Copied,
} from './clipboard';

const base = (): Profile => ({
  name: 'Test',
  mappings: [
    { trigger: 'A', actions: [{ key: 'Space' }] },
    { trigger: 'A', activator: 'long', mode: 'turbo', turbo_ms: 50, actions: [{ key: 'F' }], description: 'spam' },
    { trigger: 'B', actions: [{ key: 'Escape' }] },
    { trigger: 'LB', jump: { layer: 1 } },
    { layer: 1, trigger: 'X', actions: [{ key: 'E' }] },
    { trigger: 'LeftStickUp', actions: [{ key: 'W' }] },
    { trigger: 'LeftStickUpLeft', actions: [{ key: 'Q' }] },
    { trigger: 'LeftStickHigh', actions: [{ key: 'Shift' }] },
    { trigger: ['LeftStickUp', 'A'], actions: [{ key: 'Tab' }] },
  ],
  analog: [{ source: 'left_stick', target: 'mouse', sensitivity: 2 }],
});

const control = (c: Copied) => {
  if (c.kind !== 'control') throw new Error('not a control copy');
  return c;
};
const group = (c: Copied) => {
  if (c.kind !== 'group') throw new Error('not a group copy');
  return c;
};

describe('control copy', () => {
  it('copies every activator and pastes onto another control, replacing it', () => {
    const p = base();
    const out = pasteControl(p, 0, 'B', control(copyControl(p, 0, 'A')));
    const onB = out.mappings.filter((m) => m.trigger === 'B');
    expect(onB).toEqual([
      { trigger: 'B', actions: [{ key: 'Space' }] },
      { trigger: 'B', activator: 'long', mode: 'turbo', turbo_ms: 50, actions: [{ key: 'F' }], description: 'spam' },
    ]);
    expect(out.mappings.filter((m) => m.trigger === 'A')).toHaveLength(2);
  });

  it('copies what a layer inherits and pastes onto that layer', () => {
    const p = base();
    const copied = control(copyControl(p, 1, 'A'));
    expect(copied.mappings).toHaveLength(2);
    const out = pasteControl(p, 1, 'Y', copied);
    expect(out.mappings.filter((m) => m.trigger === 'Y').every((m) => m.layer === 1)).toBe(true);
  });

  it('pastes onto a shortcut', () => {
    const p = base();
    const out = pasteControl(p, 0, ['LeftStickUp', 'A'], control(copyControl(p, 0, 'B')));
    const sc = out.mappings.filter((m) => Array.isArray(m.trigger));
    expect(sc).toEqual([{ trigger: ['LeftStickUp', 'A'], actions: [{ key: 'Escape' }] }]);
  });

  it('the copy does not change with the profile', () => {
    const p = base();
    const copied = control(copyControl(p, 0, 'A'));
    p.mappings[0]!.actions![0] = { key: 'Z' };
    expect(copied.mappings[0]!.actions).toEqual([{ key: 'Space' }]);
  });

  it('clear removes only the layer’s own mappings', () => {
    const p = base();
    expect(hasOwn(p, 1, 'A')).toBe(false);
    expect(clearControl(p, 1, 'A')).toEqual(p);
    const out = clearControl(p, 0, 'A');
    expect(out.mappings.some((m) => m.trigger === 'A')).toBe(false);
    expect(out.mappings).toHaveLength(p.mappings.length - 2);
  });
});

describe('groups', () => {
  it('knows the group of a control', () => {
    expect(groupOf('LeftStickUpLeftHigh')).toEqual({ group: 'LeftStick', part: 'UpLeftHigh' });
    expect(groupOf('RightStickMed')).toEqual({ group: 'RightStick', part: 'Med' });
    expect(groupOf('DpadLeft')).toEqual({ group: 'Dpad', part: 'Left' });
    expect(groupOf('TouchCenter')).toEqual({ group: 'Touch', part: 'Center' });
    expect(groupOf('TrackpadRightTap')).toEqual({ group: 'TrackpadRight', part: 'Tap' });
    expect(groupOf('GyroLeanLeftLow')).toEqual({ group: 'Gyro', part: 'LeanLeftLow' });
    expect(groupOf('MouseMove:DownRight')).toEqual({ group: 'MouseMove', part: 'DownRight' });
    expect(groupOf('TouchTap')).toBeNull();
    expect(groupOf('A')).toBeNull();
    expect(groupOf('LS')).toBeNull();
  });

  it('left stick to right stick: directions, zones and routes move along', () => {
    const p = base();
    const { profile, dropped } = pasteGroup(p, 0, 'RightStick', group(copyGroup(p, 0, 'LeftStick')));
    expect(dropped).toBe(0);
    const right = profile.mappings.filter((m) => typeof m.trigger === 'string' && m.trigger.startsWith('RightStick'));
    expect(right.map((m) => m.trigger).sort()).toEqual(['RightStickHigh', 'RightStickUp', 'RightStickUpLeft']);
    expect(profile.analog).toEqual([
      { source: 'left_stick', target: 'mouse', sensitivity: 2 },
      { source: 'right_stick', target: 'mouse', sensitivity: 2 },
    ]);
    // The shortcut with a stick direction stays where it was.
    expect(profile.mappings.filter((m) => Array.isArray(m.trigger))).toHaveLength(1);
  });

  it('parts and routes the target lacks are dropped and counted', () => {
    const p = base();
    const { profile, dropped } = pasteGroup(p, 0, 'Dpad', group(copyGroup(p, 0, 'LeftStick')));
    // UpLeft, High and the mouse route have no place on the D-pad.
    expect(dropped).toBe(3);
    expect(profile.mappings.filter((m) => typeof m.trigger === 'string' && m.trigger.startsWith('Dpad'))).toEqual([
      { trigger: 'DpadUp', actions: [{ key: 'W' }] },
    ]);
    expect(profile.analog).toHaveLength(1);
  });

  it('a mouse route cannot come from the mouse', () => {
    const p = base();
    const { profile, dropped } = pasteGroup(p, 0, 'MouseMove', group(copyGroup(p, 0, 'LeftStick')));
    expect(dropped).toBe(1);
    expect(profile.analog).toEqual(p.analog);
  });

  it('pasting replaces the target group on that layer only', () => {
    const p: Profile = {
      ...base(),
      mappings: [
        ...base().mappings,
        { trigger: 'RightStickDown', actions: [{ key: 'S' }] },
        { layer: 1, trigger: 'RightStickDown', actions: [{ key: 'C' }] },
      ],
    };
    const { profile } = pasteGroup(p, 0, 'RightStick', group(copyGroup(p, 0, 'LeftStick')));
    expect(profile.mappings.some((m) => m.trigger === 'RightStickDown' && !m.layer)).toBe(false);
    expect(profile.mappings.some((m) => m.trigger === 'RightStickDown' && m.layer === 1)).toBe(true);
  });

  it('a shift layer copies what it inherits, including Main’s routes', () => {
    const p = base();
    const copied = group(copyGroup(p, 1, 'LeftStick'));
    expect(copied.parts.map((x) => x.part).sort()).toEqual(['High', 'Up', 'UpLeft']);
    expect(copied.routes).toHaveLength(1);
    const { profile } = pasteGroup(p, 1, 'RightStick', copied);
    expect(profile.analog?.at(-1)).toEqual({ source: 'right_stick', target: 'mouse', sensitivity: 2, layer: 1 });
  });

  it('clear removes the group’s own mappings and routes', () => {
    const p = base();
    expect(groupHasOwn(p, 0, 'LeftStick')).toBe(true);
    expect(groupHasOwn(p, 1, 'LeftStick')).toBe(false);
    const out = clearGroup(p, 0, 'LeftStick');
    expect(out.analog).toBeUndefined();
    expect(out.mappings.some((m) => typeof m.trigger === 'string' && m.trigger.startsWith('LeftStick'))).toBe(false);
    expect(groupHasOwn(out, 0, 'LeftStick')).toBe(false);
  });
});
