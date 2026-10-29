// The controller hotkeys (app settings): which there are and how their buttons read, for the
// settings and for the mappings that clash with one.

import { controlLabel } from './controls';
import type { HotkeyField, PadButtonName, PadFamily } from '../types';

/** Every hotkey once: a field added to `Hotkeys` does not compile until it is listed here. */
const FIELDS = {
  slot1: true,
  slot2: true,
  slot3: true,
  slot4: true,
  remap: true,
  stop_macros: true,
  emergency_stop: true,
  pad_overlay: true,
  mappings_overlay: true,
  app_navigation: true,
} satisfies Record<HotkeyField, true>;

/** The controller hotkeys by their name in `hotkeys.*`, in the order the settings list them. */
export const HOTKEY_FIELDS = Object.keys(FIELDS) as HotkeyField[];

/** A hotkey's buttons as the controller names them: "L1 + R1 + Square". */
export const hotkeyLabel = (buttons: readonly PadButtonName[], family: PadFamily | null) =>
  buttons.map((b) => controlLabel(b, family)).join(' + ');
