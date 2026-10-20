// All device drawings with their label layouts, by id.

import { dualsense } from './dualsense';
import { gamecube } from './gamecube';
import { generic } from './generic';
import { joycon } from './joycon';
import { mouse } from './mouse';
import { paddles } from './paddles';
import { switchpro } from './switchpro';
import type { DeviceLayout } from './types';
import { xbox } from './xbox';

export type { DeviceLayout, LabelSide, LayoutControl, Point } from './types';
export { CHIP_SIZE } from './types';

export const DEVICE_LAYOUTS = { dualsense, xbox, generic, switchpro, joycon, gamecube, paddles, mouse } satisfies Record<string, DeviceLayout>;

export type DeviceId = keyof typeof DEVICE_LAYOUTS;
