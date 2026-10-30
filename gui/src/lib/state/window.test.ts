// The window state against a fake Tauri window: which focus changes count as coming back.

import { describe, expect, it, vi } from 'vitest';

/** The window's focus handler, once the state has subscribed. */
let focusChanged: (e: { payload: boolean }) => void = () => {};
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    isMaximized: async () => false,
    isFullscreen: async () => false,
    onResized: async () => () => {},
    onFocusChanged: async (f: (e: { payload: boolean }) => void) => {
      focusChanged = f;
      return () => {};
    },
  }),
}));

const { win } = await import('./window.svelte');

describe('focus', () => {
  it('coming back to the front counts once; being in front at start does not', async () => {
    let back = 0;
    win.onFocus = () => back++;
    await win.init();
    focusChanged({ payload: true });
    expect(back).toBe(0);
    focusChanged({ payload: false });
    expect(win.focused).toBe(false);
    focusChanged({ payload: true });
    focusChanged({ payload: true });
    expect([back, win.focused]).toEqual([1, true]);
  });
});
