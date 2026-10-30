import { describe, expect, it } from 'vitest';
import {
  firstFocus,
  navAction,
  nextChoice,
  nextFocus,
  padMode,
  PressRepeat,
  REPEAT_DELAY,
  REPEAT_EVERY,
  stepper,
  stepperOf,
  type Box,
} from './padnav';

const box = (left: number, top: number, w = 40, h = 20): Box => ({ left, top, right: left + w, bottom: top + h });

describe('spatial focus', () => {
  // A row of three buttons, a wide field below, and a button far down on the right.
  const row = [box(0, 0), box(50, 0), box(100, 0)];
  const field = box(0, 40, 140, 20);
  const far = box(300, 200);
  const all = [...row, field, far];

  it('moves to the neighbour in a row and stops at its end', () => {
    expect(nextFocus(row[0]!, all, 'right')).toBe(1);
    expect(nextFocus(row[1]!, all, 'right')).toBe(2);
    expect(nextFocus(row[2]!, all, 'left')).toBe(1);
    expect(nextFocus(row[0]!, all, 'left')).toBe(-1);
    expect(nextFocus(row[0]!, all, 'up')).toBe(-1);
  });

  it('prefers what lies straight ahead over what is nearer but off to the side', () => {
    expect(nextFocus(row[1]!, all, 'down')).toBe(3);
    expect(nextFocus(field, all, 'up'), 'the one above its centre').toBe(1);
    expect(nextFocus(field, all, 'down')).toBe(4);
    // Straight right 60 px away beats a box only 10 px away but 30 px lower.
    const aside = [box(0, 0), box(50, 50), box(100, 0)];
    expect(nextFocus(aside[0]!, aside, 'right')).toBe(2);
  });

  it('skips boxes that hold the current one', () => {
    const outer = box(0, 0, 200, 100);
    const inner = box(10, 10);
    expect(nextFocus(inner, [outer, inner], 'right')).toBe(-1);
  });

  it('starts nearest the top left corner', () => {
    expect(firstFocus([box(100, 3), box(0, 40), box(10, 0)])).toBe(2);
    expect(firstFocus([box(1365, 0), box(10, 8)]), 'not the window buttons').toBe(1);
    expect(firstFocus([])).toBe(-1);
  });
});

describe('controller actions', () => {
  it('maps the controls of handheld mode', () => {
    expect(navAction('DpadLeft')).toEqual({ kind: 'move', dir: 'left' });
    expect(navAction('LeftStickDown')).toEqual({ kind: 'move', dir: 'down' });
    expect(navAction('A')).toEqual({ kind: 'activate' });
    expect(navAction('B')).toEqual({ kind: 'back' });
    expect(navAction('RB')).toEqual({ kind: 'view', step: 1 });
    expect(navAction('LT')).toEqual({ kind: 'layer', step: -1 });
    expect(navAction('Start')).toEqual({ kind: 'save' });
    expect(navAction('X')).toBeNull();
  });

  it('driving the app and selecting controls exclude each other', () => {
    expect(padMode({ pad_navigation: true, hook_buttons: true })).toBe('navigate');
    expect(padMode({ pad_navigation: false, hook_buttons: true })).toBe('select');
    expect(padMode({ pad_navigation: false, hook_buttons: false })).toBe('off');
  });

  it('repeats held directions only', () => {
    const r = new PressRepeat();
    const held = new Set(['DpadDown', 'A']);
    expect(r.update(held, 0)).toEqual(['DpadDown', 'A']);
    expect(r.update(held, REPEAT_DELAY - 1)).toEqual([]);
    expect(r.update(held, REPEAT_DELAY)).toEqual(['DpadDown']);
    expect(r.update(held, REPEAT_DELAY + REPEAT_EVERY - 1)).toEqual([]);
    expect(r.update(held, REPEAT_DELAY + REPEAT_EVERY)).toEqual(['DpadDown']);
    expect(r.repeating).toBe(true);
    expect(r.update(new Set(['A']), 1000)).toEqual([]);
    expect(r.repeating).toBe(false);
    expect(r.update(new Set(), 1001)).toEqual([]);
    expect(r.update(new Set(['A']), 1002)).toEqual(['A']);
  });

  it('ignores what is held when the mode comes on', () => {
    const r = new PressRepeat();
    r.reset(new Set(['A']), 0);
    expect(r.update(new Set(['A']), 10)).toEqual([]);
  });
});

describe('stepping a list', () => {
  const choices = [{ value: 'a' }, { value: 'b', disabled: true }, { value: 'c' }];

  it('steps to the next choice that can be picked, round the end', () => {
    expect(nextChoice(choices, 'a', 1)).toBe('c');
    expect(nextChoice(choices, 'c', 1)).toBe('a');
    expect(nextChoice(choices, 'a', -1)).toBe('c');
    expect(nextChoice(choices, 'c', -1)).toBe('a');
  });

  it('starts at an end when no choice has the value, and stays without another choice', () => {
    expect(nextChoice(choices, 'x', 1)).toBe('a');
    expect(nextChoice(choices, 'x', -1)).toBe('c');
    expect(nextChoice([{ value: 'a' }], 'a', 1)).toBeUndefined();
    expect(nextChoice([], 'a', 1)).toBeUndefined();
  });

  it('a stepper registered for an element is found by it, until it goes', () => {
    const el = {} as HTMLElement;
    const steps: number[] = [];
    const action = stepper(el, (s) => steps.push(s));
    stepperOf(el)?.(1);
    action.update((s) => steps.push(10 * s));
    stepperOf(el)?.(-1);
    action.destroy();
    expect(stepperOf(el)).toBeUndefined();
    expect(steps).toEqual([1, -10]);
  });
});
