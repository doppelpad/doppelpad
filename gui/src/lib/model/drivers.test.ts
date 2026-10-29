import { describe, expect, it } from 'vitest';
import type { LiveState } from '../types';
import { driverMissing, driverStates } from './drivers';

const ok = { available: true, error: null };
const missing = { available: false, error: 'not installed' };
const live = (drivers: Partial<LiveState['drivers']>) =>
  ({ drivers: { virtual_pads: ok, device_hiding: ok, input_access: ok, bluetooth: ok, ...drivers } }) as LiveState;

describe('drivers', () => {
  it('lists the virtual pads and hiding, on macOS hiding and the input permission instead', () => {
    expect(driverStates(live({}), false).map((d) => d.key)).toEqual(['virtual_pads', 'device_hiding']);
    expect(driverStates(live({}), true).map((d) => d.key)).toEqual(['device_hiding', 'input_access']);
    expect(driverStates(null, false)).toEqual([]);
  });

  it('a missing driver counts only where it is needed; Bluetooth never does', () => {
    expect(driverMissing(live({ virtual_pads: missing }), false)).toBe(true);
    expect(driverMissing(live({ virtual_pads: missing }), true)).toBe(false);
    expect(driverMissing(live({ bluetooth: missing }), false)).toBe(false);
  });
});
