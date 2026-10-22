import { describe, expect, it } from 'vitest';
import type { Catalog, Profile } from '../types';
import defaultsJson from './fixtures/defaults.json';
import { MOUSE_PRESETS, mousePresetOf, mouseResponse } from './routes';
import { setStickGroup, stickGroupConflicts, stickGroupOf, stickRouteIndex } from './stickgroups';

// Rust's defaults, as the catalog command sends them.
const def = (defaultsJson as unknown as Catalog['defaults']).analog;

/** The Minecraft case: the right stick's four directions move the mouse at a fixed speed. */
function profile(): Profile {
  return {
    name: 'test',
    mappings: [
      { trigger: 'LeftStickUp', actions: [{ key: 'W' }] },
      { trigger: 'RightStickUp', actions: [{ mouse_move: 'up', speed: 800 }] },
      { trigger: 'RightStickDown', actions: [{ mouse_move: 'down', speed: 800 }] },
      { trigger: 'RightStickLeftHigh', actions: [{ mouse_move: 'left', speed: 1600 }] },
      { trigger: ['LB', 'RightStickUp'], actions: [{ key: 'F' }] },
      { trigger: 'RightStickUp', layer: 1, actions: [{ key: 'U' }] },
      { trigger: 'RS', actions: [{ key: 'LeftShift' }] },
    ],
  } as Profile;
}

describe('whole-stick mappings', () => {
  it('counts what a choice would replace: directions and their zones on the layer, not shortcuts or the click', () => {
    expect(stickGroupConflicts(profile(), 0, 'right')).toBe(3);
    expect(stickGroupConflicts(profile(), 1, 'right')).toBe(1);
    expect(stickGroupConflicts(profile(), 0, 'left')).toBe(1);
  });

  it('turns the stick into a mouse with the standard preset', () => {
    const p = setStickGroup(profile(), 0, 'right', 'mouse', def);
    expect(p.mappings.map((m) => m.trigger)).toEqual(['LeftStickUp', ['LB', 'RightStickUp'], 'RightStickUp', 'RS']);
    const i = stickRouteIndex(p, 0, 'right');
    expect(p.analog?.[i]).toMatchObject({ source: 'right_stick', target: 'mouse', ...MOUSE_PRESETS.standard });
    expect(stickGroupOf(p, 0, 'right')).toBe('mouse');
    expect(stickGroupOf(p, 1, 'right')).toBeNull();
  });

  it('writes WASD and the arrow keys as four direction mappings, replacing a route', () => {
    let p = setStickGroup(profile(), 0, 'left', 'mouse', def);
    p = setStickGroup(p, 0, 'left', 'wasd', def);
    expect(p.analog).toBeUndefined();
    expect(stickGroupOf(p, 0, 'left')).toBe('wasd');
    const keys = (s: string) => p.mappings.find((m) => m.trigger === s)?.actions;
    expect([keys('LeftStickUp'), keys('LeftStickLeft'), keys('LeftStickDown'), keys('LeftStickRight')]).toEqual([
      [{ key: 'W' }],
      [{ key: 'A' }],
      [{ key: 'S' }],
      [{ key: 'D' }],
    ]);
    p = setStickGroup(p, 0, 'left', 'arrows', def);
    expect(stickGroupOf(p, 0, 'left')).toBe('arrows');
    expect(keys('LeftStickUp')).toEqual([{ key: 'Up' }]);
  });

  it('a flick stick route starts at the flick threshold; null clears the stick', () => {
    let p = setStickGroup(profile(), 1, 'right', 'flick', def);
    expect(p.analog?.[0]).toMatchObject({ layer: 1, source: 'right_stick', target: 'flick', deadzone: 0.6 });
    expect(stickGroupOf(p, 1, 'right')).toBe('flick');
    p = setStickGroup(p, 1, 'right', null, def);
    expect(p.analog).toBeUndefined();
    expect(stickGroupConflicts(p, 1, 'right')).toBe(0);
  });
});

describe('stick → mouse presets', () => {
  it('recognises a preset and calls anything else custom', () => {
    const r = { ...def, ...MOUSE_PRESETS.precise };
    expect(mousePresetOf(r)).toBe('precise');
    expect(mousePresetOf({ ...r, curve: 2.3 })).toBeNull();
    expect(mousePresetOf(def)).toBeNull();
  });

  it('response: nothing in the deadzone, full speed from outer, the curve in between', () => {
    const r = { deadzone: 0.1, outer: 0.9, curve: 2 };
    expect(mouseResponse(0.05, r)).toBe(0);
    expect(mouseResponse(0.95, r)).toBe(1);
    expect(mouseResponse(0.5, r)).toBeCloseTo(0.25);
  });
});
