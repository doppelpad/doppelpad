// The editor's keyboard shortcuts: one table for the key handler, the buttons' tooltips and the
// list of shortcuts, so they never disagree.

import { t } from '../i18n/index.svelte';

export type ShortcutId = 'save' | 'undo' | 'redo' | 'copy' | 'paste' | 'clear' | 'deselect' | 'layer' | 'rename' | 'duplicate' | 'help';

/** One key combination. `key` is compared with `KeyboardEvent.key`, letters in lower case. */
export interface Keys {
  key: string;
  /** Ctrl (Cmd on macOS). */
  mod?: boolean;
  shift?: boolean;
  /** Matches with or without Shift (for characters some layouts type with Shift, like `/`). */
  anyShift?: boolean;
}

export interface Shortcut {
  id: ShortcutId;
  keys: Keys[];
  /** Also while typing in a text field. */
  inText?: boolean;
  /** Also while a dialog or menu is open. */
  inOverlay?: boolean;
  /** In the keyboard view, where plain keys select themselves, these keys apply instead (none: off). */
  keyboardView?: Keys[];
}

/** Digits for the layers: 1 is Main, 2 … 9 the shift layers in the toolbar's order. */
const DIGITS = ['1', '2', '3', '4', '5', '6', '7', '8', '9'];

export const SHORTCUTS: Shortcut[] = [
  { id: 'save', keys: [{ key: 's', mod: true }], inText: true, inOverlay: true },
  { id: 'undo', keys: [{ key: 'z', mod: true }] },
  {
    id: 'redo',
    keys: [
      { key: 'y', mod: true },
      { key: 'z', mod: true, shift: true },
    ],
  },
  { id: 'copy', keys: [{ key: 'c', mod: true }] },
  { id: 'paste', keys: [{ key: 'v', mod: true }] },
  { id: 'clear', keys: [{ key: 'Delete' }], keyboardView: [{ key: 'Delete', mod: true }] },
  { id: 'deselect', keys: [{ key: 'Escape' }], keyboardView: [] },
  { id: 'layer', keys: DIGITS.map((key) => ({ key, mod: true })) },
  { id: 'rename', keys: [{ key: 'F2' }], keyboardView: [] },
  { id: 'duplicate', keys: [{ key: 'd', mod: true }] },
  { id: 'help', keys: [{ key: '/', mod: true, anyShift: true }, { key: 'F1' }], keyboardView: [{ key: '/', mod: true, anyShift: true }] },
];

/** The key press, as far as matching needs it. */
export interface KeyPress {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/** Where the press happens. */
export interface KeyContext {
  /** The keyboard view is shown: plain keys select keys there. */
  keyboardView: boolean;
  /** The focus is in a text field. */
  typing: boolean;
  /** A dialog or a menu is open. */
  overlay: boolean;
}

function matches(k: Keys, e: KeyPress): boolean {
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  return key === k.key && mod === !!k.mod && (k.anyShift || e.shiftKey === !!k.shift);
}

/** The shortcut a key press triggers (with the digit for `layer`), or `null`. AltGr and Alt never match. */
export function matchShortcut(e: KeyPress, ctx: KeyContext): { id: ShortcutId; digit?: number } | null {
  if (e.altKey) return null;
  for (const s of SHORTCUTS) {
    if (ctx.typing && !s.inText) continue;
    if (ctx.overlay && !s.inOverlay) continue;
    const keys = ctx.keyboardView ? (s.keyboardView ?? s.keys) : s.keys;
    const hit = keys.find((k) => matches(k, e));
    if (hit) return s.id === 'layer' ? { id: s.id, digit: Number(hit.key) } : { id: s.id };
  }
  return null;
}

/** A key combination as people read it: "Ctrl+Shift+Z", "Strg+Entf", "⌘Z". */
export function keysLabel(k: Keys, mac: boolean): string {
  const names: Record<string, string> = { Delete: t('keys.delete'), Escape: t('keys.esc') };
  const key = names[k.key] ?? (k.key.length === 1 ? k.key.toUpperCase() : k.key);
  if (mac) return (k.shift ? '⇧' : '') + (k.mod ? '⌘' : '') + key;
  return [k.mod && t('keys.ctrl'), k.shift && t('keys.shift'), key].filter(Boolean).join('+');
}

/** The first key combination of a shortcut, for a button's tooltip. */
export function shortcutLabel(id: ShortcutId, mac: boolean): string {
  const s = SHORTCUTS.find((x) => x.id === id);
  return s ? keysLabel(s.keys[0]!, mac) : '';
}

/** A button's tooltip with its shortcut: "Undo (Ctrl+Z)". */
export function withShortcut(label: string, id: ShortcutId, mac: boolean): string {
  return `${label} (${shortcutLabel(id, mac)})`;
}
