import { describe, expect, it } from 'vitest';
import type { Mapping, Profile } from '../types';
import { has, setLanguage } from '../i18n/index.svelte';
import {
  addLayer,
  applyMode,
  clearLayer,
  comboKey,
  copyLayer,
  editMappingIn,
  effectiveMappings,
  findMapping,
  inheritableTriggers,
  isBlocked,
  isMacroMode,
  layerIds,
  layerInheritance,
  layerUnlock,
  mappingAt,
  nativeAction,
  pasteLayer,
  removeLayer,
  removeMapping,
  segmentCount,
  setLayerInherit,
  setLayerUnlock,
  setLight,
  setProfileTiming,
  setTriggerInherited,
  setVirtualDevice,
  shortcutMembers,
  shortcuts,
  singleControl,
  tidyMapping,
  toCombo,
  upsertMapping,
} from './profile';
import { POWER_COMMANDS, RADIAL_COMMANDS, SIMPLE_COMMANDS, actionLabel, mappingSummary } from './labels';

const base = (): Profile => ({
  name: 'Test',
  mappings: [
    { trigger: 'A', actions: [{ key: 'Space' }] },
    { trigger: 'B', activator: 'long', actions: [{ key: 'Escape' }] },
    { trigger: 'LB', jump: { layer: 1 } },
    { layer: 1, trigger: 'A', actions: [{ key: 'E' }] },
    { trigger: ['LB', 'RB'], actions: [{ key: 'Tab' }] },
  ],
});

const lbl = (id: number) => (id === 0 ? 'Main' : `Shift ${id}`);

describe('combos', () => {
  it('sort and collapse', () => {
    expect(toCombo(['RB', 'LB'])).toEqual(['LB', 'RB']);
    expect(toCombo(['A', 'A'])).toBe('A');
    expect(comboKey(['RB', 'LB'])).toBe('LB+RB');
  });
});

describe('mapping edits', () => {
  it('finds by layer, trigger and activator', () => {
    const p = base();
    expect(findMapping(p, 0, 'B', 'long')?.actions).toEqual([{ key: 'Escape' }]);
    expect(findMapping(p, 0, 'B', 'single')).toBeUndefined();
    expect(findMapping(p, 0, ['RB', 'LB'], 'single')?.actions).toEqual([{ key: 'Tab' }]);
  });

  it('upsert replaces the same slot and appends new ones', () => {
    let p = upsertMapping(base(), { trigger: 'A', actions: [{ key: 'Enter' }] });
    expect(p.mappings.filter((m) => m.trigger === 'A' && !m.layer)).toHaveLength(1);
    expect(findMapping(p, 0, 'A', 'single')?.actions).toEqual([{ key: 'Enter' }]);
    p = upsertMapping(p, { trigger: 'A', activator: 'double', actions: [{ key: 'R' }] });
    expect(p.mappings).toHaveLength(6);
  });

  it('remove only touches one activator', () => {
    const p = removeMapping(base(), 0, 'B', 'long');
    expect(p.mappings).toHaveLength(4);
    expect(findMapping(p, 0, 'A', 'single')).toBeDefined();
  });

  it('tidy drops defaults', () => {
    const m: Mapping = { layer: 0, trigger: 'A', activator: 'single', mode: 'hold', actions: [], description: '' };
    expect(tidyMapping(m)).toEqual({ trigger: 'A' });
  });
});

describe('layers', () => {
  it('collects ids from layers, mappings and jumps', () => {
    expect(layerIds(base())).toEqual([0, 1]);
  });

  it('inherits base mappings unless overridden or disabled', () => {
    const p = base();
    expect(effectiveMappings(p, 1, 'A')).toMatchObject({ inherited: false });
    expect(effectiveMappings(p, 1, 'B')).toMatchObject({ inherited: true });
    const noInherit: Profile = { ...p, layers: [{ id: 1, inherit: false }] };
    expect(effectiveMappings(noInherit, 1, 'B').mappings).toHaveLength(0);
    const notB: Profile = { ...p, layers: [{ id: 1, not_inherited: ['B'] }] };
    expect(effectiveMappings(notB, 1, 'B').mappings).toHaveLength(0);
  });

  it('adds, removes and pastes layers', () => {
    const { profile, id } = addLayer(base());
    expect(id).toBe(2);
    const removed = removeLayer(base(), 1);
    expect(layerIds(removed)).toEqual([0]);
    expect(removed.mappings.find((m) => m.trigger === 'LB')).toEqual({ trigger: 'LB' });
    const pasted = pasteLayer(profile, copyLayer(base(), 0), 2);
    expect(pasted.mappings.filter((m) => m.layer === 2)).toHaveLength(4);
  });

  it('takes analog routes along', () => {
    const p: Profile = {
      ...base(),
      analog: [
        { layer: 0, source: 'right_stick', target: 'mouse' },
        { layer: 3, source: 'gyro', target: 'mouse' },
      ],
    };
    expect(layerIds(p)).toEqual([0, 1, 3]);
    expect(removeLayer(p, 3).analog).toEqual([{ layer: 0, source: 'right_stick', target: 'mouse' }]);
    expect(clearLayer(p, 3).analog).toHaveLength(1);
    expect('analog' in clearLayer(clearLayer(p, 3), 0)).toBe(false);
    const pasted = pasteLayer(p, copyLayer(p, 0), 1);
    expect(pasted.analog?.filter((r) => r.layer === 1)).toEqual([{ layer: 1, source: 'right_stick', target: 'mouse' }]);
    expect(pasted.analog).toHaveLength(3);
  });
});

describe('shortcuts and labels', () => {
  it('lists multi-control triggers', () => {
    expect(shortcuts(base(), 0)).toHaveLength(1);
    expect([...shortcutMembers(base(), 0)].sort()).toEqual(['LB', 'RB']);
  });

  it('summarises mappings', () => {
    setLanguage('en');
    expect(mappingSummary({ trigger: 'A', actions: [{ key: 'LeftCtrl' }, { key: 'C' }] }, lbl)).toBe('LeftCtrl + C');
    expect(mappingSummary({ trigger: 'A', mode: 'turbo', actions: [{ mouse: 'left' }] }, lbl)).toBe('⚡ Mouse left');
    expect(mappingSummary({ trigger: 'LB', jump: { layer: 1 } }, lbl)).toBe('→ Shift 1');
    expect(mappingSummary({ trigger: 'X' }, lbl)).toBe('⊘');
  });

  it('label actions in the app language', () => {
    setLanguage('de');
    expect(actionLabel({ mouse: 'left' })).toBe('Maus links');
    expect(actionLabel({ command: 'remap_off' })).toBe('Remapping aus');
    expect(actionLabel({ touch: 'swipe_up' })).toBe('Touch: nach oben wischen');
    setLanguage('en');
    expect(actionLabel({ mouse: 'x1', action: 'down' })).toBe('Mouse x1 ▼');
    expect(actionLabel({ command: { launch: { path: 'C:\\Games\\a.exe' } } })).toBe('Run a.exe');
    expect(actionLabel({ rumble: 200 })).toBe('Rumble 200 ms');
  });
});

describe('native outputs', () => {
  it('reproduces what a control does unmapped', () => {
    expect(nativeAction('A')).toEqual({ pad: 'A' });
    expect(nativeAction('DpadUp')).toEqual({ pad: 'DpadUp' });
    expect(nativeAction('LeftStickLeft')).toEqual({ stick: 'left', x: -1, y: 0 });
    expect(nativeAction('RT')).toEqual({ trigger: 'right' });
    expect(nativeAction('LTFull')).toEqual({ trigger: 'left' });
    expect(nativeAction('Key:Space')).toEqual({ key: 'Space' });
    expect(nativeAction('Mouse:X1')).toEqual({ mouse: 'x1' });
    expect(nativeAction('Wheel:Down')).toEqual({ wheel: 'down' });
    expect(nativeAction('RTHigh')).toEqual({ trigger: 'right' });
    expect(nativeAction('LeftStickUpLow')).toEqual({ stick: 'left', x: 0, y: 1 });
  });

  it('has none for controls without a digital output', () => {
    for (const c of ['TouchCenter', 'GyroUp', 'MouseMove:Left', 'RightStickMed', ['LB', 'RB']]) {
      expect(nativeAction(c)).toBeNull();
    }
    expect(nativeAction('Touchpad')).toEqual({ pad: 'Touchpad' });
  });
});

describe('layer inheritance', () => {
  it('declares a layer when its inheritance changes', () => {
    const p = setLayerInherit(base(), 1, false);
    expect(p.layers).toEqual([{ id: 1, inherit: false }]);
    expect(layerInheritance(p, 1).inherit).toBe(false);
    expect(effectiveMappings(p, 1, 'B').mappings).toEqual([]);
    expect(layerInheritance(base(), 2)).toEqual({ inherit: true, notInherited: [] });
  });

  it("keeps Main's keyboard and mouse mappings out of an unlock layer", () => {
    const kbm: Profile = {
      ...base(),
      mappings: [...base().mappings, { trigger: 'Key:W', actions: [{ key: 'Up' }] }, { trigger: 'Mouse:Left', actions: [{ pad: 'A' }] }],
    };
    let p = setLayerUnlock(kbm, 1, 'keyboard');
    expect(layerUnlock(p, 1)).toBe('keyboard');
    expect(effectiveMappings(p, 1, 'Key:W').mappings).toEqual([]);
    expect(effectiveMappings(p, 1, 'Mouse:Left').inherited).toBe(true);
    expect(effectiveMappings(p, 1, 'B').inherited).toBe(true);
    p = setLayerUnlock(p, 1, 'all');
    expect(effectiveMappings(p, 1, 'Mouse:Left').mappings).toEqual([]);
    p = setLayerUnlock(p, 1, undefined);
    expect(p.layers).toEqual([{ id: 1, inherit: true }]);
  });

  it('keeps single triggers of Main out of a layer and lets them back in', () => {
    let p = setTriggerInherited(base(), 1, ['RB', 'LB'], false);
    expect(p.layers).toEqual([{ id: 1, inherit: true, not_inherited: [['LB', 'RB']] }]);
    expect(effectiveMappings(p, 1, ['LB', 'RB']).mappings).toEqual([]);
    expect(effectiveMappings(p, 1, 'B').inherited).toBe(true);
    p = setTriggerInherited(p, 1, ['LB', 'RB'], true);
    expect(p.layers).toEqual([{ id: 1, inherit: true }]);
  });

  it('lists what Main maps, each trigger once', () => {
    const p = upsertMapping(base(), { trigger: 'A', activator: 'long', actions: [{ key: 'Q' }] });
    expect(inheritableTriggers(p).map(comboKey)).toEqual(['A', 'B', 'LB', 'LB+RB']);
  });
});

describe('profile timing', () => {
  it('sets and resets single values', () => {
    let p = setProfileTiming(base(), 'combo_window_ms', 80);
    expect(p.timing).toEqual({ combo_window_ms: 80 });
    p = setProfileTiming(p, 'turbo_ms', 100);
    p = setProfileTiming(p, 'combo_window_ms', undefined);
    expect(p.timing).toEqual({ turbo_ms: 100 });
    expect(setProfileTiming(p, 'turbo_ms', undefined).timing).toBeUndefined();
  });
});

describe('virtual devices', () => {
  it('change one device and keep the others', () => {
    const sticks = { left: { initial: 0.5 } };
    let p: Profile = { ...base(), virtual: { sticks } };
    p = setVirtualDevice(p, 'keyboard', { repeat: 'fast' });
    expect(p.virtual).toEqual({ sticks, keyboard: { repeat: 'fast' } });
    p = setVirtualDevice(p, 'keyboard', undefined);
    expect(p.virtual).toEqual({ sticks });
    p = setVirtualDevice(p, 'sticks', undefined);
    expect('virtual' in p).toBe(false);
  });
});

describe('commands', () => {
  it('every command the picker lists has a label', () => {
    for (const c of [...SIMPLE_COMMANDS, ...RADIAL_COMMANDS, ...POWER_COMMANDS]) expect(has(`picker.${c}`), c).toBe(true);
  });
});

describe('light bar', () => {
  it("sets or drops the profile's own, or a layer's", () => {
    const light = { mode: 'solid' as const, colors: ['#ff0000'] };
    const p = setLight({ name: 'T', mappings: [], layers: [{ id: 1 }] }, 0, light);
    expect(p.lightbar).toEqual(light);
    expect('lightbar' in setLight(p, 0, undefined)).toBe(false);
    const q = setLight(p, 1, light);
    expect(q.layers?.[0]).toEqual({ id: 1, lightbar: light });
    expect(setLight(q, 1, undefined).layers?.[0]).toEqual({ id: 1 });
  });
});

describe('segments', () => {
  it("counts a macro's parts", () => {
    expect(segmentCount([{ key: 'A' }, { break: true }, { key: 'B' }])).toBe(2);
    expect(segmentCount([{ break: true }])).toBe(1);
  });
});

describe('editing one mapping', () => {
  // Layer 1 inherits A from Main; B is mapped on the layer itself.
  const p = (): Profile => ({
    name: 'Test',
    mappings: [
      { trigger: 'A', actions: [{ key: 'Space' }] },
      { trigger: 'B', layer: 1, actions: [{ key: 'E' }] },
    ],
  });

  it('finds the own mapping, else the inherited one', () => {
    expect(mappingAt(p(), 1, 'B', 'single').own?.actions).toEqual([{ key: 'E' }]);
    expect(mappingAt(p(), 1, 'A', 'single')).toEqual({ inherited: p().mappings[0] });
    expect(mappingAt(p(), 1, 'A', 'long')).toEqual({ inherited: undefined });
  });

  it('overrides an inherited mapping on the layer, leaving Main alone', () => {
    const next = editMappingIn(p(), 1, 'A', 'single', (m) => (m.mode = 'turbo'));
    expect(findMapping(next, 1, 'A', 'single')).toEqual({ trigger: 'A', layer: 1, mode: 'turbo', actions: [{ key: 'Space' }] });
    expect(findMapping(next, 0, 'A', 'single')).toEqual({ trigger: 'A', actions: [{ key: 'Space' }] });
  });

  it('removes a mapping an edit empties, unless it should block', () => {
    const cleared = editMappingIn(p(), 1, 'B', 'single', (m) => (m.actions = []));
    expect(findMapping(cleared, 1, 'B', 'single')).toBeUndefined();
    const blocking = editMappingIn(p(), 1, 'B', 'single', (m) => (m.actions = []), true);
    expect(findMapping(blocking, 1, 'B', 'single')).toEqual({ trigger: 'B', layer: 1 });
    expect(isBlocked(findMapping(blocking, 1, 'B', 'single'))).toBe(true);
    expect(isBlocked(findMapping(p(), 1, 'B', 'single'))).toBe(false);
  });

  it("gives turbo the control's own output and takes it back in hold mode", () => {
    const native = nativeAction('A');
    const m: Mapping = { trigger: 'A' };
    applyMode(m, 'turbo', native);
    expect(m).toEqual({ trigger: 'A', mode: 'turbo', actions: [{ pad: 'A' }] });
    applyMode(m, 'hold', native);
    expect(m).toEqual({ trigger: 'A', mode: 'hold', actions: [] });
    const own: Mapping = { trigger: 'A', actions: [{ key: 'E' }] };
    applyMode(own, 'toggle', native);
    expect(own.actions).toEqual([{ key: 'E' }]);
  });

  it('knows macro modes and single controls', () => {
    expect(['once', 'repeat', 'while_held', 'hold', 'turbo'].map((m) => isMacroMode(m as never))).toEqual([true, true, true, false, false]);
    expect(singleControl('A')).toBe('A');
    expect(singleControl(['LB', 'A'])).toBeNull();
  });
});
