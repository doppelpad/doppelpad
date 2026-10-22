// The "identify buttons" wizard for generic pads: one step per pad control, each recording
// the raw input (button, hat direction or axis) that moved away from the resting state.
import type { GenericLayout, HatDir, PadButtonName, RawInput, RawSource, StickAxis } from '../types';

export type Step =
  | { kind: 'button'; button: PadButtonName }
  | { kind: 'trigger'; trigger: 'left' | 'right' }
  /** Push the stick right (x) or up (y). */
  | { kind: 'stick'; axis: StickAxis };

export const STEPS: Step[] = [
  ...(['A', 'B', 'X', 'Y', 'LB', 'RB'] as const).map((button): Step => ({ kind: 'button', button })),
  { kind: 'trigger', trigger: 'left' },
  { kind: 'trigger', trigger: 'right' },
  ...(['Back', 'Start', 'LS', 'RS', 'Guide', 'DpadUp', 'DpadDown', 'DpadLeft', 'DpadRight'] as const).map(
    (button): Step => ({ kind: 'button', button }),
  ),
  ...(['LeftX', 'LeftY', 'RightX', 'RightY'] as const).map((axis): Step => ({ kind: 'stick', axis })),
];

/** What a step recorded. */
export type Hit = { source: RawSource } | { stick: number; invert: boolean };

/** How far an axis moves before it counts. */
const MOVED = 0.5;
/** How close to rest everything must be before the next step starts. */
const SETTLED = 0.25;

const HAT_DIRS: Record<number, HatDir> = { 0: 'up', 2: 'right', 4: 'down', 6: 'left' };

export function modelKey(vendorId: number, productId: number): string {
  const hex = (n: number) => n.toString(16).padStart(4, '0');
  return `${hex(vendorId)}:${hex(productId)}`;
}

function pressedButton(rest: RawInput, now: RawInput): RawSource | null {
  const i = now.buttons.findIndex((b, i) => b && !rest.buttons[i]);
  return i >= 0 ? { kind: 'button', index: i } : null;
}

function pushedHat(rest: RawInput, now: RawInput): RawSource | null {
  for (let i = 0; i < now.hats.length; i++) {
    const d = now.hats[i];
    // Diagonals are ambiguous: wait for a straight push.
    if (d !== null && d !== rest.hats[i] && HAT_DIRS[d]) return { kind: 'hat', index: i, dir: HAT_DIRS[d] };
  }
  return null;
}

function movedAxis(rest: RawInput, now: RawInput): number {
  let best = -1;
  let far = MOVED;
  now.axes.forEach((v, i) => {
    const d = Math.abs(v - (rest.axes[i] ?? 0));
    if (d > far) {
      far = d;
      best = i;
    }
  });
  return best;
}

function axisSource(rest: RawInput, now: RawInput, i: number): RawSource {
  const r = rest.axes[i] ?? 0;
  return { kind: 'axis', index: i, rest: r, full: now.axes[i] > r ? 1 : -1 };
}

/** The input the user moved for `step`, compared with the resting state, if any. */
export function detect(step: Step, rest: RawInput, now: RawInput): Hit | null {
  if (step.kind === 'stick') {
    const i = movedAxis(rest, now);
    // Right and up are positive; HID's y grows downwards.
    return i < 0 ? null : { stick: i, invert: now.axes[i] < (rest.axes[i] ?? 0) };
  }
  const axis = movedAxis(rest, now);
  const button = pressedButton(rest, now);
  // Triggers are analog when they can be: some pads report a button with the axis.
  if (step.kind === 'trigger' && axis >= 0) return { source: axisSource(rest, now, axis) };
  const source = button ?? pushedHat(rest, now) ?? (axis >= 0 ? axisSource(rest, now, axis) : null);
  return source ? { source } : null;
}

/** Everything is back where it rested (so the next step does not see the last one). */
export function settled(rest: RawInput, now: RawInput): boolean {
  return (
    now.buttons.every((b, i) => b === !!rest.buttons[i]) &&
    now.hats.every((h, i) => h === (rest.hats[i] ?? null)) &&
    now.axes.every((v, i) => Math.abs(v - (rest.axes[i] ?? 0)) < SETTLED)
  );
}

/** The layout made of the hits, by step index; skipped steps map nothing. */
export function buildLayout(device: string, hits: (Hit | null)[]): GenericLayout {
  const layout: GenericLayout = { device, buttons: [], triggers: [], sticks: [] };
  STEPS.forEach((step, i) => {
    const hit = hits[i];
    if (!hit) return;
    if ('stick' in hit) {
      if (step.kind === 'stick') layout.sticks.push({ axis: step.axis, from: hit.stick, invert: hit.invert });
    } else if (step.kind === 'button') {
      layout.buttons.push({ button: step.button, from: hit.source });
    } else if (step.kind === 'trigger') {
      layout.triggers.push({ trigger: step.trigger, from: hit.source });
    }
  });
  return layout;
}
