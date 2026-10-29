// "Press a control": the next key or controller press is taken before anything else sees it.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Catalog, ControlName } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ Channel: class {}, invoke: async () => null }));
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));

/** The window's key listeners: the function and whether it listens in the capture phase. */
const keyListeners = new Set<{ fn: (e: KeyboardEvent) => void; capture: boolean }>();
vi.stubGlobal('window', {
  addEventListener: (type: string, fn: (e: KeyboardEvent) => void, capture?: boolean) => {
    if (type === 'keydown') keyListeners.add({ fn, capture: !!capture });
  },
  removeEventListener: (type: string, fn: (e: KeyboardEvent) => void) => {
    for (const l of keyListeners) if (type === 'keydown' && l.fn === fn) keyListeners.delete(l);
  },
});

const { app } = await import('./app.svelte');
const { controlChoiceItems, createControlCapture } = await import('./controlCapture.svelte');

/** A key press as the window gets it. */
function press(code: string) {
  const e = { code, key: code, preventDefault: vi.fn(), stopPropagation: vi.fn() };
  for (const l of [...keyListeners]) l.fn(e as unknown as KeyboardEvent);
  return e;
}

beforeEach(() => keyListeners.clear());

describe('control capture', () => {
  it('takes the next key in the capture phase, so no shortcut runs with it', () => {
    const picked: ControlName[] = [];
    const capture = createControlCapture((c) => picked.push(c));
    capture.start();
    expect(capture.active).toBe(true);
    expect([...keyListeners].map((l) => l.capture)).toEqual([true]);
    const e = press('F1');
    expect(e.preventDefault).toHaveBeenCalled();
    expect(e.stopPropagation).toHaveBeenCalled();
    expect(picked).toEqual(['Key:F1']);
    expect(capture.active).toBe(false);
    expect(keyListeners.size).toBe(0);
  });

  it('takes a controller press instead of the editor; Escape stops without a pick', () => {
    const picked: ControlName[] = [];
    let presses: ((c: ControlName) => void) | undefined;
    const spy = vi.spyOn(app, 'capturePresses').mockImplementation((fn) => {
      presses = fn;
      return () => (presses = undefined);
    });
    const capture = createControlCapture((c) => picked.push(c));
    capture.start();
    presses?.('LB');
    expect(picked).toEqual(['LB']);
    expect(presses).toBeUndefined();
    capture.start();
    press('Escape');
    expect(capture.active).toBe(false);
    expect(presses).toBeUndefined();
    expect(picked).toEqual(['LB']);
    spy.mockRestore();
  });
});

describe('control choices', () => {
  const catalog = { pad_buttons: ['A', 'B'], mouse_buttons: ['Left'] } as unknown as Catalog;

  it('offers pressing one, then the controller and the mouse, without the ones left out', () => {
    const picked: ControlName[] = [];
    let pressed = false;
    const items = controlChoiceItems(catalog, 'xbox', (c) => picked.push(c), {
      press: 'Press one',
      onpress: () => (pressed = true),
      extra: ['LT'],
      skip: ['B'],
    });
    expect(items.map((i) => i.label)).toEqual(['Press one', 'Controller', 'A', 'Left trigger', 'Mouse', 'Left button']);
    items[0]!.onclick?.();
    items[2]!.onclick?.();
    items[5]!.onclick?.();
    expect(pressed).toBe(true);
    expect(picked).toEqual(['A', 'Mouse:Left']);
  });
});
