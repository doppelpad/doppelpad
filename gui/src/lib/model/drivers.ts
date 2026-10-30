// The drivers Doppelpad needs, as the settings and the welcome guide list them.

import type { LiveState } from '../types';

/**
 * The drivers Doppelpad uses and whether each is installed (none before the first live state).
 * On macOS (`mac`) also the permission keyboard and mouse need, while virtual controllers do not
 * count: macOS has none to install.
 */
export function driverStates(live: LiveState | null, mac: boolean) {
  if (!live) return [];
  const drivers = [
    { key: 'virtual_pads' as const, state: live.drivers.virtual_pads },
    { key: 'device_hiding' as const, state: live.drivers.device_hiding },
  ];
  return mac ? [...drivers.slice(1), { key: 'input_access' as const, state: live.drivers.input_access }] : drivers;
}

/** Whether a driver Doppelpad needs is missing. */
export const driverMissing = (live: LiveState | null, mac: boolean) => driverStates(live, mac).some((d) => !d.state.available);
