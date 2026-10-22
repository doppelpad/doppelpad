import { describe, expect, it } from 'vitest';
import { analogTabOf, fullPullBase, parseControl, zoneParts } from './controls';
import controls from './fixtures/controls.json';

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

describe('parsing control names', () => {
  it('splits every pad control the engine knows', () => {
    const kinds = new Map<string, number>();
    for (const c of controls) {
      const p = parseControl(c);
      expect(p, c).not.toBeNull();
      kinds.set(p!.kind, (kinds.get(p!.kind) ?? 0) + 1);
    }
    // 22 buttons (with LS, RS and the touchpad click), 2 × 8 directions, 2 × 9 × 3 zones, …
    expect(Object.fromEntries(kinds)).toEqual({
      button: 22,
      stick_dir: 16,
      stick_zone: 54,
      trigger_press: 2,
      trigger_full: 2,
      trigger_zone: 6,
      touch: 9,
      gesture: 15,
      trackpad: 22,
      gyro: 6,
      gyro_zone: 21,
    });
  });

  it('reads the parts of each kind', () => {
    expect(parseControl('LeftStickUpLeftHigh')).toEqual({ kind: 'stick_zone', side: 'left', dir: 'UpLeft', zone: 'High' });
    expect(parseControl('RightStickLow')).toEqual({ kind: 'stick_zone', side: 'right', dir: null, zone: 'Low' });
    expect(parseControl('RTFull')).toEqual({ kind: 'trigger_full', side: 'right' });
    expect(parseControl('GyroLeanLeftMed')).toEqual({ kind: 'gyro_zone', dir: 'LeanLeft', zone: 'Med' });
    expect(parseControl('TrackpadRightClick')).toEqual({ kind: 'trackpad', half: 'Right', part: 'Click' });
    expect(parseControl('TouchSwipe2Left')).toEqual({ kind: 'gesture', gesture: 'Swipe2Left' });
    expect(parseControl('Key:Space')).toEqual({ kind: 'key', key: 'Space' });
    expect(parseControl('MouseMove:DownRight')).toEqual({ kind: 'mouse_move', dir: 'DownRight' });
    expect(parseControl('MouseMove:High')).toEqual({ kind: 'mouse_zone', dir: null, zone: 'High' });
    expect(parseControl('MouseMove:')).toBeNull();
    expect(parseControl('LeftStick')).toEqual({ kind: 'button' });
  });

  it('keeps gyro zones apart from the sides', () => {
    expect(zoneParts('GyroUpHigh')).toEqual({ base: 'GyroUp', zone: 'High', source: { kind: 'gyro' } });
    expect(zoneParts('LTMed')).toEqual({ base: 'LT', zone: 'Med', source: { kind: 'trigger', side: 'left' } });
    expect(zoneParts('LTFull')).toBeNull();
  });
});
