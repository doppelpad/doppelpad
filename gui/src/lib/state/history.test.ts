import { describe, expect, it } from 'vitest';
import { FIELD_MERGE_MS, HISTORY_LIMIT, History } from './history';
import { isStreamingField, isTextEntry } from '../dom';

describe('history', () => {
  it('undoes and redoes steps in order', () => {
    const h = new History();
    h.record('a', null, 0);
    h.record('b', null, 10);
    expect(h.undo('c')).toBe('b');
    expect(h.undo('b')).toBe('a');
    expect(h.undo('a')).toBeNull();
    expect(h.canUndo).toBe(false);
    expect(h.redo('a')).toBe('b');
    expect(h.redo('b')).toBe('c');
    expect(h.redo('c')).toBeNull();
  });

  it('a new step drops what could be redone', () => {
    const h = new History();
    h.record('a', null, 0);
    h.undo('b');
    expect(h.canRedo).toBe(true);
    h.record('a', null, 5);
    expect(h.canRedo).toBe(false);
  });

  it('one gesture is one step however long it takes', () => {
    const h = new History();
    const drag = { kind: 'gesture', id: 'pointer:1' } as const;
    h.record('a', drag, 0);
    h.record('b', drag, 5000);
    h.record('c', { kind: 'gesture', id: 'pointer:1' }, 9000);
    expect(h.undo('d')).toBe('a');
    expect(h.canUndo).toBe(false);
  });

  it('a field merges only while its changes come quickly', () => {
    const h = new History();
    const input = {};
    h.record('a', { kind: 'field', id: input }, 0);
    h.record('b', { kind: 'field', id: input }, FIELD_MERGE_MS - 1);
    h.record('c', { kind: 'field', id: input }, 3 * FIELD_MERGE_MS);
    expect(h.undo('d')).toBe('c');
    expect(h.undo('c')).toBe('a');
  });

  it('different keys and clicks are steps of their own', () => {
    const h = new History();
    h.record('a', { kind: 'field', id: 1 }, 0);
    h.record('b', { kind: 'field', id: 2 }, 1);
    h.record('c', null, 2);
    h.record('d', null, 3);
    expect([h.undo('e'), h.undo('d'), h.undo('c'), h.undo('b')]).toEqual(['d', 'c', 'b', 'a']);
  });

  it('after undo the same field starts a new step', () => {
    const h = new History();
    const key = { kind: 'field', id: 'x' } as const;
    h.record('a', key, 0);
    h.undo('b');
    h.record('a', key, 1);
    h.record('a2', key, 2);
    expect(h.undo('a3')).toBe('a');
    expect(h.undo('a')).toBeNull();
  });

  it('keeps at most the limit, dropping the oldest', () => {
    const h = new History();
    for (let i = 0; i < HISTORY_LIMIT + 5; i++) h.record(String(i), null, i);
    let last = '';
    let n = 0;
    for (let s = h.undo('now'); s !== null; s = h.undo(s)) {
      last = s;
      n++;
    }
    expect(n).toBe(HISTORY_LIMIT);
    expect(last).toBe('5');
  });

  it('clear forgets everything', () => {
    const h = new History();
    h.record('a', null, 0);
    h.undo('b');
    h.record('c', null, 1);
    h.clear();
    expect(h.canUndo || h.canRedo).toBe(false);
  });
});

describe('fields', () => {
  it('text entry and streaming fields by tag and type', () => {
    expect(isTextEntry({ tagName: 'INPUT', type: 'text' })).toBe(true);
    expect(isTextEntry({ tagName: 'INPUT', type: 'number' })).toBe(true);
    expect(isTextEntry({ tagName: 'TEXTAREA' })).toBe(true);
    expect(isTextEntry({ tagName: 'DIV', isContentEditable: true })).toBe(true);
    expect(isTextEntry({ tagName: 'INPUT', type: 'range' })).toBe(false);
    expect(isTextEntry({ tagName: 'INPUT', type: 'checkbox' })).toBe(false);
    expect(isTextEntry({ tagName: 'BUTTON' })).toBe(false);
    expect(isTextEntry(null)).toBe(false);
    expect(isStreamingField({ tagName: 'INPUT', type: 'range' })).toBe(true);
    expect(isStreamingField({ tagName: 'INPUT', type: 'color' })).toBe(true);
    expect(isStreamingField({ tagName: 'INPUT', type: 'checkbox' })).toBe(false);
    expect(isStreamingField({ tagName: 'SELECT' })).toBe(false);
  });
});
