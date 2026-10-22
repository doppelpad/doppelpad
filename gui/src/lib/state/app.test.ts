// The app state against a fake bridge: settings writes that overlap.

import { describe, expect, it, vi } from 'vitest';
import type { GuiSettings } from '../types';

/** What the bridge stored, write by write; each write takes a moment, like the real one. */
const stored: GuiSettings[] = [];
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {},
  invoke: async (cmd: string, args?: { settings?: GuiSettings }) => {
    if (cmd !== 'settings_set' || !args?.settings) return null;
    await new Promise((r) => setTimeout(r, 5));
    stored.push(args.settings);
  },
}));

const { app } = await import('./app.svelte');

describe('settings', () => {
  it('two changes within one round trip both stay', async () => {
    const before = app.settings;
    await Promise.all([app.setSettings({ stay_live: !before.stay_live }), app.setSettings({ hook_buttons: !before.hook_buttons })]);
    expect(app.settings.stay_live).toBe(!before.stay_live);
    expect(app.settings.hook_buttons).toBe(!before.hook_buttons);
    expect(stored.at(-1)).toMatchObject({ stay_live: !before.stay_live, hook_buttons: !before.hook_buttons });
  });

  it('a change of one part of a nested setting builds on the change before it', async () => {
    const osd = app.settings.osd;
    await Promise.all([
      app.setSettings((s) => ({ osd: { ...s.osd, remap: !osd.remap } })),
      app.setSettings((s) => ({ osd: { ...s.osd, slot: !osd.slot } })),
    ]);
    expect(app.settings.osd).toMatchObject({ remap: !osd.remap, slot: !osd.slot });
  });
});
