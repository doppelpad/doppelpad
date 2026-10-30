// The shared tooltip: which element's text shows, and when.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { hideTooltip, tip, tooltip } from './tooltip.svelte';

/** An element as far as the tooltip action looks at it, with its listeners to fire. */
function element() {
  const on = new Map<string, () => void>();
  const attrs = new Map<string, string>();
  const el = {
    isConnected: true,
    textContent: '',
    addEventListener: (type: string, fn: () => void) => on.set(type, fn),
    removeEventListener: (type: string) => on.delete(type),
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 10, bottom: 10, width: 10, height: 10 }),
    hasAttribute: (name: string) => attrs.has(name),
    setAttribute: (name: string, value: string) => attrs.set(name, value),
    removeAttribute: (name: string) => attrs.delete(name),
    matches: () => true,
  };
  return { el: el as unknown as HTMLElement, fire: (type: string) => on.get(type)?.() };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  hideTooltip();
  vi.useRealTimers();
});

describe('tooltips', () => {
  it('shows after the pointer rests, and goes when it leaves', () => {
    const a = element();
    tip(a.el, 'Save');
    a.fire('pointerenter');
    expect(tooltip.current).toBeNull();
    vi.advanceTimersByTime(600);
    expect(tooltip.current?.text).toBe('Save');
    a.fire('pointerleave');
    expect(tooltip.current).toBeNull();
  });

  it('another element going away does not cancel the tooltip on its way', () => {
    const a = element();
    const b = element();
    tip(a.el, 'Undo');
    const actionB = tip(b.el, 'Redo');
    b.fire('focus');
    a.fire('blur');
    vi.advanceTimersByTime(10);
    expect(tooltip.current?.text).toBe('Redo');
    hideTooltip();
    b.fire('pointerenter');
    a.fire('pointerleave');
    actionB.update('Redo again');
    vi.advanceTimersByTime(600);
    expect(tooltip.current?.text).toBe('Redo again');
  });
});
