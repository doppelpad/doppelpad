// The primary controller's presses in the editor: the controls held (the drawing lights
// them), and who gets a new press: a capture ("press a control"), controller navigation while
// the controller drives the app (held directions repeat), or the editor, which selects it.

import { PressRepeat } from '../padnav';
import type { ControlName } from '../types';

/** How often a held direction is looked at again while no new live state comes (a held D-pad does not change it). */
const REPEAT_TICK = 40;

export class PadInput {
  /** Controls held on the primary controller; the drawing highlights exactly these. */
  pressed = $state.raw(new Set<ControlName>());
  /** Runs what the controller does while it drives the app (set by App.svelte). */
  navigate: ((controls: ControlName[]) => void) | null = null;
  /** Who gets the presses instead of the editor (see `capture`). */
  private listener: ((c: ControlName) => void) | null = null;
  private repeat = new PressRepeat();
  private timer: ReturnType<typeof setInterval> | null = null;

  /**
   * `mode`: what the controller does in the focused window now; `select`: the editor selects
   * a pressed control (in the select mode).
   */
  constructor(private readonly host: { mode: () => 'navigate' | 'select' | 'off'; select: (c: ControlName) => void }) {}

  /** The controls held now, from a new live state. */
  update(now: Set<ControlName>) {
    for (const c of now) if (!this.pressed.has(c)) this.press(c);
    // Live state comes many times a second; only a change of the held controls is news.
    if (now.size !== this.pressed.size || [...now].some((c) => !this.pressed.has(c))) this.pressed = now;
    this.drive();
  }

  /**
   * Send the presses to `fn` instead of the editor (without `fn` they just stay away from it)
   * until the returned function is called. That call does nothing once another caller has
   * taken the presses over.
   */
  capture(fn: (c: ControlName) => void = () => {}): () => void {
    this.listener = fn;
    return () => {
      if (this.listener === fn) this.listener = null;
    };
  }

  private press(c: ControlName) {
    if (this.listener) this.listener(c);
    else if (this.host.mode() === 'select' && document.hasFocus()) this.host.select(c);
  }

  /** While the controller drives the app: act on new presses and on held directions, again and again. */
  private drive() {
    const now = performance.now();
    const driving = this.host.mode() === 'navigate' && !this.listener && document.hasFocus();
    if (!driving) {
      // What is held when the mode comes on does not act.
      this.repeat.reset(this.pressed, now);
    } else {
      const acting = this.repeat.update(this.pressed, now);
      if (acting.length) this.navigate?.(acting);
    }
    if (driving && this.repeat.repeating) this.timer ??= setInterval(() => this.drive(), REPEAT_TICK);
    else if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }
}
