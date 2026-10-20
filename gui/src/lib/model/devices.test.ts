import { describe, expect, it } from 'vitest';
import type { PadFamily } from '../types';
import { drawingsFor } from './devices';

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
