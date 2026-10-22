import { describe, expect, it } from 'vitest';
import { has, setLanguage } from '../i18n/index.svelte';
import { SHORTCUTS, keysLabel, matchShortcut, shortcutLabel, type KeyContext, type KeyPress } from './shortcuts';

const press = (key: string, mods: Partial<Omit<KeyPress, 'key'>> = {}): KeyPress => ({
  key,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});
const editor: KeyContext = { keyboardView: false, typing: false, overlay: false };
const id = (e: KeyPress, ctx = editor) => matchShortcut(e, ctx)?.id ?? null;

describe('matching', () => {
  it('finds the editor shortcuts', () => {
    expect(id(press('s', { ctrlKey: true }))).toBe('save');
    expect(id(press('z', { ctrlKey: true }))).toBe('undo');
    expect(id(press('Z', { ctrlKey: true, shiftKey: true }))).toBe('redo');
    expect(id(press('y', { ctrlKey: true }))).toBe('redo');
    expect(id(press('c', { metaKey: true }))).toBe('copy');
    expect(id(press('v', { ctrlKey: true }))).toBe('paste');
    expect(id(press('Delete'))).toBe('clear');
    expect(id(press('Escape'))).toBe('deselect');
    expect(id(press('F2'))).toBe('rename');
    expect(id(press('d', { ctrlKey: true }))).toBe('duplicate');
    expect(id(press('F1'))).toBe('help');
    expect(id(press('/', { ctrlKey: true, shiftKey: true }))).toBe('help');
  });

  it('layers by digit, Main first', () => {
    expect(matchShortcut(press('1', { ctrlKey: true }), editor)).toEqual({ id: 'layer', digit: 1 });
    expect(matchShortcut(press('9', { ctrlKey: true }), editor)).toEqual({ id: 'layer', digit: 9 });
    // Ctrl+0 is left for the window size.
    expect(id(press('0', { ctrlKey: true }))).toBe('zoom_reset');
    expect(id(press('3'))).toBeNull();
  });

  it('sizes the window from anywhere, with either plus key', () => {
    const typing = { ...editor, typing: true, overlay: true };
    expect(id(press('+', { ctrlKey: true, shiftKey: true }), typing)).toBe('zoom_in');
    expect(id(press('=', { ctrlKey: true }))).toBe('zoom_in');
    expect(id(press('-', { ctrlKey: true }), typing)).toBe('zoom_out');
    expect(id(press('0', { ctrlKey: true }), typing)).toBe('zoom_reset');
  });

  it('needs the exact modifiers; Alt and AltGr never match', () => {
    expect(id(press('s'))).toBeNull();
    expect(id(press('Delete', { ctrlKey: true }))).toBeNull();
    expect(id(press('z', { ctrlKey: true, altKey: true }))).toBeNull();
    expect(id(press('c', { ctrlKey: true, shiftKey: true }))).toBeNull();
  });

  it('text fields keep their keys, except save', () => {
    const typing = { ...editor, typing: true };
    expect(id(press('z', { ctrlKey: true }), typing)).toBeNull();
    expect(id(press('Delete'), typing)).toBeNull();
    expect(id(press('c', { ctrlKey: true }), typing)).toBeNull();
    expect(id(press('s', { ctrlKey: true }), typing)).toBe('save');
  });

  it('dialogs and menus own the keyboard, except save', () => {
    const overlay = { ...editor, overlay: true };
    expect(id(press('Escape'), overlay)).toBeNull();
    expect(id(press('z', { ctrlKey: true }), overlay)).toBeNull();
    expect(id(press('s', { ctrlKey: true }), overlay)).toBe('save');
  });

  it('the keyboard view leaves plain keys to the key selection', () => {
    const kb = { ...editor, keyboardView: true };
    expect(id(press('Delete'), kb)).toBeNull();
    expect(id(press('Delete', { ctrlKey: true }), kb)).toBe('clear');
    expect(id(press('Escape'), kb)).toBeNull();
    expect(id(press('F2'), kb)).toBeNull();
    expect(id(press('F1'), kb)).toBeNull();
    expect(id(press('/', { ctrlKey: true }), kb)).toBe('help');
    expect(id(press('z', { ctrlKey: true }), kb)).toBe('undo');
  });
});

describe('labels', () => {
  it('reads like the keyboard, per language and platform', () => {
    setLanguage('en');
    expect(shortcutLabel('redo', false)).toBe('Ctrl+Y');
    expect(keysLabel({ key: 'z', mod: true, shift: true }, false)).toBe('Ctrl+Shift+Z');
    expect(keysLabel({ key: 'z', mod: true, shift: true }, true)).toBe('⇧⌘Z');
    setLanguage('de');
    expect(shortcutLabel('clear', false)).toBe('Entf');
    expect(keysLabel({ key: 'Delete', mod: true }, false)).toBe('Strg+Entf');
    setLanguage('en');
  });

  it('every shortcut has a description in both languages', () => {
    expect(new Set(SHORTCUTS.map((s) => s.id)).size).toBe(SHORTCUTS.length);
    for (const lang of ['en', 'de']) {
      setLanguage(lang);
      expect(SHORTCUTS.filter((s) => !has(`shortcuts_list.${s.id}`)).map((s) => s.id)).toEqual([]);
    }
    setLanguage('en');
  });
});
