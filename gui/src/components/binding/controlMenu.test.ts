// The context menu's entries against the app state (bridge faked; nothing is called).

import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ Channel: class {}, invoke: async () => null }));

const { app } = await import('../../lib/state/app.svelte');
const { copyAt, menuItems, pasteAt } = await import('./controlMenu');

const labels = (c: string) => menuItems(c).map((i) => (i.separator ? '---' : `${i.label}${i.disabled ? ' (off)' : ''}`));

describe('control menu', () => {
  it('a button has copy, paste and clear; paste waits for a copy', () => {
    app.profile = { name: 'T', mappings: [{ trigger: 'A', actions: [{ key: 'Space' }] }] };
    app.clipboard = null;
    expect(labels('A')).toEqual(['Copy', 'Paste (off)', 'Clear']);
    expect(labels('B')).toEqual(['Copy', 'Paste (off)', 'Clear (off)']);
    copyAt('A');
    expect(labels('B')).toEqual(['Copy', 'Paste (A)', 'Clear (off)']);
    pasteAt('B');
    expect(app.profile.mappings.filter((m) => m.trigger === 'B')).toEqual([{ trigger: 'B', actions: [{ key: 'Space' }] }]);
  });

  it('a stick click names itself and adds the stick’s directions', () => {
    app.profile = { name: 'T', mappings: [] };
    app.clipboard = null;
    expect(labels('LS')).toEqual([
      'Copy Left stick click',
      'Paste (off)',
      'Clear Left stick click (off)',
      '---',
      'Copy all directions',
      'Paste all directions (off)',
      'Clear all directions (off)',
    ]);
  });

  it('a direction offers its group too', () => {
    app.profile = { name: 'T', mappings: [{ trigger: 'DpadUp', actions: [{ key: 'W' }] }] };
    expect(labels('DpadUp').slice(3)).toEqual(['---', 'Copy all directions', 'Paste all directions (off)', 'Clear all directions']);
  });

  it('a read-only config can be copied from, not changed', () => {
    app.profile = { name: 'T', mappings: [{ trigger: 'A', actions: [{ key: 'Space' }] }] };
    app.readOnly = true;
    copyAt('A');
    expect(labels('A')).toEqual(['Copy', 'Paste (A) (off)', 'Clear (off)']);
    app.readOnly = false;
  });
});
