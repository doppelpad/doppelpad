// Which drawings show a controller: its front, and its back with the shoulder buttons and
// exactly the back buttons that model has. Families cover several models, so the USB
// vendor and product IDs pick the variant.

import type { DeviceId } from '../../assets/devices/layouts';
import type { ControlName, LiveDevice } from '../types';

export interface Drawings {
  front: DeviceId;
  back: DeviceId;
}

const MICROSOFT = 0x045e;
const SONY = 0x054c;
const NINTENDO = 0x057e;

const DUALSENSE_EDGE = 0x0df2;
const SWITCH2_PRO = 0x2069;
/** Joy-Con 2 (R), Joy-Con 2 (L) and a combined Joy-Con 2 pair. */
const JOYCON2 = [0x2066, 0x2067, 0x2068];
/** Elite and Elite Series 2 (USB, Bluetooth). XInput hides the IDs and the paddles. */
const XBOX_ELITE = [0x02e3, 0x0b00, 0x0b05, 0x0b22];

/** Front drawings of controllers with motion sensors, whose gyro can be mapped. */
const MOTION_DRAWINGS: DeviceId[] = ['dualsense', 'dualsenseEdge', 'switchpro', 'switch2pro', 'joycon', 'joycon2'];

/** Whether the controller drawn by `drawing` has a gyro. */
export function hasMotion(drawing: DeviceId): boolean {
  return MOTION_DRAWINGS.includes(drawing);
}

/** Drawings for a controller; without one, the Xbox drawing with all four paddles. */
export function drawingsFor(dev: Pick<LiveDevice, 'family' | 'vendor_id' | 'product_id'> | null): Drawings {
  if (!dev) return { front: 'xbox', back: 'xboxEliteBack' };
  const is = (vid: number, pids: number[]) => dev.vendor_id === vid && pids.includes(dev.product_id);
  switch (dev.family) {
    case 'dual_sense':
      return is(SONY, [DUALSENSE_EDGE])
        ? { front: 'dualsenseEdge', back: 'dualsenseEdgeBack' }
        : { front: 'dualsense', back: 'dualsenseBack' };
    case 'dual_shock4':
      return { front: 'dualsense', back: 'dualsenseBack' };
    case 'switch_pro':
      return is(NINTENDO, [SWITCH2_PRO])
        ? { front: 'switch2pro', back: 'switch2proBack' }
        : { front: 'switchpro', back: 'switchproBack' };
    case 'joy_con':
      return { front: is(NINTENDO, JOYCON2) ? 'joycon2' : 'joycon', back: 'joyconBack' };
    case 'game_cube':
      return { front: 'gamecube', back: 'gamecubeBack' };
    case 'generic':
      return { front: 'generic', back: 'genericBack' };
    case 'xbox':
      return { front: 'xbox', back: is(MICROSOFT, XBOX_ELITE) ? 'xboxEliteBack' : 'xboxBack' };
  }
}

/** Stick deflection / trigger pull that counts as "pressed" (highlight and press-to-select). */
const STICK_SELECT = 0.65;
const TRIGGER_SELECT = 0.4;

/** The controls a controller's live state presses (`buttons`: the catalog's pad buttons). */
export function controlsFromPad(dev: LiveDevice, buttons: string[]): Set<ControlName> {
  const s = new Set<ControlName>();
  buttons.forEach((b, i) => {
    if ((dev.state.buttons >>> i) & 1) s.add(b);
  });
  const a = dev.state.axes;
  const stick = (name: string, x: number, y: number) => {
    if (y > STICK_SELECT) s.add(`${name}Up`);
    if (y < -STICK_SELECT) s.add(`${name}Down`);
    if (x < -STICK_SELECT) s.add(`${name}Left`);
    if (x > STICK_SELECT) s.add(`${name}Right`);
  };
  stick('LeftStick', a[0], a[1]);
  stick('RightStick', a[2], a[3]);
  if (a[4] > TRIGGER_SELECT) s.add('LT');
  if (a[5] > TRIGGER_SELECT) s.add('RT');
  return s;
}
