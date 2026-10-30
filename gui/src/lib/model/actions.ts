// Editing the parameters of single actions (stick amount, trigger value, wheel clicks, …).

import type { Action, ControlName } from '../types';
import { nativeAction } from './profile';
import { clamp, roundTo } from './util';

type StickAction = Extract<Action, { stick: unknown }>;

/** How far a stick action deflects: the larger axis, 0–1. */
export function stickAmount(a: StickAction): number {
  return Math.max(Math.abs(a.x), Math.abs(a.y));
}

/** The same direction with another deflection (0–1). */
export function withStickAmount(a: StickAction, amount: number): StickAction {
  const now = stickAmount(a);
  const v = clamp(amount, 0, 1);
  if (now === 0) return a;
  return { ...a, x: roundTo((a.x / now) * v, 3), y: roundTo((a.y / now) * v, 3) };
}

/** Whether `parameterFields` has anything to offer for this action. */
export function hasParameters(a: Action): boolean {
  return (
    'stick' in a ||
    'trigger' in a ||
    'wheel' in a ||
    'mouse_move' in a ||
    'rumble' in a ||
    'turn' in a ||
    'touch' in a ||
    'mouse_to' in a ||
    'mouse_clip' in a ||
    ('command' in a && typeof a.command === 'object' && 'launch' in a.command)
  );
}

/** An action that presses something: a key, a button, a stick or a trigger. */
export type PressAction = Extract<Action, { action?: unknown }>;

/** Whether an action presses something, so a step can press, release or tap it (`action`). */
export function hasPress(a: Action): a is PressAction {
  return 'key' in a || 'mouse' in a || 'pad' in a || 'stick' in a || 'trigger' in a;
}

/** How long "hold for a time" holds at first. */
export const HOLD_MS = 500;

/**
 * Steps where step `i` (one that presses something) is held for `ms`: pressed, a pause,
 * released.
 */
export function holdFor(steps: Action[], i: number, ms = HOLD_MS): Action[] {
  const s = steps[i];
  if (!s || !hasPress(s)) return steps;
  const down: Action = { ...s, action: 'down' };
  const up: Action = { ...s, action: 'up' };
  return [...steps.slice(0, i), down, { pause: ms }, up, ...steps.slice(i + 1)];
}

/** Pause steps shorter than this are not recorded (one time unit). */
export const RECORD_MIN_PAUSE_MS = 10;

/**
 * Steps for a recorded change of held controls: releases first, then presses, preceded
 * by the pause since the previous change when it is long enough to matter.
 */
export function recordedSteps(
  before: ReadonlySet<ControlName>,
  after: ReadonlySet<ControlName>,
  gapMs: number | null,
  keyDelay: number,
): Action[] {
  const out: Action[] = [];
  const ups = [...before].filter((c) => !after.has(c)).sort();
  const downs = [...after].filter((c) => !before.has(c)).sort();
  for (const [list, action] of [
    [ups, 'up'],
    [downs, 'down'],
  ] as const) {
    for (const c of list) {
      const a = nativeAction(c);
      if (!a || !('pad' in a || 'stick' in a || 'trigger' in a)) continue;
      if (out.length === 0 && gapMs !== null && gapMs >= RECORD_MIN_PAUSE_MS && gapMs > keyDelay) {
        out.push({ pause: Math.round(gapMs) });
      }
      out.push({ ...a, action } as Action);
    }
  }
  return out;
}
