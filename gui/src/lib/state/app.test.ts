// The app state against a fake bridge: start-up, settings writes that overlap, undo and redo,
// opening and saving configs. Each test starts from a new state.

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Game, GuiSettings, Hotkeys, LiveState, LoadedConfig, Profile } from '../types';

/** What the bridge stored, write by write; each write takes a moment, like the real one. */
const stored: GuiSettings[] = [];
/** The hotkeys the bridge stored, write by write. */
const storedHotkeys: Hotkeys[] = [];
/** Commands that fail. */
const failing = new Set<string>();
/** Sends a live state to the app, once it has subscribed. */
let sendLive: (s: LiveState) => void = () => {};
vi.mock('@tauri-apps/api/core', () => ({
  Channel: class {},
  invoke: async (cmd: string, args?: { settings?: GuiSettings; path?: string; hotkeys?: Hotkeys }) => {
    if (failing.has(cmd)) throw new Error(`${cmd} failed`);
    if (cmd === 'live_subscribe') {
      const channel = (args as unknown as { onEvent: { onmessage: (s: LiveState) => void } }).onEvent;
      sendLive = (s) => channel.onmessage(s);
      return null;
    }
    if (cmd === 'hotkeys_get') return hotkeys;
    if (cmd === 'hotkeys_set') {
      await new Promise((r) => setTimeout(r, 5));
      storedHotkeys.push(args!.hotkeys!);
      return null;
    }
    if (cmd === 'config_load') {
      // A slow disk for paths that say so.
      if (args?.path?.includes('slow')) await new Promise((r) => setTimeout(r, 20));
      return loaded(args?.path ?? '');
    }
    if (cmd === 'config_check') {
      // The check of a config named "slow" takes longer than the ones after it.
      if ((args as { profile?: Profile } | undefined)?.profile?.name === 'slow') await new Promise((r) => setTimeout(r, 500));
      return [{ severity: 'warning', path: '', message: (args as { profile: Profile }).profile.name }];
    }
    if (cmd === 'library_scan') return { games: [game], unsorted: [], errors: [] };
    if (cmd === 'settings_get') return { ...SETTINGS_PLACEHOLDER, check_updates: false };
    if (cmd === 'take_notices' || cmd === 'take_open_files' || cmd === 'config_import_launches') return [];
    if (cmd === 'config_import') return { path: 'imported.toml', diagnostics: [{ severity: 'warning', path: '', message: 'converted' }] };
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

const hotkeys: Hotkeys = {
  hold_ms: 500,
  slot1: [],
  slot2: [],
  slot3: [],
  slot4: [],
  remap: [],
  stop_macros: [],
  emergency_stop: [],
  pad_overlay: [],
  mappings_overlay: [],
  app_navigation: [],
  rumble: false,
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

  it('the controller navigation hotkey twice within one round trip turns it on and off again', async () => {
    await app.init();
    const live = (presses: number) =>
      ({ devices: [], device_slots: [], sessions: [], active_slot: 0, app_navigation: presses }) as unknown as LiveState;
    sendLive(live(0));
    sendLive(live(1));
    sendLive(live(2));
    // Waits for the writes before it.
    await app.setSettings({});
    expect(app.settings.pad_navigation).toBe(false);
  });

  it('a change of one part of a nested setting builds on the change before it', async () => {
    const osd = app.settings.osd;
    await Promise.all([
      app.setSettings((s) => ({ osd: { ...s.osd, remap: !osd.remap } })),
      app.setSettings((s) => ({ osd: { ...s.osd, slot: !osd.slot } })),
    ]);
    expect(app.settings.osd).toMatchObject({ remap: !osd.remap, slot: !osd.slot });
  });

  it('two hotkey changes within one round trip both stay', async () => {
    await app.refreshHotkeys();
    await Promise.all([app.setHotkeys({ slot1: ['A', 'B'] }), app.setHotkeys((h) => ({ hold_ms: h.hold_ms + 100 }))]);
    expect(app.hotkeys).toMatchObject({ slot1: ['A', 'B'], hold_ms: 600 });
    expect(storedHotkeys.at(-1)).toMatchObject({ slot1: ['A', 'B'], hold_ms: 600 });
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

  it('an edit turns the config into JSON once', async () => {
    await app.openConfig('one.toml', true);
    const stringify = vi.spyOn(JSON, 'stringify');
    rename('changed');
    expect(app.dirty).toBe(true);
    expect(stringify).toHaveBeenCalledTimes(1);
    stringify.mockRestore();
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

describe('games', () => {
  it('deleting the open game asks, then closes its config', async () => {
    app.library = { games: [game], unsorted: [], errors: [] };
    await app.selectGame('game');
    const deleting = app.deleteGame(game);
    expect(app.dialogs.question?.message).toContain('Game');
    app.dialogs.answerQuestion('ok');
    await deleting;
    expect([app.gameDir, app.configPath, app.profile]).toEqual([null, null, null]);
  });

  it('a cancelled delete leaves everything as it was', async () => {
    app.library = { games: [game], unsorted: [], errors: [] };
    await app.selectGame('game');
    const deleting = app.deleteGame(game);
    app.dialogs.answerQuestion(null);
    await deleting;
    expect([app.gameDir, app.configPath]).toEqual(['game', 'one.toml']);
  });
});

describe('importing', () => {
  it('a bundle with configs that cannot be checked asks even without programs, and lists them', async () => {
    const keep = app.keepLaunches([], [{ file: 'new.toml', error: 'made by a newer Doppelpad' }]);
    expect(app.dialogs.question?.items?.map((i) => i.code)).toEqual(['new.toml']);
    expect(app.dialogs.question?.items?.[0]?.notes[0]).toContain('made by a newer Doppelpad');
    app.dialogs.answerQuestion('ok');
    expect(await keep).toBe(false);
    expect(await app.keepLaunches([], [])).toBe(false);
  });

  it("a profile file without programs to start opens as a new config, with the converter's notes", async () => {
    app.library = { games: [game], unsorted: [], errors: [] };
    await expect(app.importConfig('game', 'C:/old.rewasd')).resolves.toBe(true);
    expect(app.configPath).toBe('imported.toml');
    expect(app.toasts.map((x) => x.text)).toContain('converted');
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

  it('a config opened later wins over a slower one opened before it', async () => {
    const slow = app.openConfig('slow.toml', true);
    await app.openConfig('fast.toml', true);
    await slow;
    expect([app.configPath, app.profile?.name]).toEqual(['fast.toml', 'fast.toml']);
  });

  it('the findings of the latest check stay, also when an older check answers later', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      await app.openConfig('one.toml', true);
      rename('slow');
      await vi.advanceTimersByTimeAsync(300);
      rename('quick');
      await vi.advanceTimersByTimeAsync(300);
      await vi.advanceTimersByTimeAsync(500);
      expect(app.diagnostics.map((d) => d.message)).toEqual(['quick']);
    } finally {
      vi.useRealTimers();
    }
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
