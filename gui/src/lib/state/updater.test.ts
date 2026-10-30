// The update check against a fake bridge.

import { describe, expect, it, vi } from 'vitest';
import type { UpdateInfo } from '../types';

/** What the bridge's update check answers next; an Error is thrown. */
let next: UpdateInfo | null | Error = null;
const installs: string[] = [];
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {},
  invoke: async (cmd: string) => {
    if (cmd === 'update_check') {
      if (next instanceof Error) throw next;
      return next;
    }
    if (cmd === 'update_install') installs.push(cmd);
    return null;
  },
}));

const { Updater } = await import('./updater.svelte');

function updater(answer: boolean) {
  const toasts: string[] = [];
  const asked: string[] = [];
  const u = new Updater({
    toast: (text) => toasts.push(text),
    confirmAsk: async (message) => {
      asked.push(message);
      return answer;
    },
    confirmDiscard: async () => true,
  });
  return { u, toasts, asked };
}

describe('updates', () => {
  it('at start-up, no update and a failed check say nothing; Check now says both', async () => {
    const { u, toasts } = updater(false);
    next = null;
    await u.check();
    next = new Error('offline');
    await u.check();
    expect(toasts).toEqual([]);
    await u.check(true);
    next = null;
    await u.check(true);
    expect(toasts).toHaveLength(2);
    expect(toasts[0]).toContain('offline');
    expect(u.busy).toBe(false);
  });

  it('a newer version is offered, and installed only when the answer is yes', async () => {
    next = { version: '9.9.9', current: '0.7.0', notes: null } as UpdateInfo;
    const later = updater(false);
    await later.u.check();
    expect(later.asked).toHaveLength(1);
    expect(installs).toEqual([]);
    const yes = updater(true);
    await yes.u.check();
    expect(installs).toEqual(['update_install']);
    expect(yes.u.progress).toBeNull();
  });
});
