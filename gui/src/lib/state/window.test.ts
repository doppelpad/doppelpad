// The window state against a fake Tauri window: which focus changes count as coming back.

import { describe, expect, it, vi } from 'vitest';

/** The window's focus handler, once the state has subscribed. */
let focusChanged: (e: { payload: boolean }) => void = () => {};
/** The app's own events, by name, once the state listens. */
const events = new Map<string, () => void>();
/** What the window says about itself at start. */
let visible = true;
let focused = true;
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    isMaximized: async () => false,
    isFullscreen: async () => false,
    isVisible: async () => visible,
    isFocused: async () => focused,
    onResized: async () => () => {},
    onFocusChanged: async (f: (e: { payload: boolean }) => void) => {
      focusChanged = f;
      return () => {};
    },
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, f: () => void) => {
    events.set(name, f);
    return () => {};
  },
}));

const { win } = await import('./window.svelte');

/** The window as the app starts it; counts how often it comes back. */
async function started(shown: boolean) {
  [visible, focused] = [shown, true];
  const back = { count: 0 };
  win.onFocus = () => back.count++;
  await win.init();
  return back;
}

describe('focus', () => {
  it('coming back to the front counts once; being in front at start does not', async () => {
    const back = await started(true);
    focusChanged({ payload: true });
    expect(back.count).toBe(0);
    focusChanged({ payload: false });
    expect(win.focused).toBe(false);
    focusChanged({ payload: true });
    focusChanged({ payload: true });
    expect([back.count, win.focused]).toEqual([1, true]);
  });

  it('started hidden with the system, showing it counts, though Windows changes no focus', async () => {
    const back = await started(false);
    expect(win.focused).toBe(false);
    events.get('main-shown')!();
    // A focus change that does come after it is the same coming back.
    focusChanged({ payload: true });
    expect([back.count, win.focused]).toEqual([1, true]);
  });

  it('hidden to the tray and shown again counts, also when Windows kept calling it focused', async () => {
    const back = await started(true);
    events.get('main-hidden')!();
    events.get('main-shown')!();
    expect(back.count).toBe(1);
  });
});
