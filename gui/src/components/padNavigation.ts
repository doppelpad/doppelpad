// What the controller does while it drives the app (handheld mode); which control does what is
// in `lib/padnav.ts`. The focus moves between the elements the keyboard reaches, inside the
// open menu or dialog if there is one, as Tab would.

import { menuOpen, popoverOpen } from '../lib/dom';
import { layerIds } from '../lib/model/profile';
import { firstFocus, navAction, nextFocus, stepperOf } from '../lib/padnav';
import { TOOLBAR_VIEWS, type View } from '../lib/model/views';
import { app } from '../lib/state/app.svelte';
import type { ControlName, Dir } from '../lib/types';
import { modalOpen } from './ui/Dialog.svelte';

const FOCUSABLE = [
  'button',
  'a[href]',
  'input:not([type="hidden"])',
  'textarea',
  'summary',
  '[tabindex]:not([tabindex="-1"])',
  '[role="menuitem"]',
  '[role="tab"]',
].join(', ');

/** Where the focus may go: the open menu or popover, else the dialog that has the focus (else the last one), else the window. */
function scope(): HTMLElement {
  const active = document.activeElement;
  const top = (selector: string) => {
    const inside = active?.closest<HTMLElement>(selector);
    const all = document.querySelectorAll<HTMLElement>(selector);
    return inside ?? all[all.length - 1] ?? null;
  };
  return top('[role="menu"], [data-popover]') ?? top('[role="dialog"]') ?? document.body;
}

/** An element the focus may go to: enabled, shown and at least partly in the window. */
function usable(el: HTMLElement): boolean {
  if ((el as HTMLButtonElement).disabled || el.closest('[inert], [aria-hidden="true"]')) return false;
  const r = el.getBoundingClientRect();
  if (r.width === 0 || r.height === 0) return false;
  if (r.bottom < 0 || r.right < 0 || r.top > window.innerHeight || r.left > window.innerWidth) return false;
  return el.checkVisibility?.() ?? true;
}

function targets(): HTMLElement[] {
  return [...scope().querySelectorAll<HTMLElement>(FOCUSABLE)].filter(usable);
}

function focus(el: HTMLElement) {
  el.focus();
  el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

function changed(el: HTMLElement) {
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
}

/** Left and right change a slider, a number field or a list instead of moving away; whether `el` took the direction. */
function adjust(el: Element | null, dir: Dir): boolean {
  if (dir !== 'left' && dir !== 'right') return false;
  const step = dir === 'right' ? 1 : -1;
  if (el instanceof HTMLInputElement && (el.type === 'range' || el.type === 'number')) {
    if (step > 0) el.stepUp();
    else el.stepDown();
    changed(el);
    return true;
  }
  const stepList = stepperOf(el);
  if (stepList) {
    stepList(step);
    return true;
  }
  return false;
}

function moveFocus(dir: Dir) {
  const active = document.activeElement as HTMLElement | null;
  if (adjust(active, dir)) return;
  const list = targets();
  const boxes = list.map((e) => e.getBoundingClientRect());
  const from = active && list.includes(active) ? active : null;
  const i = from ? nextFocus(from.getBoundingClientRect(), boxes, dir) : firstFocus(boxes);
  if (i >= 0) focus(list[i]!);
}

/** A: press the focused button, tick the box, step the list; a text field keeps the focus for typing. */
function activate() {
  const el = document.activeElement as HTMLElement | null;
  const list = targets();
  if (!el || !list.includes(el)) {
    const i = firstFocus(list.map((e) => e.getBoundingClientRect()));
    if (i >= 0) focus(list[i]!);
    return;
  }
  const stepList = stepperOf(el);
  if (stepList) return stepList(1);
  if (el instanceof HTMLTextAreaElement) return;
  if (el instanceof HTMLInputElement && !['checkbox', 'radio', 'button', 'submit', 'reset', 'color', 'file'].includes(el.type)) return;
  el.click();
}

/** B: what Escape does — close the menu or dialog, else drop the selection. */
function back() {
  const target = (document.activeElement as HTMLElement | null) ?? document.body;
  target.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true, cancelable: true }));
}

/** The bumpers step through the toolbar's views. */
function stepView(step: 1 | -1) {
  const n = TOOLBAR_VIEWS.length;
  const i = (TOOLBAR_VIEWS as readonly View[]).indexOf(app.view);
  app.view = i < 0 ? 'front' : TOOLBAR_VIEWS[(i + step + n) % n]!;
}

function stepLayer(step: 1 | -1) {
  if (!app.profile) return;
  const ids = layerIds(app.profile);
  const i = Math.max(0, ids.indexOf(app.layer));
  app.layer = ids[(i + step + ids.length) % ids.length]!;
}

/** Act on the controls pressed (or repeating) now. Views, layers and saving wait while a dialog or menu is open. */
export function runPadControls(controls: ControlName[]) {
  const overlay = modalOpen() || menuOpen() || popoverOpen();
  for (const c of controls) {
    const a = navAction(c);
    if (!a) continue;
    switch (a.kind) {
      case 'move':
        moveFocus(a.dir);
        break;
      case 'activate':
        activate();
        break;
      case 'back':
        back();
        break;
      case 'view':
        if (!overlay) stepView(a.step);
        break;
      case 'layer':
        if (!overlay) stepLayer(a.step);
        break;
      case 'save':
        if (!overlay && app.dirty && !app.readOnly) app.save();
        break;
    }
  }
}
