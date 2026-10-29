// Small facts about the page that the app's key handling and edit history need. They look at
// tag names rather than DOM classes, so they also work in the tests (no DOM there).

/** Input types whose text the user types and edits (they have an undo of their own). */
const TEXT_TYPES = new Set(['text', 'search', 'number', 'email', 'url', 'tel', 'password']);
/** Inputs whose value changes in many small steps (a drag, arrow keys, a colour picker). */
const VALUE_TYPES = new Set(['range', 'color']);

type Field = { tagName?: unknown; type?: unknown; isContentEditable?: unknown };

const field = (el: unknown): Field | null =>
  el && typeof el === 'object' && typeof (el as Field).tagName === 'string' ? (el as Field) : null;

/** `el` takes typed text: a text-like input, a text area or editable content. */
export function isTextEntry(el: unknown): boolean {
  const f = field(el);
  if (!f) return false;
  if (f.tagName === 'TEXTAREA') return true;
  if (f.tagName === 'INPUT') return TEXT_TYPES.has(String(f.type));
  return f.isContentEditable === true;
}

/** `el` is a field whose changes come in a stream: text entry, a slider or a colour input. */
export function isStreamingField(el: unknown): boolean {
  const f = field(el);
  return isTextEntry(f) || (f?.tagName === 'INPUT' && VALUE_TYPES.has(String(f.type)));
}

/** `el` takes keys itself: a text entry, or another input (Space ticks a box, arrows move a slider). */
function takesKeys(el: unknown): boolean {
  return isTextEntry(el) || field(el)?.tagName === 'INPUT';
}

/** What the keyboard (Tab) can reach. */
export const FOCUSABLE = [
  'button',
  'a[href]',
  'input:not([type="hidden"])',
  'textarea',
  'summary',
  '[tabindex]:not([tabindex="-1"])',
  '[role="menuitem"]',
  '[role="tab"]',
].join(', ');

/** The elements in `root` that Tab stops at, in their order: enabled, shown, not taken out of the order. */
export function tabbables(root: Element): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) => el.tabIndex >= 0 && !(el as HTMLButtonElement).disabled && el.getClientRects().length > 0,
  );
}

/**
 * Action for a radio group or a tab list: the arrow keys (and Home, End) move to the previous
 * or next of its `[role=radio]` / `[role=tab]` items and choose it, as the system's own groups
 * do. In a grid of `columns`, up and down move by a row. The items themselves keep only the
 * chosen one in the Tab order (`tabindex`).
 */
export function arrowKeys(node: HTMLElement, columns = 0) {
  function onkeydown(e: KeyboardEvent) {
    const items = [...node.querySelectorAll<HTMLElement>('[role=radio], [role=tab]')].filter((el) => !(el as HTMLButtonElement).disabled);
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (i < 0) return;
    const row = columns || 1;
    const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -row, ArrowDown: row, Home: -i, End: items.length - 1 - i }[e.key];
    if (step === undefined) return;
    e.preventDefault();
    const next = items[(i + step + items.length) % items.length]!;
    next.focus();
    next.click();
  }
  node.addEventListener('keydown', onkeydown);
  return {
    update: (c = 0) => (columns = c),
    destroy: () => node.removeEventListener('keydown', onkeydown),
  };
}

/** Open dialogs (ui/Dialog), the topmost last: Escape closes only that one. */
export const dialogStack: symbol[] = [];

/** A dialog is open; it owns the keyboard then. */
export function modalOpen(): boolean {
  return dialogStack.length > 0;
}

/** A menu (ours or a context menu) is open and owns the keyboard. */
export function menuOpen(): boolean {
  return document.querySelector('[role=menu]') !== null;
}

/** A popover marked `data-popover` (e.g. the stick's direction menu, Find) is open; Escape closes it first. */
export function popoverOpen(): boolean {
  return document.querySelector('[data-popover]') !== null;
}

/**
 * The keys pressed at `target` belong to something other than the view: a field that takes
 * them, a dialog, an open menu or popover.
 */
export function keyboardOwned(target: unknown): boolean {
  return takesKeys(target) || modalOpen() || menuOpen() || popoverOpen();
}

/** Listen to `type` events on `target` (in the capture phase with `capture`) until the returned function is called. */
export function listen<E extends Event>(target: EventTarget, type: string, fn: (e: E) => void, capture = false): () => void {
  target.addEventListener(type, fn as EventListener, capture);
  return () => target.removeEventListener(type, fn as EventListener, capture);
}

/**
 * The pointer gesture in progress: `down` while a button is held, `id` counts the gestures.
 * Changes made while it is down (a drag) belong together.
 */
export const pointer = { down: false, id: 0 };

if (typeof window !== 'undefined') {
  const up = () => (pointer.down = false);
  window.addEventListener(
    'pointerdown',
    () => {
      pointer.down = true;
      pointer.id++;
    },
    true,
  );
  window.addEventListener('pointerup', up, true);
  window.addEventListener('pointercancel', up, true);
  window.addEventListener('blur', up);
}
