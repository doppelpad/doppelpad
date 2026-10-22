// The app state against a fake bridge: settings writes that overlap, undo and redo.

import { describe, expect, it, vi } from 'vitest';
import type { GuiSettings, LoadedConfig, Profile } from '../types';

/** What the bridge stored, write by write; each write takes a moment, like the real one. */
const stored: GuiSettings[] = [];
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {},
  invoke: async (cmd: string, args?: { settings?: GuiSettings; path?: string }) => {
    if (cmd === 'config_load') return loaded(args?.path ?? '');
    if (cmd !== 'settings_set' || !args?.settings) return null;
    await new Promise((r) => setTimeout(r, 5));
    stored.push(args.settings);
  },
}));

/** A config as the bridge loads it; paths with "ro" in them are read-only. */
function loaded(path: string): LoadedConfig {
  const profile: Profile = { name: path, mappings: [{ trigger: 'A', actions: [{ key: 'Space' }] }] };
  return { profile, diagnostics: [], read_only: path.includes('ro') };
}

const { app } = await import('./app.svelte');
const { addLayer } = await import('../model/profile');

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

describe('undo and redo', () => {
  const rename = (name: string) => app.edit((p) => ({ ...p, name }));

  it('undo brings back the loaded config and redo the change', async () => {
    await app.openConfig('one.toml', true);
    expect(app.canUndo).toBe(false);
    rename('changed');
    expect(app.dirty).toBe(true);
    expect(app.canUndo).toBe(true);
    app.undo();
    expect(app.profile?.name).toBe('one.toml');
    expect(app.dirty).toBe(false);
    expect(app.canRedo).toBe(true);
    app.redo();
    expect(app.profile?.name).toBe('changed');
    expect(app.canRedo).toBe(false);
  });

  it('a change that changes nothing is no step', async () => {
    await app.openConfig('one.toml', true);
    app.edit((p) => p);
    app.update({ ...app.profile! });
    expect(app.canUndo).toBe(false);
  });

  it('changes made in place are undone too', async () => {
    await app.openConfig('one.toml', true);
    app.edit((p) => {
      p.mappings[0]!.actions = [{ key: 'E' }];
      return p;
    });
    app.undo();
    expect(app.profile?.mappings[0]?.actions).toEqual([{ key: 'Space' }]);
  });

  it('opening another config forgets the steps', async () => {
    await app.openConfig('one.toml', true);
    rename('changed');
    await app.openConfig('two.toml', true);
    expect(app.canUndo).toBe(false);
    app.undo();
    expect(app.profile?.name).toBe('two.toml');
  });

  it('a read-only config records nothing', async () => {
    await app.openConfig('ro.json', true);
    rename('changed');
    expect(app.canUndo).toBe(false);
    expect(app.profile?.name).toBe('ro.json');
  });

  it('an edit group is one step', async () => {
    await app.openConfig('one.toml', true);
    app.beginEditGroup();
    rename('a');
    rename('b');
    rename('c');
    app.endEditGroup();
    rename('d');
    app.undo();
    expect(app.profile?.name).toBe('c');
    app.undo();
    expect(app.profile?.name).toBe('one.toml');
  });

  it('undoing a new layer leaves it for Main', async () => {
    await app.openConfig('one.toml', true);
    const { profile, id } = addLayer(app.profile!);
    app.update(profile);
    app.layer = id;
    app.undo();
    expect(app.layer).toBe(0);
  });
});

describe('questions', () => {
  it('a silenced question answers OK without asking; others still ask', async () => {
    await app.silence('clear_mappings');
    expect(app.settings.silenced).toContain('clear_mappings');
    await expect(app.confirmAsk('Clear?', { id: 'clear_mappings' })).resolves.toBe(true);
    expect(app.question).toBeNull();
    const pending = app.confirmAsk('Replace?', { id: 'whole_stick_replace' });
    expect(app.question?.id).toBe('whole_stick_replace');
    app.question?.resolve(null);
    await expect(pending).resolves.toBe(false);
    await app.silence('clear_mappings', false);
    expect(app.settings.silenced).not.toContain('clear_mappings');
    app.confirmAsk('Clear?', { id: 'clear_mappings' });
    expect(app.question?.message).toBe('Clear?');
    app.question?.resolve(null);
    app.question = null;
  });

  it('every question that can be silenced has a name in the settings', async () => {
    const { QUESTION_IDS } = await import('./app.svelte');
    const { has } = await import('../i18n/index.svelte');
    expect(QUESTION_IDS.filter((id) => !has(`questions.ids.${id}`))).toEqual([]);
  });
});
