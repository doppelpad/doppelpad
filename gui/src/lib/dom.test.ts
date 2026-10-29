import { describe, expect, it, vi } from 'vitest';
import { dialogStack, isStreamingField, isTextEntry, keyboardOwned } from './dom';

/** Which selectors match an element on the page. */
const open = new Set<string>();
vi.stubGlobal('document', { querySelector: (s: string) => (open.has(s) ? {} : null) });

describe('keys and fields', () => {
  it('tells text entry from other fields', () => {
    expect(isTextEntry({ tagName: 'INPUT', type: 'text' })).toBe(true);
    expect(isTextEntry({ tagName: 'INPUT', type: 'checkbox' })).toBe(false);
    expect(isTextEntry({ tagName: 'DIV', isContentEditable: true })).toBe(true);
    expect(isStreamingField({ tagName: 'INPUT', type: 'range' })).toBe(true);
  });

  it('the keys belong to a field, a dialog, a menu or a popover, else to the view', () => {
    const button = { tagName: 'BUTTON' };
    expect(keyboardOwned(button)).toBe(false);
    expect(keyboardOwned({ tagName: 'INPUT', type: 'checkbox' })).toBe(true);
    expect(keyboardOwned({ tagName: 'TEXTAREA' })).toBe(true);
    open.add('[data-popover]');
    expect(keyboardOwned(button)).toBe(true);
    open.clear();
    const dialog = Symbol('dialog');
    dialogStack.push(dialog);
    expect(keyboardOwned(button)).toBe(true);
    dialogStack.pop();
    expect(keyboardOwned(button)).toBe(false);
  });
});
