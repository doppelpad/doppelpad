// What an overlay window shows, sent by the app (gui/src-tauri/src/overlay.rs). The event is
// named after the window, since every page hears every event whichever window it is sent to;
// `<label>:patch` changes some fields of the content (the radial menu's pointed sector) without
// sending the rest again.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

/** What the patch event of a window is named: its label and this. */
export const PATCH_SUFFIX = ':patch';

/** `content` with the fields of `patch`; nothing while there is no content. */
export function patched<T>(content: T | null, patch: Partial<T>): T | null {
  return content === null ? null : { ...content, ...patch };
}

/** Call `apply` with the current content, then with every change; returns the unsubscribe. */
export function overlayContent<T>(apply: (content: T | null) => void): () => void {
  const label = getCurrentWebviewWindow().label;
  let current: T | null = null;
  const show = (content: T | null) => {
    current = content;
    apply(content);
  };
  const offs = Promise.all([
    listen<T | null>(label, (e) => show(e.payload)),
    listen<Partial<T>>(label + PATCH_SUFFIX, (e) => show(patched(current, e.payload))),
  ]);
  // Ready only once the page listens: the app sends changes as events from then on.
  offs
    .then(() => invoke<T | null>('overlay_ready'))
    .then(show)
    .catch((e) => console.error('overlay_ready failed', e));
  return () => {
    offs.then((fs) => fs.forEach((f) => f()));
  };
}
