import { describe, expect, it } from 'vitest';
import { polylinePoints } from '../svg';
import { parseDecimal, parseNonNegInt, parsePercent } from './numbers';

describe('typed numbers', () => {
  it('read decimals with a point or a comma', () => {
    expect(['1.5', '1,5', '-2', '3abc', 'x', ''].map(parseDecimal)).toEqual([1.5, 1.5, -2, 3, null, null]);
  });

  it('read percentages as fractions within 0..1', () => {
    expect(['50', '12,5', '150', '-3', 'x'].map(parsePercent)).toEqual([0.5, 0.125, 1, 0, null]);
  });

  it('read whole numbers of at least 0', () => {
    expect(['0', '42', '7.9', '-1', ''].map(parseNonNegInt)).toEqual([0, 42, 7, null, null]);
  });

  it('write polyline points', () => {
    expect(polylinePoints([[0, 1], [2.5, 3]])).toBe('0,1 2.5,3');
  });
});
