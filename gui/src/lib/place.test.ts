import { describe, expect, it } from 'vitest';
import { placePopup, pointBox, type Box } from './place';

const bounds: Box = { left: 0, top: 0, right: 800, bottom: 600 };
const button: Box = { left: 100, top: 100, right: 180, bottom: 130 };

describe('popup placement', () => {
  it('opens below its anchor, lined up with its left edge', () => {
    expect(placePopup(button, { width: 200, height: 150 }, bounds)).toEqual({ left: 100, top: 134, up: false, maxHeight: 458 });
  });

  it('opens above when it does not fit below and there is more room above', () => {
    const low: Box = { left: 100, top: 500, right: 180, bottom: 530 };
    expect(placePopup(low, { width: 200, height: 150 }, bounds)).toMatchObject({ top: 346, up: true, maxHeight: 488 });
    // Not enough room on either side: the larger one, with a scrolling list.
    const middle: Box = { left: 100, top: 250, right: 180, bottom: 280 };
    expect(placePopup(middle, { width: 200, height: 500 }, bounds)).toMatchObject({ up: false, top: 284, maxHeight: 308 });
  });

  it('keeps at least the least height it is given', () => {
    const bottom: Box = { left: 100, top: 560, right: 180, bottom: 590 };
    expect(placePopup(bottom, { width: 100, height: 50 }, { ...bounds, top: 500 }, { minHeight: 120 }).maxHeight).toBe(120);
  });

  it('lines up with the end or the centre, and stays inside the bounds', () => {
    expect(placePopup(button, { width: 200, height: 50 }, bounds, { align: 'end' }).left).toBe(8);
    expect(placePopup(button, { width: 40, height: 20 }, bounds, { align: 'center' }).left).toBe(120);
    expect(placePopup({ ...button, left: 760, right: 790 }, { width: 200, height: 50 }, bounds).left).toBe(592);
    expect(placePopup(button, { width: 900, height: 50 }, bounds).left).toBe(8);
  });

  it('opens at a point, e.g. a right click', () => {
    expect(placePopup(pointBox(300, 590), { width: 180, height: 100 }, bounds, { gap: 0, edge: 4 })).toMatchObject({
      left: 300,
      top: 490,
      up: true,
    });
  });
});
