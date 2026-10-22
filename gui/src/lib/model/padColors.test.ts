import { describe, expect, it } from 'vitest';
import { padColor } from './padColors';

describe('tray colours of controllers', () => {
  it('picks the same palette colour by id as the tray', () => {
    // The same values as tray.rs's test.
    expect(padColor('xinput:0', {})).toBe('#a855f7');
    expect(padColor('hid:a', {})).toBe('#ec4899');
  });

  it('uses a chosen colour, but not a broken one', () => {
    expect(padColor('hid:a', { 'hid:a': '#123456' })).toBe('#123456');
    expect(padColor('hid:a', { 'hid:a': 'red' })).toBe('#ec4899');
  });
});
