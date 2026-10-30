import { describe, expect, it } from 'vitest';
import { DEVICE_LAYOUTS } from '../../assets/devices/layouts';
import type { LiveDevice, PadFamily } from '../types';
import { batteryTone, controlsFromPad, drawingsFor, hasMotion, sideShowing } from './devices';

const dev = (family: PadFamily, vendor_id: number, product_id: number) => ({ family, vendor_id, product_id });

describe('drawings for a controller', () => {
  it('tells the Switch Pro Controllers apart', () => {
    expect(drawingsFor(dev('switch_pro', 0x057e, 0x2009))).toEqual({ front: 'switchpro', back: 'switchproBack' });
    expect(drawingsFor(dev('switch_pro', 0x057e, 0x2069))).toEqual({ front: 'switch2pro', back: 'switch2proBack' });
    // Switch-style third-party pads look like the first Pro Controller
    expect(drawingsFor(dev('switch_pro', 0x0f0d, 0x0092)).front).toBe('switchpro');
  });

  it('shows back buttons only on models that have them', () => {
    expect(drawingsFor(dev('dual_sense', 0x054c, 0x0ce6)).back).toBe('dualsenseBack');
    expect(drawingsFor(dev('dual_sense', 0x054c, 0x0df2))).toEqual({ front: 'dualsenseEdge', back: 'dualsenseEdgeBack' });
    expect(drawingsFor(dev('dual_shock4', 0x054c, 0x09cc)).back).toBe('dualsenseBack');
    // XInput reports no product ID, and cannot read paddles anyway
    expect(drawingsFor(dev('xbox', 0x045e, 0)).back).toBe('xboxBack');
    expect(drawingsFor(dev('xbox', 0x045e, 0x0b00)).back).toBe('xboxEliteBack');
  });

  it('draws the C button only on Joy-Con 2', () => {
    expect(drawingsFor(dev('joy_con', 0x057e, 0x2008)).front).toBe('joycon');
    expect(drawingsFor(dev('joy_con', 0x057e, 0x2068)).front).toBe('joycon2');
  });

  it('offers all four paddles when no controller is connected', () => {
    expect(drawingsFor(null)).toEqual({ front: 'xbox', back: 'xboxEliteBack' });
  });
});

describe('the side that shows a control', () => {
  const pro = drawingsFor(dev('switch_pro', 0x057e, 0x2009));
  const dualSense = drawingsFor(dev('dual_sense', 0x054c, 0x0ce6));

  it('turns to the back for triggers, which hide behind the bumpers', () => {
    expect(sideShowing(pro, 'front', 'LT')).toBe('back');
    expect(sideShowing(dualSense, 'front', 'RT')).toBe('back');
  });

  it('turns to the front for face buttons', () => {
    expect(sideShowing(pro, 'back', 'A')).toBe('front');
    // L1 hides behind L2 on the DualSense's back
    expect(sideShowing(dualSense, 'back', 'LB')).toBe('front');
  });

  it('stays where the control shows, or where neither side draws it', () => {
    expect(sideShowing(pro, 'back', 'LB')).toBe('back');
    expect(sideShowing(pro, 'front', 'LB')).toBe('front');
    expect(sideShowing(pro, 'front', 'LeftStickUp')).toBe('front');
  });
});

describe('motion sensors', () => {
  it('offer gyro controls on the drawings of controllers that have them', () => {
    expect(hasMotion('dualsense')).toBe(true);
    expect(hasMotion('switchpro')).toBe(true);
    expect(hasMotion('xbox')).toBe(false);
    expect(hasMotion('gamecube')).toBe(false);
  });
});

describe('controls pressed on a controller', () => {
  const pad = (buttons: number, axes: number[]): LiveDevice => ({
    id: 'p',
    name: 'Pad',
    family: 'xbox',
    backend: 'xinput',
    vendor_id: 0,
    product_id: 0,
    state: { buttons, axes, motion: null, touch: [] },
    gyro_calibration: null,
    battery: null,
    raw: null,
    mirror: false,
  });

  it('reads buttons by catalog position, stick directions and triggers', () => {
    const held = controlsFromPad(pad(0b101, [-0.9, 0.1, 0.7, -0.8, 0.5, 0.3]).state, ['A', 'B', 'X']);
    expect([...held]).toEqual(['A', 'X', 'LeftStickLeft', 'RightStickDown', 'RightStickRight', 'LT']);
  });

  it('ignores small deflections and light pulls', () => {
    expect(controlsFromPad(pad(0, [0.6, -0.6, 0, 0, 0.4, 0.39]).state, ['A']).size).toBe(0);
  });
});

describe('battery tone', () => {
  const battery = (level: number, charging = false, wired = false) => ({ level, charging, wired });
  it('warns below 40 % and alarms below 20 %, unless charging or wired', () => {
    expect([battery(40), battery(39), battery(20), battery(19)].map(batteryTone)).toEqual([undefined, 'warning', 'warning', 'danger']);
    expect([battery(5, true), battery(5, false, true), null].map(batteryTone)).toEqual([undefined, undefined, undefined]);
  });
});

describe('device layouts', () => {
  it('are listed under their own id, each control once', () => {
    for (const [id, layout] of Object.entries(DEVICE_LAYOUTS)) {
      expect(layout.id).toBe(id);
      const names = layout.controls.map((c) => c.control);
      expect(new Set(names).size, id).toBe(names.length);
    }
  });
});
