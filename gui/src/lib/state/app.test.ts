// The app state against a fake bridge: start-up, settings writes that overlap, undo and redo,
// opening and saving configs. Each test starts from a new state.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Game, GuiSettings, LoadedConfig, Profile } from '../types';

/** What the bridge stored, write by write; each write takes a moment, like the real one. */
const stored: GuiSettings[] = [];
/** Commands that fail. */
const failing = new Set<string>();
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {},
  invoke: async (cmd: string, args?: { settings?: GuiSettings; path?: string }) => {
    if (failing.has(cmd)) throw new Error(`${cmd} failed`);
    if (cmd === 'config_load') return loaded(args?.path ?? '');
    if (cmd === 'library_scan') return { games: [game], unsorted: [], errors: [] };
    if (cmd === 'settings_get') return { ...SETTINGS_PLACEHOLDER, check_updates: false };
    if (cmd === 'take_notices' || cmd === 'take_open_files') return [];
    if (cmd === 'config_save') {
      await new Promise((r) => setTimeout(r, 5));
      return [];
    }
    if (cmd !== 'settings_set' || !args?.settings) return null;
    await new Promise((r) => setTimeout(r, 5));
    stored.push(args.settings);
  },
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));

/** A config as the bridge loads it; paths with "ro" in them are read-only. */
function loaded(path: string): LoadedConfig {
  const profile: Profile = { name: path, mappings: [{ trigger: 'A', actions: [{ key: 'Space' }] }] };
  return { profile, diagnostics: [], read_only: path.includes('ro') };
}

const { AppState } = await import('./app.svelte');
const { SETTINGS_PLACEHOLDER } = await import('./settings');
const { addLayer } = await import('../model/profile');

let app: InstanceType<typeof AppState>;
beforeEach(() => {
  app = new AppState();
  failing.clear();
});

/** A game with two configs. */
const game: Game = {
  dir: 'game',
  meta: { name: 'Game', processes: [] },
  configs: [
    { path: 'one.toml', name: 'one', error: null },
    { path: 'two.toml', name: 'two', error: null },
  ],
};

const rename = (name: string) => app.edit((p) => ({ ...p, name }));

describe('start-up', () => {
  it('a step that fails says so, and the others still run', async () => {
    failing.add('hotkeys_get').add('catalog');
    await app.init();
    expect(app.toasts.map((x) => x.text)).toEqual(['catalog failed', 'hotkeys_get failed']);
    expect(app.library.games).toEqual([game]);
    expect(app.configPath).toBe('one.toml');
  });
});

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

describe('opening and saving', () => {
  it('choosing the open game again keeps its unsaved changes', async () => {
    app.library = { games: [game], unsorted: [], errors: [] };
    await app.selectGame('game');
    expect(app.configPath).toBe('one.toml');
    rename('changed');
    await app.selectGame('game');
    expect(app.profile?.name).toBe('changed');
    expect(app.dirty).toBe(true);
    expect(app.canUndo).toBe(true);
  });

  it('a change made while saving stays unsaved', async () => {
    await app.openConfig('one.toml', true);
    rename('saved');
    const saving = app.save();
    rename('typed meanwhile');
    await expect(saving).resolves.toBe(true);
    expect(app.dirty).toBe(true);
    app.undo();
    expect(app.dirty).toBe(false);
  });
});

describe('questions', () => {
  it('a silenced question answers OK without asking; others still ask', async () => {
    await app.silence('clear_mappings');
    expect(app.settings.silenced).toContain('clear_mappings');
    await expect(app.confirmAsk('Clear?', { id: 'clear_mappings' })).resolves.toBe(true);
    expect(app.dialogs.question).toBeNull();
    const pending = app.confirmAsk('Replace?', { id: 'whole_stick_replace' });
    expect(app.dialogs.question?.id).toBe('whole_stick_replace');
    app.dialogs.answerQuestion(null);
    await expect(pending).resolves.toBe(false);
    await app.silence('clear_mappings', false);
    expect(app.settings.silenced).not.toContain('clear_mappings');
    app.confirmAsk('Clear?', { id: 'clear_mappings' });
    expect(app.dialogs.question?.message).toBe('Clear?');
  });

  it('OK with "Don\'t ask again" silences the question in the settings', async () => {
    const pending = app.confirmAsk('Replace?', { id: 'whole_stick_replace' });
    app.dialogs.answerQuestion('ok', true);
    await expect(pending).resolves.toBe(true);
    await vi.waitFor(() => expect(app.settings.silenced).toContain('whole_stick_replace'));
  });
});

describe('going to a place in the config', () => {
  it('a mapping shows in the view that has its control, on its layer and activator', async () => {
    await app.openConfig('one.toml', true);
    app.view = 'output';
    app.reveal({ kind: 'mapping', layer: 1, trigger: 'Key:W', activator: 'double' });
    expect([app.view, app.layer, app.activator, app.selected]).toEqual(['keyboard', 1, 'double', 'Key:W']);
    app.reveal({ kind: 'mapping', layer: 0, trigger: ['LB', 'RB'], activator: 'single' });
    expect([app.view, app.layer, app.activator, app.selected]).toEqual(['shortcuts', 0, 'single', ['LB', 'RB']]);
  });

  it('a layer shows in a view that shows layers, a radial menu in its own', async () => {
    await app.openConfig('one.toml', true);
    app.view = 'keyboard';
    app.reveal({ kind: 'layer', layer: 2 });
    expect([app.view, app.layer]).toEqual(['keyboard', 2]);
    app.view = 'output';
    app.reveal({ kind: 'layer', layer: 1 });
    expect([app.view, app.layer]).toEqual(['front', 1]);
    app.reveal({ kind: 'radial', menu: 1, sector: 3 });
    expect([app.view, app.radialMenu, app.radialSector]).toEqual(['radial', 1, 3]);
  });
});
