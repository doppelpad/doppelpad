// The app settings the window works with until the real ones arrive from the app.

import type { GuiSettings } from '../types';

/**
 * Rust's defaults (a test compares them), except that the welcome guide counts as done: it
 * shows only once the real settings say it is not.
 */
export const SETTINGS_PLACEHOLDER: GuiSettings = {
  language: 'en',
  stay_live: false,
  hook_buttons: true,
  rewasd_at_start: 'ask',
  osd: {
    enabled: true,
    remap: true,
    slot: true,
    layer: true,
    keep_layer: false,
    controller: true,
    battery: true,
    monitor: 0,
    corner: 'bottom_right',
    scale: 100,
    opacity: 80,
    seconds: 5,
  },
  pad_overlay: { corner: 'top_left', size: 70, opacity: 90, hide_after: 5 },
  mappings_overlay: { corner: 'top_right', opacity: 90 },
  welcome_done: true,
  show_notes: false,
};
