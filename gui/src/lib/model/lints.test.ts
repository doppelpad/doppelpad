import { describe, expect, it } from 'vitest';
import type { DiagMsg, Hotkeys, Profile } from '../types';
import { diagTarget, flagsOf, hotkeyClashes, isFlagged, isReached, mappingPath, waitsForSave } from './lints';

const p: Profile = {
  name: 'T',
  layers: [{ id: 2 }],
  mappings: [
    { trigger: ['LB', 'RB', 'X'], actions: [{ key: 'F' }] },
    { trigger: 'A', actions: [{ key: 'Space' }] },
    { layer: 1, trigger: 'B', actions: [{ key: 'E' }] },
    { trigger: ['Back', 'Start'], actions: [{ key: 'Escape' }] },
  ],
};

const hotkeys: Hotkeys = {
  hold_ms: 500,
  slot1: ['LB', 'RB', 'X'],
  slot2: ['LB', 'RB', 'Y'],
  slot3: [],
  slot4: [],
  remap: [],
  stop_macros: [],
  emergency_stop: ['Back', 'Start', 'LB', 'RB'],
  pad_overlay: [],
  mappings_overlay: [],
  app_navigation: [],
  rumble: false,
};

const warn = (path: string): DiagMsg => ({ severity: 'warning', path, message: 'x' });

describe('lints', () => {
  it('a mapping holding every button of a hotkey clashes; a part of one does not', () => {
    const found = hotkeyClashes(p, hotkeys, 'xbox');
    expect(found.map((d) => d.path)).toEqual(['mappings[0] (LB+RB+X)']);
    expect(found[0]!.message).toContain('Left bumper + Right bumper + X');
    expect(hotkeyClashes(p, null, 'xbox')).toEqual([]);
  });

  it('every hotkey can clash, named as the controller names its buttons', () => {
    const found = hotkeyClashes(p, { ...hotkeys, app_navigation: ['Back', 'Start'] }, 'dual_sense');
    expect(found.map((d) => d.path)).toEqual(['mappings[0] (LB+RB+X)', 'mappings[3] (Back+Start)']);
    expect(found[1]!.message).toContain('Create + Options');
  });

  it('paths lead to mappings and layers', () => {
    expect(mappingPath(3, ['Back', 'Start'])).toBe('mappings[3] (Back+Start)');
    expect(diagTarget(p, warn('mappings[2] (B)'))).toEqual({ kind: 'mapping', layer: 1, trigger: 'B', activator: 'single' });
    expect(diagTarget(p, warn('layers[0]'))).toEqual({ kind: 'layer', layer: 2 });
    expect(diagTarget(p, warn('layers[0].lightbar'))).toBeNull();
    expect(diagTarget(p, warn('analog[0]'))).toBeNull();
    expect(diagTarget(p, warn('mappings[9] (Z)'))).toBeNull();
  });

  it('warnings mark their control on its layer, and on layers that take it over', () => {
    const flags = flagsOf(p, [warn('mappings[1] (A)'), warn('layers[0]'), { severity: 'info', path: 'mappings[2] (B)', message: 'x' }]);
    expect(isFlagged(flags, p, 0, 'A')).toBe(true);
    expect(isFlagged(flags, p, 1, 'A')).toBe(true);
    expect(isFlagged(flags, p, 1, 'B')).toBe(false);
    expect(isFlagged(flags, p, 0, 'B')).toBe(false);
    expect(flags.layers).toEqual(new Set([2]));
  });
});

describe('layers being set up', () => {
  const q: Profile = {
    name: 'T',
    layers: [{ id: 1 }, { id: 2 }],
    mappings: [
      { trigger: 'LB', jump: { layer: 1 } },
      { layer: 2, trigger: 'A', jump: { layer: 2, mode: 'toggle' } },
    ],
  };
  const unreached: DiagMsg = { severity: 'warning', path: 'layers[1]', message: 'nothing changes to layer 2' };

  it('knows which layers something changes to', () => {
    expect(isReached(q, 0)).toBe(true);
    expect(isReached(q, 1)).toBe(true);
    // Only from another layer.
    expect(isReached(q, 2)).toBe(false);
  });

  it('holds back the warning about a layer the saved config does not have', () => {
    expect(waitsForSave(q, unreached, new Set([0, 1]))).toBe(true);
    expect(waitsForSave(q, unreached, new Set([0, 1, 2]))).toBe(false);
    expect(waitsForSave(q, { ...unreached, severity: 'error' }, new Set([0]))).toBe(false);
    expect(waitsForSave(q, { severity: 'warning', path: 'mappings[1] (A)', message: 'x' }, new Set([0]))).toBe(false);
  });
});
