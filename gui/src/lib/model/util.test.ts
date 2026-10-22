import { describe, expect, it } from 'vitest';
import type { Mapping, Profile } from '../types';
import { declaredLayer, patchLayer, setVirtualDevice } from './layers';
import { tidyMapping } from './profile';
import { clamp, clone, dropUndefined, nonEmpty, roundTo, sameJson, setOrDrop } from './util';

describe('model utilities', () => {
  it('clamps and rounds', () => {
    expect([clamp(-1, 0, 1), clamp(0.5, 0, 1), clamp(2, 0, 1)]).toEqual([0, 0.5, 1]);
    expect(roundTo(0.12345, 2)).toBe(0.12);
    expect(roundTo(0.7071067, 3)).toBe(0.707);
  });

  it('copies and compares JSON data', () => {
    const a = { x: [1, { y: 2 }] };
    const b = clone(a);
    expect(b).toEqual(a);
    expect(b.x).not.toBe(a.x);
    expect(sameJson(a, b)).toBe(true);
    expect(sameJson({ x: 1, y: 2 }, { y: 2, x: 1 })).toBe(false);
  });

  it('sets or drops a field in place', () => {
    const o: { a?: number; b?: number; c?: number } = { a: 1, b: 2 };
    expect(Object.keys(setOrDrop(o, 'a', 5))).toEqual(['a', 'b']);
    expect(setOrDrop(o, 'a', undefined)).toEqual({ b: 2 });
    expect(Object.keys(setOrDrop(o, 'c', 3))).toEqual(['a', 'b', 'c']);
    expect(o).toEqual({ a: 1, b: 2 });
  });

  it('drops undefined fields and empty values', () => {
    expect(Object.keys(dropUndefined({ a: 1, b: undefined, c: null }))).toEqual(['a', 'c']);
    expect(nonEmpty({})).toBeUndefined();
    expect(nonEmpty([])).toBeUndefined();
    expect(nonEmpty([1])).toEqual([1]);
  });

  it('tidies every unset mapping field', () => {
    const m: Mapping = { trigger: 'A', layer: 0, random_ms: undefined, turbo_ms: undefined, jump: undefined };
    expect(Object.keys(tidyMapping(m))).toEqual(['trigger']);
  });
});

describe('layers', () => {
  const p: Profile = { name: 'p', mappings: [], layers: [{ id: 2, name: 'Aim', inherit: true }] };

  it('finds and patches declared layers, declaring new ones', () => {
    expect(declaredLayer(p, 2)?.name).toBe('Aim');
    expect(declaredLayer(p, 3)).toBeUndefined();
    const renamed = patchLayer(p, 2, (l) => ({ ...l, name: 'Run' }));
    expect(renamed.layers).toEqual([{ id: 2, name: 'Run', inherit: true }]);
    const added = patchLayer(p, 3, (l) => ({ ...l, inherit: false, not_inherited: [] }));
    expect(added.layers?.[1]).toEqual({ id: 3, inherit: false });
  });

  it('drops the virtual devices when the last one goes', () => {
    const withSticks = setVirtualDevice(p, 'sticks', { left: { speed: 1 } });
    expect(withSticks.virtual).toEqual({ sticks: { left: { speed: 1 } } });
    expect('virtual' in setVirtualDevice(withSticks, 'sticks', undefined)).toBe(false);
  });
});
