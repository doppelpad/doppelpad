import { describe, expect, it } from 'vitest';
import type { RawInput } from '../types';
import { STEPS, buildLayout, detect, modelKey, settled } from './identify';

const rest: RawInput = { buttons: [false, false, false], axes: [0, 0, -1], hats: [null] };
const at = (patch: Partial<RawInput>): RawInput => ({ ...rest, ...patch });

describe('identify wizard', () => {
  it('keys models by vendor and product id', () => {
    expect(modelKey(0x79, 0x6)).toBe('0079:0006');
  });

  it('finds the pressed button, hat direction or axis', () => {
    const a = STEPS.findIndex((s) => s.kind === 'button' && s.button === 'A');
    expect(detect(STEPS[a], rest, at({ buttons: [false, true, false] }))).toEqual({ source: { kind: 'button', index: 1 } });
    expect(detect(STEPS[a], rest, at({ hats: [4] }))).toEqual({ source: { kind: 'hat', index: 0, dir: 'down' } });
    expect(detect(STEPS[a], rest, at({ hats: [3] })), 'diagonal').toBeNull();
    expect(detect(STEPS[a], rest, at({ axes: [0.2, 0, -1] })), 'too little').toBeNull();
  });

  it('prefers the axis for triggers', () => {
    const lt = STEPS.findIndex((s) => s.kind === 'trigger');
    const pulled = at({ buttons: [true, false, false], axes: [0, 0, 1] });
    expect(detect(STEPS[lt], rest, pulled)).toEqual({ source: { kind: 'axis', index: 2, rest: -1, full: 1 } });
  });

  it('records stick direction', () => {
    const up = STEPS.findIndex((s) => s.kind === 'stick' && s.axis === 'LeftY');
    expect(detect(STEPS[up], rest, at({ axes: [0, -1, -1] }))).toEqual({ stick: 1, invert: true });
  });

  it('waits until everything rests again', () => {
    expect(settled(rest, at({ axes: [0.1, 0, -0.9] }))).toBe(true);
    expect(settled(rest, at({ buttons: [true, false, false] }))).toBe(false);
    expect(settled(rest, at({ axes: [0, 0, 0] }))).toBe(false);
  });

  it('builds a layout of the hits', () => {
    const hits = STEPS.map(() => null as ReturnType<typeof detect>);
    hits[0] = { source: { kind: 'button', index: 2 } };
    hits[6] = { source: { kind: 'axis', index: 2, rest: -1, full: 1 } };
    hits[STEPS.length - 1] = { stick: 3, invert: true };
    expect(buildLayout('0079:0006', hits)).toEqual({
      device: '0079:0006',
      buttons: [{ button: 'A', from: { kind: 'button', index: 2 } }],
      triggers: [{ trigger: 'left', from: { kind: 'axis', index: 2, rest: -1, full: 1 } }],
      sticks: [{ axis: 'RightY', from: 3, invert: true }],
    });
  });
});
