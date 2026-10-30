// State of the app window, for the title bar the app draws itself (see main.rs).

import { getCurrentWindow } from '@tauri-apps/api/window';

export type Platform = 'windows' | 'macos' | 'linux';

/** The webview's user agent names the OS reliably enough for picking a title bar style. */
export const platform: Platform = /Macintosh|Mac OS X/.test(navigator.userAgent)
  ? 'macos'
  : /Windows/.test(navigator.userAgent)
    ? 'windows'
    : 'linux';

class WindowState {
  maximized = $state(false);
  fullscreen = $state(false);
  focused = $state(true);
  /** Runs each time the window comes to the front again (not at start). */
  onFocus: () => void = () => {};

  async init() {
    const w = getCurrentWindow();
    const sync = async () => {
      this.maximized = await w.isMaximized();
      this.fullscreen = await w.isFullscreen();
    };
    await sync();
    await w.onResized(sync);
    await w.onFocusChanged(({ payload }) => {
      if (payload && !this.focused) this.onFocus();
      this.focused = payload;
    });
  }

  minimize() {
    getCurrentWindow().minimize();
  }

  toggleMaximize() {
    getCurrentWindow().toggleMaximize();
  }

  /** Hides to the tray, like the system close button (see main.rs). */
  close() {
    getCurrentWindow().close();
  }
}

export const win = new WindowState();
