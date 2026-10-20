// All device drawings with their label layouts, by id.

import { dualsense } from './dualsense';
import { dualsenseBack } from './dualsenseBack';
import { dualsenseEdge } from './dualsenseEdge';
import { dualsenseEdgeBack } from './dualsenseEdgeBack';
import { gamecube } from './gamecube';
import { gamecubeBack } from './gamecubeBack';
import { generic } from './generic';
import { genericBack } from './genericBack';
import { joycon } from './joycon';
import { joycon2 } from './joycon2';
import { joyconBack } from './joyconBack';
import { mouse } from './mouse';
import { switch2pro } from './switch2pro';
import { switch2proBack } from './switch2proBack';
import { switchpro } from './switchpro';
import { switchproBack } from './switchproBack';
import type { DeviceLayout } from './types';
import { xbox } from './xbox';
import { xboxBack } from './xboxBack';
import { xboxEliteBack } from './xboxEliteBack';

export type { DeviceLayout, LabelSide, LayoutControl, Point } from './types';
export { CHIP_SIZE } from './types';

export const DEVICE_LAYOUTS = {
  dualsense,
  dualsenseEdge,
  xbox,
  generic,
  switchpro,
  switch2pro,
  joycon,
  joycon2,
  gamecube,
  dualsenseBack,
  dualsenseEdgeBack,
  xboxBack,
  xboxEliteBack,
  genericBack,
  switchproBack,
  switch2proBack,
  joyconBack,
  gamecubeBack,
  mouse,
} satisfies Record<string, DeviceLayout>;

export type DeviceId = keyof typeof DEVICE_LAYOUTS;
