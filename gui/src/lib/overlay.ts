// What an overlay window shows, sent by the app (gui/src-tauri/src/overlay.rs). The event is
// named after the window, since every page hears every event whichever window it is sent to.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

/** Call `apply` with the current content, then with every change; returns the unsubscribe. */
export function overlayContent<T>(apply: (content: T | null) => void): () => void {
  const off = listen<T>(getCurrentWebviewWindow().label, (e) => apply(e.payload));
  invoke<T | null>('overlay_ready')
    .then(apply)
    .catch((e) => console.error('overlay_ready failed', e));
  return () => {
    off.then((f) => f());
  };
}
