import { describe, expect, it } from 'vitest';
import type { AnalogMapping, Profile, StickConfig, TriggerConfig, VirtualStick } from '../types';
import defaultsJson from './fixtures/defaults.json';
import examples from './fixtures/examples.json';
import fixture from './fixtures/profile.json';
import {
  addCurvePoint,
  addRoute,
  applyCurve,
  curvePreset,
  curveToGraph,
  graphToCurve,
  layerOverrides,
  layerRoutes,
  moveCurvePoint,
  newRoute,
  overrideRoute,
  removeCurvePoint,
  removeRoute,
  resetLayerPart,
  rotate,
  routeFields,
  routeTargets,
  setStick,
  setTrigger,
  setVirtualStick,
  shapeStick,
  shapeTrigger,
  sourcePatch,
  stickConfig,
  stickResponse,
  triggerConfig,
  updateRoute,
  virtualRamp,
  virtualStickConfig,
} from './analog';
import { setStickLevel, setTriggerLevel, type ZoneDefaults } from './zones';

// Rust's defaults, as the catalog command sends them.
const def = defaultsJson as unknown as {
  stick: Required<StickConfig>;
  trigger: Required<TriggerConfig>;
  analog: Required<AnalogMapping>;
  virtual_stick: Required<VirtualStick>;
};

const zd: ZoneDefaults = {
  stickLow: def.stick.zones.low ?? 0.24,
  triggerLow: def.trigger.zones.low ?? 0.03,
  fullPull: def.trigger.full_pull,
};

const empty = (): Profile => ({ name: 'Test', mappings: [] });
const close = (a: number, b: number) => expect(Math.abs(a - b)).toBeLessThan(1e-6);

describe('stick and trigger settings', () => {
  it('fill in the defaults', () => {
    expect(stickConfig(empty(), 'left', def.stick)).toEqual(def.stick);
    const shooter = examples['shooter-advanced'] as unknown as Profile;
    expect(stickConfig(shooter, 'right', def.stick).curve).toEqual([[0.3, 0.15], [0.7, 0.55]]);
    expect(triggerConfig(shooter, 'right', def.trigger).zones.low).toBe(0.1);
  });

  it('are written like Rust writes them, zones included', () => {
    let p = empty();
    p = setStick(p, 'left', { deadzone: 0.08, outer: 0.95, anti_deadzone: 0.15, sensitivity: 1.2 }, def.stick);
    let left = setStickLevel(def.stick.zones, 'low', 0, 0.25, zd);
    left = setStickLevel(left, 'low', 1, 0.375, zd);
    left = setStickLevel(left, 'high', 'both', 0.75, zd);
    p = setStick(p, 'left', { invert_y: true, rotation: 45, zones: left, diagonals: true, direction_width: 20 }, def.stick);
    const right = setStickLevel(def.stick.zones, 'low', 'both', 0.25, zd);
    p = setStick(p, 'right', { deadzone: 0.06, curve: [[0.3, 0.15], [0.7, 0.55]], zones: right }, def.stick);
    p = setTrigger(p, 'left', { deadzone: 0.05, full_pull: 0.9, zones: setTriggerLevel(def.trigger.zones, 'low', 0.0625) }, def.trigger);
    const rt = setTriggerLevel(setTriggerLevel(def.trigger.zones, 'low', 0.125), 'med', 0.5);
    p = setTrigger(p, 'right', { full_pull: 0.875, zones: rt }, def.trigger);
    expect(p.sticks).toEqual(fixture.sticks);
    expect(p.triggers).toEqual(fixture.triggers);
  });

  it('drop a low zone set back to its default', () => {
    let p = setStick(empty(), 'left', { zones: setStickLevel(def.stick.zones, 'low', 'both', 0.3, zd) }, def.stick);
    expect(p.sticks?.left?.zones).toEqual({ low: 0.3 });
    p = setStick(p, 'left', { zones: setStickLevel(p.sticks?.left?.zones, 'low', 'both', 0.24, zd) }, def.stick);
    expect('sticks' in p).toBe(false);
  });

  it('include how buttons push the virtual sticks', () => {
    let p: Profile = { ...empty(), virtual: { keyboard: { repeat: 40 } } };
    p = setVirtualStick(p, 'left', { initial: 0.25, speed: 1, acceleration: 0.5, multiplier_y: 2 }, def.virtual_stick);
    p = setVirtualStick(p, 'right', { curve: [[0.5, 0.25]] }, def.virtual_stick);
    expect(p.virtual).toEqual(fixture.virtual);
    const left = virtualStickConfig(p, 'left', def.virtual_stick);
    expect(virtualRamp(left, 0, 1)).toBe(0.25);
    expect(virtualRamp(left, 0.5, 1)).toBeCloseTo(0.8125);
    expect(virtualRamp(left, 0.5, 2)).toBe(1);
    expect(virtualRamp(def.virtual_stick, 3, 1)).toBe(1);
    // Back to the defaults: the section disappears again.
    p = setVirtualStick(p, 'left', def.virtual_stick, def.virtual_stick);
    p = setVirtualStick(p, 'right', def.virtual_stick, def.virtual_stick);
    expect(p.virtual).toEqual({ keyboard: { repeat: 40 } });
  });

  it('on a layer, keep only what differs from Main', () => {
    const fx = fixture as unknown as Profile;
    const base = { sticks: fx.sticks, triggers: fx.triggers, virtual: fx.virtual };
    let p: Profile = { ...empty(), ...base, layers: [{ id: 1, name: 'Aim', inherit: true }] };
    const mainRight = stickConfig(p, 'right', def.stick);
    p = setStick(p, 'right', { sensitivity: 0.5, deadzone: mainRight.deadzone }, def.stick, 1);
    p = setStick(p, 'right', { zones: setStickLevel(mainRight.zones, 'low', 'both', 0.125, zd) }, def.stick, 1);
    p = setTrigger(p, 'right', { full_pull: 0.75 }, def.trigger, 1);
    p = setVirtualStick(p, 'left', { max: 0.5, speed: 1 }, def.virtual_stick, 1);
    expect(p.layers?.[0]).toEqual(fx.layers?.[0]);
    expect(p.sticks).toEqual(fx.sticks);
    expect(stickConfig(p, 'right', def.stick, 1)).toEqual({ ...mainRight, sensitivity: 0.5, zones: { low: 0.125 } });
    expect(stickConfig(p, 'left', def.stick, 1)).toEqual(stickConfig(p, 'left', def.stick));
    expect(layerOverrides(p, 1, 'sticks', 'right')).toEqual(['sensitivity', 'zones']);

    // Setting Main's value again drops the field; resetting drops the rest.
    p = setStick(p, 'right', { sensitivity: mainRight.sensitivity }, def.stick, 1);
    expect(layerOverrides(p, 1, 'sticks', 'right')).toEqual(['zones']);
    p = resetLayerPart(resetLayerPart(p, 1, 'sticks', 'right'), 1, 'triggers', 'right');
    p = resetLayerPart(p, 1, 'virtual_sticks', 'left');
    expect(p.layers?.[0]).toEqual({ id: 1, name: 'Aim', inherit: true });

    // An undeclared layer is declared on first change.
    p = setTrigger(p, 'left', { deadzone: 0.2 }, def.trigger, 3);
    expect(p.layers?.find((l) => l.id === 3)).toEqual({ id: 3, inherit: true, triggers: { left: { deadzone: 0.2 } } });
  });

  it('are left out once both sides are back at their defaults', () => {
    let p = setStick(empty(), 'right', { deadzone: 0.1 }, def.stick);
    expect(p.sticks?.left).toEqual(def.stick);
    p = setStick(p, 'right', { deadzone: def.stick.deadzone }, def.stick);
    expect('sticks' in p).toBe(false);
    p = setTrigger(empty(), 'left', { full_pull: 0.5 }, def.trigger);
    expect('triggers' in setTrigger(p, 'left', { full_pull: def.trigger.full_pull }, def.trigger)).toBe(false);
  });

  it('stay identical when an edit changes nothing', () => {
    for (const [name, json] of Object.entries({ ...examples, fixture })) {
      const p = json as unknown as Profile;
      let edited = p;
      for (const side of ['left', 'right'] as const) {
        edited = setStick(edited, side, stickConfig(p, side, def.stick), def.stick);
        edited = setTrigger(edited, side, triggerConfig(p, side, def.trigger), def.trigger);
      }
      (p.analog ?? []).forEach((_, i) => (edited = updateRoute(edited, i, {})));
      expect(edited, name).toEqual(p);
    }
  });
});

describe('shaping mirrors the engine', () => {
  it('curve is piecewise linear', () => {
    const pts: [number, number][] = [[0.5, 0.25]];
    expect(applyCurve(pts, 0)).toBe(0);
    close(applyCurve(pts, 0.25), 0.125);
    close(applyCurve(pts, 0.75), 0.625);
    expect(applyCurve(pts, 1)).toBe(1);
    expect(applyCurve([], 0.3)).toBe(0.3);
  });

  it('stick deadzone is scaled, anti-deadzone lifts, sensitivity saturates', () => {
    const cfg = { ...def.stick, deadzone: 0.2 };
    expect(shapeStick([0.1, 0.1], cfg)).toEqual([0, 0]);
    close(shapeStick([0.6, 0], cfg)[0], 0.5);
    close(shapeStick([1, 0], cfg)[0], 1);
    expect(stickResponse(0.01, { ...def.stick, anti_deadzone: 0.25 })).toBeGreaterThan(0.25);
    expect(stickResponse(0.8, { ...def.stick, sensitivity: 2 })).toBe(1);
    close(stickResponse(0.5, { ...def.stick, outer: 0.5 }), 1);
  });

  it('rotation is counter-clockwise', () => {
    const [x, y] = rotate([1, 0], 90);
    close(x, 0);
    close(y, 1);
  });

  it('trigger rescales between deadzone and full pull', () => {
    const cfg = { ...def.trigger, deadzone: 0.1, full_pull: 0.9 };
    expect(shapeTrigger(0.05, cfg)).toBe(0);
    close(shapeTrigger(0.5, cfg), 0.5);
    expect(shapeTrigger(0.95, cfg)).toBe(1);
  });
});

describe('curve editing', () => {
  it('keeps points sorted and inside their neighbours', () => {
    let pts = addCurvePoint([], [0.7, 0.55]);
    pts = addCurvePoint(pts, [0.3, 0.15]);
    expect(pts).toEqual([[0.3, 0.15], [0.7, 0.55]]);
    expect(curvePreset(pts)).toBe('precise');
    expect(moveCurvePoint(pts, 0, [0.9, 1.4])).toEqual([[0.69, 1], [0.7, 0.55]]);
    expect(removeCurvePoint(pts, 0)).toEqual([[0.7, 0.55]]);
    expect(addCurvePoint(pts, [0.3, 0.5])).toEqual([[0.3, 0.5], [0.7, 0.55]]);
  });

  it('maps points to the response graph and back', () => {
    const cfg = { ...def.stick, deadzone: 0.2, outer: 0.8, anti_deadzone: 0.1, sensitivity: 0.5 };
    const g = curveToGraph([0.5, 0.5], cfg);
    close(g[0], 0.5);
    close(g[1], 0.275);
    close(g[1], stickResponse(g[0], { ...cfg, curve: [[0.5, 0.5]] }));
    expect(graphToCurve(g, cfg)).toEqual([0.5, 0.5]);
  });
});

describe('analog routes', () => {
  it('offer only the fields the engine reads', () => {
    expect(routeFields('right_stick', 'mouse')).toEqual(['curve', 'deadzone', 'smoothing_ms', 'consume']);
    expect(routeFields('left_stick', 'right_stick')).toEqual(['deadzone', 'anti_deadzone', 'consume']);
    expect(routeFields('mouse', 'right_stick')).toContain('decay_ms');
    expect(routeFields('gyro', 'mouse')).toEqual(['smoothing_ms']);
    expect(routeFields('touchpad', 'mouse')).toEqual(['trackball_friction']);
    expect(routeTargets('mouse')).toEqual(['right_stick', 'left_stick']);
    expect(sourcePatch({ target: 'mouse' }, 'mouse')).toEqual({ source: 'mouse', target: 'right_stick' });
    expect(sourcePatch({ target: 'mouse' }, 'gyro')).toEqual({ source: 'gyro' });
  });

  it('are written like Rust writes them', () => {
    let p = empty();
    p = addRoute(p, newRoute(def.analog, 0, { sensitivity: 1.2, vertical_ratio: 0.8, curve: 1.8, deadzone: 0.12 }));
    p = updateRoute(p, 0, { smoothing_ms: 10 });
    p = addRoute(p, newRoute(def.analog, 1, { source: 'gyro', target: 'right_stick', while_held: ['LT'] }));
    p = updateRoute(p, 1, { sensitivity: 2.5, invert_y: true, anti_deadzone: 0.2, consume: false });
    p = addRoute(p, newRoute(def.analog, 2, { source: 'mouse', target: 'left_stick', sensitivity: 1.5 }));
    p = updateRoute(p, 2, { invert_x: true, anti_deadzone: 0.2, smoothing_ms: 8, decay_ms: 60 });
    p = updateRoute(p, 2, { while_held: ['LB', 'Mouse:Right'] });
    p = addRoute(p, newRoute(def.analog, 0, { source: 'touchpad', target: 'mouse', trackball_friction: 0.5 }));
    expect(p.analog).toEqual(fixture.analog);
    // Rust leaves out an empty list.
    for (let i = 3; i >= 0; i--) p = removeRoute(p, i);
    expect('analog' in p).toBe(false);
  });

  it('inherit Main routes for sources a layer does not route', () => {
    const p = fixture as unknown as Profile;
    expect(layerRoutes(p, 0).map((r) => r.index)).toEqual([0, 3]);
    expect(layerRoutes(p, 1).map((r) => [r.index, r.inherited])).toEqual([[1, false], [0, true], [3, true]]);
    // Layer 2 does not inherit.
    expect(layerRoutes(p, 2).map((r) => r.index)).toEqual([2]);
    const over = overrideRoute(p, 0, 1);
    expect(layerRoutes(over, 1).map((r) => [r.route.source, r.inherited])).toEqual([
      ['gyro', false],
      ['right_stick', false],
      ['touchpad', true],
    ]);
  });
});
