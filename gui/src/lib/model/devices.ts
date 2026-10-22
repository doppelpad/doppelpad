// Which drawings show a controller: its front, and its back with the shoulder buttons and
// exactly the back buttons that model has. Families cover several models, so the USB
// vendor and product IDs pick the variant.

import { DEVICE_LAYOUTS, type DeviceId } from '../../assets/devices/layouts';
import type { ControlName, LiveDevice } from '../types';
import type { Side } from './analog';
import { SIDES } from './controls';

export interface Drawings {
  front: DeviceId;
  back: DeviceId;
}

const MICROSOFT = 0x045e;
const SONY = 0x054c;
const NINTENDO = 0x057e;
const VALVE = 0x28de;

// The Nintendo ids mirror `nintendo::pid` in crates/dp-platform/src/nintendo/mod.rs.
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
      // Steam Controller and Deck: their grips are paddles.
      return { front: 'xbox', back: is(MICROSOFT, XBOX_ELITE) || dev.vendor_id === VALVE ? 'xboxEliteBack' : 'xboxBack' };
  }
}

/**
 * The side to show for control `c`: the current one, unless only the other side's drawing
 * has it. The front shows no triggers and the back no face buttons, as on the controller.
 */
export function sideShowing(drawings: Drawings, current: 'front' | 'back', c: ControlName): 'front' | 'back' {
  const shows = (id: DeviceId) => DEVICE_LAYOUTS[id].controls.some((lc) => lc.control === c);
  const other = current === 'front' ? 'back' : 'front';
  return !shows(drawings[current]) && shows(drawings[other]) ? other : current;
}

/** Stick deflection / trigger pull that counts as "pressed" (highlight and press-to-select). */
const STICK_SELECT = 0.65;
const TRIGGER_SELECT = 0.4;

/** Where each side's stick (x, y) and trigger are in `PadState.axes`. */
export const AXES = {
  left: { x: 0, y: 1, trigger: 4 },
  right: { x: 2, y: 3, trigger: 5 },
} as const satisfies Record<Side, { x: number; y: number; trigger: number }>;

const stickIn = (axes: number[], side: Side): [number, number] => [axes[AXES[side].x], axes[AXES[side].y]];

/** A stick's deflection `[x, y]` (−1..1, up positive). */
export const stickAxes = (dev: LiveDevice, side: Side) => stickIn(dev.state.axes, side);

/** A trigger's pull (0..1). */
export const triggerAxis = (dev: LiveDevice, side: Side): number => dev.state.axes[AXES[side].trigger];

/** The pad buttons a controller holds, from the catalog's `names` (in bit order). */
export function padButtonsDown<T extends string>(dev: LiveDevice, names: T[]): T[] {
  return names.filter((_, i) => (dev.state.buttons >>> i) & 1);
}

/** Move a drawing's stick caps by the sticks' deflection in `axes` (as in `PadState`). */
export function moveStickCaps(svg: Element, axes: number[]) {
  for (const g of svg.querySelectorAll<SVGGElement>('[data-stick]')) {
    const [x, y] = stickIn(axes, g.getAttribute('data-stick') === 'Left' ? 'left' : 'right');
    g.querySelector('.stick-cap')?.setAttribute('transform', `translate(${(x * 14).toFixed(1)} ${(-y * 14).toFixed(1)})`);
  }
}

/** The controls a controller's live state presses (`buttons`: the catalog's pad buttons). */
export function controlsFromPad(dev: LiveDevice, buttons: string[]): Set<ControlName> {
  const s = new Set<ControlName>(padButtonsDown(dev, buttons));
  const stick = (side: Side) => {
    const name = SIDES[side].stick;
    const [x, y] = stickAxes(dev, side);
    if (y > STICK_SELECT) s.add(`${name}Up`);
    if (y < -STICK_SELECT) s.add(`${name}Down`);
    if (x < -STICK_SELECT) s.add(`${name}Left`);
    if (x > STICK_SELECT) s.add(`${name}Right`);
  };
  stick('left');
  stick('right');
  if (triggerAxis(dev, 'left') > TRIGGER_SELECT) s.add('LT');
  if (triggerAxis(dev, 'right') > TRIGGER_SELECT) s.add('RT');
  return s;
}
