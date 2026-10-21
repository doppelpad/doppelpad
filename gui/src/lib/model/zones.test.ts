import { describe, expect, it } from 'vitest';
import { setLanguage } from '../i18n/index.svelte';
import type { Profile, StickZones } from '../types';
import { controlLabel, touchControls, withZone, zoneFamily, zoneParts } from './controls';
import { nativeAction } from './profile';
import {
  isElliptic,
  setElliptic,
  setStickLevel,
  setTriggerLevel,
  stickThresholds,
  stickZoneAt,
  storedZones,
  triggerThresholds,
  triggerZoneAt,
  withZones,
  zoneProblem,
  type ZoneDefaults,
} from './zones';

const D: ZoneDefaults = { stickLow: 0.24, triggerLow: 0.03, fullPull: 0.99 };
const near = (a: number, b: number) => Math.abs(a - b) < 1e-9;

describe('zone controls', () => {
  it('split into base and zone', () => {
    expect(zoneParts('LeftStickUpHigh')).toEqual({
      base: 'LeftStickUp',
      zone: 'High',
      source: { kind: 'stick', side: 'left' },
    });
    expect(zoneParts('RTMed')).toEqual({ base: 'RT', zone: 'Med', source: { kind: 'trigger', side: 'right' } });
    expect(zoneParts('RightStickLow')?.base).toBe('RightStick');
    expect(zoneParts('LT')).toEqual({ base: 'LT', zone: null, source: { kind: 'trigger', side: 'left' } });
    expect(zoneParts('LeftStickLeft')?.zone).toBeNull();
    for (const c of ['LeftStick', 'LTFull', 'A', 'Key:L', 'Gyro']) expect(zoneParts(c)).toBeNull();
    expect(zoneParts('GyroLeanLeftHigh')).toEqual({ base: 'GyroLeanLeft', zone: 'High', source: { kind: 'gyro', side: 'left' } });
    expect(zoneParts('GyroLow')?.base).toBe('Gyro');
    expect(zoneParts('GyroUp')?.zone).toBeNull();
  });

  it('build zone controls and families', () => {
    expect(withZone('LeftStickDown', 'Low')).toBe('LeftStickDownLow');
    expect(withZone('RT', null)).toBe('RT');
    expect(zoneFamily('LT')).toEqual(['LT', 'LTLow', 'LTMed', 'LTHigh']);
    expect(zoneFamily('RightStick')).toEqual(['RightStickLow', 'RightStickMed', 'RightStickHigh']);
  });

  it('have readable names', () => {
    setLanguage('en');
    expect(controlLabel('RTHigh', null)).toBe('Right trigger · high zone');
    expect(controlLabel('RTHigh', 'dual_sense')).toBe('R2 · high zone');
    expect(controlLabel('LeftStickUpLow', null)).toBe('Left stick Up · low zone');
    expect(controlLabel('RightStickMed', null)).toBe('Right stick, any direction · medium zone');
    expect(controlLabel('TouchSwipe2Left', null)).toBe('Two-finger swipe left');
    expect(controlLabel('TrackpadRightTap', null)).toBe('Right half: tap');
    expect(controlLabel('TrackpadLeftUpLeft', null)).toBe('Left half: up-left');
    expect(controlLabel('GyroLeanLeft', null)).toBe('Gyro lean left');
    expect(controlLabel('GyroUpHigh', null)).toBe('Gyro Up · high zone');
    expect(touchControls(true)).toContain('TrackpadLeftClick');
    expect(touchControls(false)).not.toContain('TrackpadLeftClick');
  });
});

describe('zone thresholds', () => {
  it('split the range above low into thirds unless set', () => {
    const s = stickThresholds(undefined, D);
    expect(s.x[0]).toBe(0.24);
    expect(near(s.x[1], 0.24 + 0.76 / 3)).toBe(true);
    expect(near(s.y[2], 0.24 + (0.76 * 2) / 3)).toBe(true);
    const tr = triggerThresholds({ low: 0.1, high: 0.8 }, 0.94, D);
    expect(tr[0]).toBe(0.1);
    expect(near(tr[1], 0.1 + 0.84 / 3)).toBe(true);
    expect(tr[2]).toBe(0.8);
  });

  it('follow the engine band rule', () => {
    const t: [number, number, number] = [0.1, 0.4, 0.7];
    expect(triggerZoneAt(0.05, t)).toBeNull();
    expect(triggerZoneAt(0.4, t)).toBe('Med');
    expect(triggerZoneAt(0.39, t)).toBe('Low');
    expect(triggerZoneAt(1, t)).toBe('High');
    const e = stickThresholds({ low: [0.2, 0.5], med: [0.4, 0.7], high: [0.6, 0.9] }, D);
    expect(stickZoneAt(0.3, 0, e)).toBe('Low');
    expect(stickZoneAt(0, 0.3, e)).toBeNull();
    expect(stickZoneAt(0, -0.8, e)).toBe('Med');
  });

  it('edit one level or one axis', () => {
    let z = setStickLevel(undefined, 'high', 'both', 0.8, D);
    expect(z).toEqual({ high: 0.8 });
    z = setStickLevel(z, 'low', 1, 0.3, D);
    expect(z).toEqual({ high: 0.8, low: [0.24, 0.3] });
    expect(isElliptic(z)).toBe(true);
    z = setStickLevel(z, 'low', 0, 0.3, D);
    expect(z.low).toBe(0.3);
    expect(setElliptic({ med: [0.5, 0.6] }, false)).toEqual({ med: 0.5 });
    expect(setStickLevel({ med: 0.5 }, 'med', 'both', undefined, D)).toEqual({});
    expect(setTriggerLevel({ low: 0.1 }, 'high', 0.9)).toEqual({ low: 0.1, high: 0.9 });
  });

  it('are stored per stick or trigger', () => {
    const p: Profile = { mappings: [], triggers: { right: { full_pull: 0.95 } } };
    const q = withZones(p, { kind: 'trigger', side: 'right' }, { low: 0.2 });
    expect(q.triggers).toEqual({ right: { full_pull: 0.95, zones: { low: 0.2 } } });
    const r = withZones(q, { kind: 'trigger', side: 'right' }, {});
    expect(r.triggers).toEqual({ right: { full_pull: 0.95 } });
    const s = withZones(p, { kind: 'stick', side: 'left' }, { med: [0.4, 0.5] });
    expect(s.sticks).toEqual({ left: { zones: { med: [0.4, 0.5] } } });
  });

  it('can be a layer’s own', () => {
    const rt = { kind: 'trigger', side: 'right' } as const;
    const p: Profile = { mappings: [], triggers: { right: { zones: { low: 0.1 } } }, layers: [{ id: 1, inherit: true }] };
    const q = withZones(p, rt, { low: 0.3 }, 1);
    expect(q.triggers).toEqual(p.triggers);
    expect(q.layers).toEqual([{ id: 1, inherit: true, triggers: { right: { zones: { low: 0.3 } } } }]);
    expect(storedZones(q, rt, 1)).toEqual({ low: 0.3 });
    expect(storedZones(q, rt, 2)).toEqual({ low: 0.1 });
    expect(withZones(q, rt, { low: 0.1 }, 1).layers).toEqual([{ id: 1, inherit: true }]);
  });

  it('are elliptic per layer', () => {
    // The zone editor shows separate axes for a layer whose own zones are elliptic.
    const ls = { kind: 'stick', side: 'left' } as const;
    const p: Profile = { mappings: [], sticks: { left: { zones: { low: 0.2 } } }, layers: [{ id: 1 }] };
    const q = withZones(p, ls, { low: [0.2, 0.4] }, 1);
    expect(isElliptic(storedZones(q, ls, 1) as StickZones)).toBe(true);
    expect(isElliptic(storedZones(q, ls, 0) as StickZones)).toBe(false);
  });

  it('report unusable thresholds', () => {
    expect(zoneProblem([0.2, 0.5, 0.8])).toBeNull();
    expect(zoneProblem([0.5, 0.4, 0.8])).toBe('zones.err_order');
    expect(zoneProblem([0, 0.4, 0.8])).toBe('zones.err_range');
    expect(zoneProblem([0.2, 0.4, 1.2])).toBe('zones.err_range');
  });
});

describe('diagonal directions', () => {
  it('have zones like the cardinals', () => {
    expect(zoneParts('LeftStickUpLeftHigh')).toEqual({
      base: 'LeftStickUpLeft',
      zone: 'High',
      source: { kind: 'stick', side: 'left' },
    });
    expect(zoneParts('RightStickDownRight')?.zone).toBeNull();
  });

  it('reproduce a diagonal stick push', () => {
    const a = nativeAction('LeftStickUpLeft');
    expect(a && 'stick' in a && [Math.round(a.x * 100), Math.round(a.y * 100)]).toEqual([-71, 71]);
    expect(nativeAction('TouchDownRight')).toBeNull();
  });
});
