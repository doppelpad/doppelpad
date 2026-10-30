// Who gets the primary controller's presses: a capture, the editor, or controller navigation.

import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ControlName } from '../types';
import { PadInput } from './padInput.svelte';

vi.stubGlobal('document', { hasFocus: () => true });

type Mode = 'navigate' | 'select' | 'off';

function pad(start: Mode) {
  const selected: ControlName[] = [];
  const navigated: ControlName[][] = [];
  const state = { mode: start };
  const input = new PadInput({ mode: () => state.mode, select: (c) => selected.push(c) });
  input.navigate = (cs) => navigated.push(cs);
  return { input, selected, navigated, state };
}

const held = (...cs: ControlName[]) => new Set(cs);

afterEach(() => vi.useRealTimers());

describe('pad input', () => {
  it('a new press selects the control; one still held does not again', () => {
    const { input, selected } = pad('select');
    input.update(held('A'));
    input.update(held('A', 'B'));
    expect(selected).toEqual(['A', 'B']);
    expect(input.pressed).toEqual(held('A', 'B'));
  });

  it('a capture takes the presses until it lets go; an older one cannot let a newer go', () => {
    const { input, selected } = pad('select');
    const taken: ControlName[] = [];
    const first = input.capture((c) => taken.push(`1:${c}`));
    const second = input.capture((c) => taken.push(`2:${c}`));
    first();
    input.update(held('A'));
    second();
    input.update(held('A', 'B'));
    expect(taken).toEqual(['2:A']);
    expect(selected).toEqual(['B']);
  });

  it('while navigating, what was held when it came on does not act; a held direction repeats', () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'performance'] });
    const { input, selected, navigated, state } = pad('off');
    input.update(held('A'));
    state.mode = 'navigate';
    input.update(held('A', 'DpadDown'));
    expect(navigated).toEqual([['DpadDown']]);
    vi.advanceTimersByTime(600);
    expect(navigated.length).toBeGreaterThan(1);
    expect(navigated.every((cs) => cs.join() === 'DpadDown')).toBe(true);
    input.update(held());
    const n = navigated.length;
    vi.advanceTimersByTime(600);
    expect(navigated.length).toBe(n);
    expect(selected).toEqual([]);
  });
});
