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

/** A menu (ours or a context menu) is open and owns the keyboard. */
export function menuOpen(): boolean {
  return document.querySelector('[role=menu]') !== null;
}

/** A popover marked `data-popover` (e.g. the stick's direction menu) is open; Escape closes it first. */
export function popoverOpen(): boolean {
  return document.querySelector('[data-popover]') !== null;
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
