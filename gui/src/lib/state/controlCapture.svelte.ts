// "Press a control": the next keyboard key or controller press picks a control, and the menu
// that offers it next to the controller's and the mouse's buttons.

import { t } from '../i18n/index.svelte';
import { controlLabel } from '../model/controls';
import { keyFromEvent } from '../model/keymap';
import type { Catalog, ControlName, PadFamily } from '../types';
import type { MenuItem } from '../menu';
import { app } from './app.svelte';

/**
 * Waits, once started, for the next key or controller press and gives it to `onpick`; Escape
 * stops waiting. Keys are taken in the capture phase, before the shortcuts and the open view
 * see them, and controller presses instead of the editor. Stop it when its component goes.
 */
export function createControlCapture(onpick: (c: ControlName) => void) {
  let active = $state(false);
  let release = () => {};

  function onkeydown(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') return stop();
    const key = keyFromEvent(e);
    if (key) pick(`Key:${key}`);
  }

  function pick(c: ControlName) {
    stop();
    onpick(c);
  }

  function start() {
    if (active) return;
    active = true;
    release = app.capturePresses(pick);
    window.addEventListener('keydown', onkeydown, true);
  }

  function stop() {
    if (!active) return;
    active = false;
    release();
    window.removeEventListener('keydown', onkeydown, true);
  }

  return {
    /** Waiting for a press. */
    get active() {
      return active;
    },
    start,
    stop,
  };
}

/**
 * The items of a menu that picks a control: `press` (which starts a capture), then the
 * controller's buttons and `extra` controls (triggers), then the mouse buttons; `skip` leaves
 * out controls already chosen.
 */
export function controlChoiceItems(
  catalog: Catalog | null,
  family: PadFamily | null,
  onpick: (c: ControlName) => void,
  { press, onpress, extra = [], skip = [] }: { press: string; onpress: () => void; extra?: ControlName[]; skip?: readonly ControlName[] },
): MenuItem[] {
  const item = (c: ControlName): MenuItem => ({ label: controlLabel(c, family), onclick: () => onpick(c) });
  const offered = (list: ControlName[]) => list.filter((c) => !skip.includes(c)).map(item);
  return [
    { label: press, onclick: onpress },
    { heading: t('picker.gamepad') },
    ...offered([...(catalog?.pad_buttons ?? []), ...extra]),
    { heading: t('picker.mouse') },
    ...offered((catalog?.mouse_buttons ?? []).map((b) => `Mouse:${b}`)),
  ];
}
