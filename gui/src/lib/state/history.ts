// Undo and redo for the open config: the profile as it was before each change, as JSON.

/** At most this many steps are kept; the oldest go first. */
export const HISTORY_LIMIT = 100;
/** Changes from one focused field closer together than this are one step. */
export const FIELD_MERGE_MS = 1000;

/**
 * What a change belongs to, so a slider drag or typing into a field is one step:
 * - `gesture`: one pointer drag or one recording; its changes merge however long it takes;
 * - `field`: one focused input; its changes merge while they come less than
 *   `FIELD_MERGE_MS` apart;
 * - `null`: a step of its own (a click, a menu entry).
 */
export type EditKey = { kind: 'gesture' | 'field'; id: unknown } | null;

const sameKey = (a: EditKey, b: EditKey) => !!a && !!b && a.kind === b.kind && a.id === b.id;

export class History {
  private past: string[] = [];
  private future: string[] = [];
  private lastKey: EditKey = null;
  private lastAt = 0;

  get canUndo(): boolean {
    return this.past.length > 0;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  /**
   * Note a change: `before` is the profile before it. A change with the key of the step before
   * (see `EditKey`) belongs to that step and adds nothing. A new step drops what could be redone.
   */
  record(before: string, key: EditKey, now: number) {
    const merge = sameKey(key, this.lastKey) && (key!.kind === 'gesture' || now - this.lastAt < FIELD_MERGE_MS);
    this.lastKey = key;
    this.lastAt = now;
    if (merge) return;
    this.past.push(before);
    if (this.past.length > HISTORY_LIMIT) this.past.shift();
    this.future = [];
  }

  /** The profile before the last step, given the current one (which redo brings back); `null` if there is none. */
  undo(current: string): string | null {
    return this.move(this.past, this.future, current);
  }

  /** The profile of the step undone last, given the current one; `null` if there is none. */
  redo(current: string): string | null {
    return this.move(this.future, this.past, current);
  }

  clear() {
    this.past = [];
    this.future = [];
    this.lastKey = null;
  }

  private move(from: string[], to: string[], current: string): string | null {
    const target = from.pop();
    if (target === undefined) return null;
    to.push(current);
    // The next change starts a step of its own, even from the same field or drag.
    this.lastKey = null;
    return target;
  }
}
