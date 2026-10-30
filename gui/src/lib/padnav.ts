// Driving the app with a controller (handheld mode): what each control does, directions that
// repeat while held, and the spatial search for the next element to focus.

import type { ControlName, Dir } from './types';

export type NavAction =
  | { kind: 'move'; dir: Dir }
  | { kind: 'activate' }
  | { kind: 'back' }
  | { kind: 'view'; step: 1 | -1 }
  | { kind: 'layer'; step: 1 | -1 }
  | { kind: 'save' };

const move = (dir: Dir): NavAction => ({ kind: 'move', dir });

/** The D-pad and the left stick move the focus; A activates, B goes back, the bumpers switch views, the triggers layers, Start saves. */
const ACTIONS: Record<string, NavAction> = {
  DpadUp: move('up'),
  DpadDown: move('down'),
  DpadLeft: move('left'),
  DpadRight: move('right'),
  LeftStickUp: move('up'),
  LeftStickDown: move('down'),
  LeftStickLeft: move('left'),
  LeftStickRight: move('right'),
  A: { kind: 'activate' },
  B: { kind: 'back' },
  LB: { kind: 'view', step: -1 },
  RB: { kind: 'view', step: 1 },
  LT: { kind: 'layer', step: -1 },
  RT: { kind: 'layer', step: 1 },
  Start: { kind: 'save' },
};

/** What a controller press does while the controller drives the app. */
export const navAction = (c: ControlName): NavAction | null => ACTIONS[c] ?? null;

/** What the controller does in the focused window: drive the app, select the pressed control, or nothing. Driving wins. */
export function padMode(s: { pad_navigation: boolean; hook_buttons: boolean }): 'navigate' | 'select' | 'off' {
  return s.pad_navigation ? 'navigate' : s.hook_buttons ? 'select' : 'off';
}

/** A held direction acts at once, then after this long … */
export const REPEAT_DELAY = 400;
/** … again this often. */
export const REPEAT_EVERY = 120;

/** Turns the held controls into the ones that act now: each press once, held directions again and again. */
export class PressRepeat {
  private held = new Map<ControlName, { since: number; last: number }>();

  /** The controls that act at `now` (ms), given the ones held then. */
  update(held: Set<ControlName>, now: number): ControlName[] {
    for (const c of [...this.held.keys()]) if (!held.has(c)) this.held.delete(c);
    const out: ControlName[] = [];
    for (const c of held) {
      const h = this.held.get(c);
      if (!h) {
        this.held.set(c, { since: now, last: now });
        out.push(c);
      } else if (navAction(c)?.kind === 'move' && now - h.since >= REPEAT_DELAY && now - h.last >= REPEAT_EVERY) {
        h.last = now;
        out.push(c);
      }
    }
    return out;
  }

  /** Whether a held control may act again later (a direction). */
  get repeating(): boolean {
    return [...this.held.keys()].some((c) => navAction(c)?.kind === 'move');
  }

  /** Forget what is held, so nothing held now acts (the mode just came on). */
  reset(held: Set<ControlName> = new Set(), now = 0) {
    this.held = new Map([...held].map((c) => [c, { since: now, last: now }]));
  }
}

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

const center = (b: Box) => ({ x: (b.left + b.right) / 2, y: (b.top + b.bottom) / 2 });
/** Distance between two ranges on a line; 0 when they overlap. */
const gap = (a0: number, a1: number, b0: number, b1: number) => Math.max(0, Math.max(a0, b0) - Math.min(a1, b1));
/** Whether `outer` holds all of `inner`. */
const holds = (outer: Box, inner: Box) =>
  outer.left <= inner.left && outer.top <= inner.top && outer.right >= inner.right && outer.bottom >= inner.bottom;

/**
 * The box to move to from `from` in `dir`, or -1. Only boxes ahead count: their centre and
 * their far edge lie beyond `from`'s in that direction, and they do not hold it. Among them the nearest wins, where a
 * sideways offset (the gap between the two boxes across the direction) counts twice as much
 * as the distance ahead, and the offset of the centres breaks ties.
 */
export function nextFocus(from: Box, boxes: Box[], dir: Dir): number {
  const c = center(from);
  let best = -1;
  let bestScore = Infinity;
  boxes.forEach((b, i) => {
    if (holds(b, from)) return;
    const bc = center(b);
    let ahead: number;
    let side: number;
    let align: number;
    switch (dir) {
      case 'right':
        if (bc.x <= c.x || b.right <= from.right) return;
        ahead = Math.max(0, b.left - from.right);
        side = gap(from.top, from.bottom, b.top, b.bottom);
        align = Math.abs(bc.y - c.y);
        break;
      case 'left':
        if (bc.x >= c.x || b.left >= from.left) return;
        ahead = Math.max(0, from.left - b.right);
        side = gap(from.top, from.bottom, b.top, b.bottom);
        align = Math.abs(bc.y - c.y);
        break;
      case 'down':
        if (bc.y <= c.y || b.bottom <= from.bottom) return;
        ahead = Math.max(0, b.top - from.bottom);
        side = gap(from.left, from.right, b.left, b.right);
        align = Math.abs(bc.x - c.x);
        break;
      case 'up':
        if (bc.y >= c.y || b.top >= from.top) return;
        ahead = Math.max(0, from.top - b.bottom);
        side = gap(from.left, from.right, b.left, b.right);
        align = Math.abs(bc.x - c.x);
        break;
    }
    const score = ahead + 2 * side + 0.01 * align;
    if (score < bestScore) {
      bestScore = score;
      best = i;
    }
  });
  return best;
}

/**
 * The box to start from when nothing has the focus: the one nearest the top left corner (window
 * buttons in the top right corner come last, a dialog starts at its first page or field).
 */
export function firstFocus(boxes: Box[]): number {
  let best = -1;
  boxes.forEach((b, i) => {
    const o = boxes[best];
    if (!o || b.left + b.top < o.left + o.top) best = i;
  });
  return best;
}

/**
 * The value `step` choices on from `value` that can be picked, round the end of the list (from
 * an end when no choice has `value`); `undefined` when there is no other.
 */
export function nextChoice<T>(choices: readonly { value: T; disabled?: boolean }[], value: T, step: 1 | -1): T | undefined {
  const n = choices.length;
  let i = choices.findIndex((c) => c.value === value);
  if (i < 0) i = step > 0 ? -1 : n;
  for (let k = 0; k < n; k++) {
    i = (i + step + n) % n;
    const c = choices[i]!;
    if (!c.disabled) return c.value === value ? undefined : c.value;
  }
  return undefined;
}

/** What steps a field's value on (a drop-down list), by the field. */
const steppers = new WeakMap<Element, (step: 1 | -1) => void>();

/**
 * Action: left and right on the controller (and A) step the value of this field through its
 * choices instead of moving the focus away; the field opens its list for everyone else.
 */
export function stepper(el: HTMLElement, step: ((step: 1 | -1) => void) | undefined) {
  const set = (s: typeof step) => (s ? steppers.set(el, s) : steppers.delete(el));
  set(step);
  return { update: set, destroy: () => steppers.delete(el) };
}

/** What steps `el`'s value on, if it is such a field (see `stepper`). */
export const stepperOf = (el: Element | null) => (el ? steppers.get(el) : undefined);
