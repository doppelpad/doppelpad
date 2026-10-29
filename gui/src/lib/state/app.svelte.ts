// Central application state (Svelte 5 runes). Components read and mutate `app`.

import { listen } from '@tauri-apps/api/event';
import { api, errorText } from '../api';
import { setLanguage, t } from '../i18n/index.svelte';
import type { Side } from '../model/analog';
import { controlsFromPad, drawingsFor, sideShowing } from '../model/devices';
import { setStickGroup, stickGroupConflicts, stickGroupOf, type StickGroup } from '../model/stickgroups';
import { comboKey, comboList, editMappingIn, layerIds, mappingAt, type LayerCopy } from '../model/profile';
import { VIEWS, viewFor, type AnalogTab, type OutputPage, type Place, type View } from '../model/views';
import { layerLabel } from '../model/labels';
import { launchRows } from '../model/launches';
import { clone } from '../model/util';
import { isStreamingField, pointer } from '../dom';
import { movedPath } from '../paths';
import { padMode } from '../padnav';
import { Dialogs, type Answer, type QuestionId, type QuestionOptions } from './dialogs.svelte';
import { History, type EditKey } from './history';
import { PadInput } from './padInput.svelte';
import { flagsOf, hotkeyClashes, waitsForSave, type Flags } from '../model/lints';
import type { Copied } from '../model/clipboard';
import type { MenuItem } from '../menu';
import { SETTINGS_PLACEHOLDER } from './settings';
import { Updater } from './updater.svelte';
import type {
  Action,
  Activator,
  Catalog,
  Combo,
  ControlName,
  DiagMsg,
  Game,
  GuiSettings,
  Hotkeys,
  Launch,
  Library,
  LiveDevice,
  LiveState,
  Mapping,
  PadFamily,
  Profile,
  RewasdStatus,
} from '../types';

export interface Toast {
  id: number;
  text: string;
  kind: 'info' | 'ok' | 'error';
}

export interface ContextMenuRequest {
  items: MenuItem[];
  /** Window coordinates of its top left corner (it moves to stay inside the window). */
  x: number;
  y: number;
  /** Gets the focus back when the menu closes without a choice. */
  from: HTMLElement | null;
}

/**
 * Runs writes one after the other: each starts when the one before has settled, so it sees
 * its result. One that throws does not stop those after it.
 */
function writeChain() {
  let last: Promise<void> = Promise.resolve();
  return (write: () => Promise<void>): Promise<void> => {
    const next = last.then(write);
    last = next.catch(() => {});
    return next;
  };
}

export class AppState {
  catalog = $state<Catalog | null>(null);
  library = $state<Library>({ games: [], unsorted: [], errors: [] });
  live = $state<LiveState | null>(null);
  /** Placeholder until the app's settings arrive. */
  settings = $state<GuiSettings>(clone(SETTINGS_PLACEHOLDER));
  /** `null` when reWASD is not installed. */
  rewasd = $state<RewasdStatus | null>(null);
  /** The start-up question whether to quit reWASD is open. */
  rewasdAsk = $state(false);
  rewasdQuitting = $state(false);

  gameDir = $state<string | null>(null);
  configPath = $state<string | null>(null);
  profile = $state<Profile | null>(null);
  savedJson = $state('');
  /**
   * The draft as JSON, kept with every change that goes through the app (`commit`, undo, open):
   * the undo steps and `dirty` compare it instead of turning the whole config into JSON again.
   */
  private draftJson = $state('');
  readOnly = $state(false);
  /** The validator's findings for the open config: from loading or saving, then after each change. */
  diagnostics = $state<DiagMsg[]>([]);
  /** The controller hotkeys, for the mappings they clash with. */
  hotkeys = $state<Hotkeys | null>(null);

  layer = $state(0);
  view = $state<View>('front');
  selected = $state<Combo | null>(null);
  /** A whole stick shown in the side panel (its route or group), instead of `selected`. */
  stick = $state<Side | null>(null);
  activator = $state<Activator>('single');
  /** Mapping opened in the macro editor (layer + trigger + activator of it). */
  macroTarget = $state<{ layer: number; trigger: Combo; activator: Activator } | null>(null);
  analogTab = $state<AnalogTab>('left');
  /** The page of the config settings. */
  outputPage = $state<OutputPage>('controller');
  /** The radial menu shown in its view, and its sector in the side panel. */
  radialMenu = $state(0);
  radialSector = $state<number | null>(null);
  applySlot = $state(0);
  /** The controller whose own slots the bottom bar shows; `null` for the shared slots. */
  slotDevice = $state<string | null>(null);
  toasts = $state<Toast[]>([]);
  /** Looking for, offering and installing updates. */
  readonly updater = new Updater({
    toast: (text, kind) => this.toast(text, kind),
    confirmAsk: (message, options) => this.confirmAsk(message, options),
    confirmDiscard: () => this.confirmDiscard(),
  });
  /** The questions, hints and prompts waiting to be answered, one shown at a time. */
  readonly dialogs = new Dialogs({ silenced: () => this.settings.silenced, silence: (id) => this.silence(id) });
  copiedLayer = $state.raw<LayerCopy | null>(null);
  /** A copied control or direction group (see `lib/model/clipboard.ts`); kept across configs. */
  clipboard = $state.raw<Copied | null>(null);
  /** Copied macro steps. */
  copiedSteps = $state.raw<Action[] | null>(null);
  /** The open context menu: its items, where it opens and where the focus goes back to. */
  contextMenu = $state.raw<ContextMenuRequest | null>(null);
  canUndo = $state(false);
  canRedo = $state(false);
  /** The list of keyboard shortcuts is open. */
  shortcutsOpen = $state(false);
  /** The find panel is open: with a query, or listing the mappings that press one key. */
  find = $state<{ query: string; key?: string } | null>(null);

  /** Earlier states of the open config, for undo and redo. */
  private history = new History();
  /** While set, every change belongs to one undo step (e.g. a recording); see `editGroup`. */
  private group: unknown = null;
  /** The primary controller's presses: held controls, captures, controller navigation. */
  readonly pad = new PadInput({ mode: () => padMode(this.settings), select: (c) => this.selectPressed(c) });
  private toastId = 0;
  /** Settings and hotkey writes, each one after the one before, so it builds on its result. */
  private settingsWrites = writeChain();
  private hotkeyWrites = writeChain();

  primary = $derived<LiveDevice | null>(this.live?.devices.find((d) => d.id === this.live?.primary) ?? this.live?.devices[0] ?? null);
  family = $derived<PadFamily | null>(this.primary?.family ?? null);
  dirty = $derived(this.profile !== null && this.draftJson !== this.savedJson);
  /** The layers of the config as saved; one added since is still being set up (see `waitsForSave`). */
  savedLayers = $derived<ReadonlySet<number>>(new Set(this.savedJson ? layerIds(JSON.parse(this.savedJson) as Profile) : []));
  /**
   * Everything the diagnostics bar lists: the validator's findings, except those that wait
   * for the config to be saved, and the hotkey clashes.
   */
  problems = $derived<DiagMsg[]>(
    this.profile
      ? [
          ...this.diagnostics.filter((d) => !waitsForSave(this.profile!, d, this.savedLayers)),
          ...hotkeyClashes(this.profile, this.hotkeys, this.family),
        ]
      : [],
  );
  /** The controls and layers with a warning mark. */
  flags = $derived<Flags>(flagsOf(this.profile, this.problems));
  game = $derived<Game | null>(this.library.games.find((g) => g.dir === this.gameDir) ?? null);
  /** The slot set the bottom bar shows and "Apply to slot" fills: a controller's own or the shared one. */
  slotSet = $derived.by(() => {
    const own = this.live?.device_slots.find((d) => d.id === this.slotDevice);
    const owner = own?.id ?? null;
    return {
      device: owner,
      slots: own?.slots ?? this.live?.slots ?? [null, null, null, null],
      active: own?.active_slot ?? this.live?.active_slot ?? 0,
      session: this.live?.sessions.find((s) => s.owner === owner) ?? null,
    };
  });
  rewasdRunning = $derived(!!this.rewasd && (this.rewasd.service !== null || this.rewasd.programs.length > 0));

  /** The radial menu shown: the last one when the chosen one is gone (undo). */
  radialIndex = $derived(Math.min(this.radialMenu, Math.max(0, (this.profile?.radial_menus?.length ?? 0) - 1)));

  layerLabel = (id: number) => (this.profile ? layerLabel(this.profile, id) : String(id));

  /** Start-up: each step on its own, so one that fails says why and the others still run. */
  async init() {
    const settingsRead = await this.attempt(
      api.settingsGet().then((s) => {
        this.settings = s;
        setLanguage(s.language);
      }),
    );
    await this.attempt(api.catalog().then((c) => (this.catalog = c)));
    // Only for the clash marks: the rest does not wait for the service's answer.
    this.refreshHotkeys();
    await this.attempt(this.refreshLibrary());
    await this.attempt(api.liveSubscribe((s) => this.onLive(s)));
    await this.attempt(api.takeNotices().then((notices) => notices.forEach((n) => this.toast(t(`notices.${n}`), 'info'))));
    // Bundles opened from Explorer: the one the app started with now, later ones as they come.
    await this.attempt(listen<string[]>('open-files', (e) => this.openFiles(e.payload)));
    // Hints the app sends while something happens elsewhere (a game in front).
    await this.attempt(
      listen<{ id: QuestionId; game: string }>('hint', (e) =>
        this.inform(t(`hints.${e.payload.id}`, { game: e.payload.game }), e.payload.id),
      ),
    );
    let opened: string[] = [];
    await this.attempt(api.takeOpenFiles().then((files) => (opened = files)));
    await this.attempt(api.rewasdStatus().then((r) => (this.rewasd = r)));
    if (this.rewasdRunning) {
      if (this.settings.rewasd_at_start === 'quit') this.quitRewasd();
      else if (this.settings.rewasd_at_start === 'ask') {
        this.rewasdAsk = true;
        // The app may have started hidden with Windows.
        await this.attempt(api.showWindow());
      }
    }
    // Open the game of the config that is currently applied (when the first live state is
    // already here), else the first game.
    const applied = this.live?.profile_path;
    const game = this.library.games.find((g) => g.configs.some((c) => c.path === applied)) ?? this.library.games[0];
    if (game) await this.selectGame(game.dir);
    if (opened.length) await this.openFiles(opened);
    // The question waits in the window, also when the app started hidden with Windows.
    if (settingsRead && this.settings.check_updates) this.updater.check();
  }

  private onLive(s: LiveState) {
    const before = this.live;
    const firstSnapshot = before === null;
    this.live = s;
    // The navigation hotkey fired (it counts; the app keeps the mode).
    if (before && s.app_navigation !== before.app_navigation) this.setPadNavigation(!this.settings.pad_navigation);
    if (firstSnapshot) this.applySlot = s.active_slot;
    const dev = this.primary;
    this.pad.update(dev && this.catalog ? controlsFromPad(dev.state, this.catalog.pad_buttons) : new Set<ControlName>());
  }

  /** Controller navigation on or off (bottom bar, hotkey); it replaces press-to-select while on. */
  setPadNavigation(on: boolean) {
    if (on === this.settings.pad_navigation) return;
    this.setSettings({ pad_navigation: on });
    this.toast(t(on ? 'padnav.on' : 'padnav.off'), 'info');
  }

  /** A control pressed on the controller selects it on the controller views, on the side that shows it. */
  private selectPressed(c: ControlName) {
    if (!this.profile || (this.view !== 'front' && this.view !== 'back')) return;
    this.select(c);
    this.view = sideShowing(drawingsFor(this.primary), this.view, c);
  }

  /** Controls held on the primary controller; the drawing highlights exactly these. */
  get pressed(): ReadonlySet<ControlName> {
    return this.pad.pressed;
  }

  /** The controller's presses go to `fn` instead of the editor until the returned function is called (see `PadInput.capture`). */
  capturePresses(fn?: (c: ControlName) => void): () => void {
    return this.pad.capture(fn);
  }

  /** Await `p`; when it fails, show why, else show `ok` if given. Returns whether it worked. */
  async attempt(p: Promise<unknown>, ok?: string): Promise<boolean> {
    try {
      await p;
    } catch (e) {
      this.toast(errorText(e), 'error');
      return false;
    }
    if (ok) this.toast(ok, 'ok');
    return true;
  }

  /**
   * Change some settings. Writes go one after the other; a function patch sees the settings as
   * the writes before it left them (for a part of a nested setting).
   */
  setSettings(patch: Partial<GuiSettings> | ((s: GuiSettings) => Partial<GuiSettings>)): Promise<void> {
    return this.settingsWrites(async () => {
      const next = { ...this.settings, ...(typeof patch === 'function' ? patch(this.settings) : patch) };
      if (await this.attempt(api.settingsSet(next))) {
        this.settings = next;
        setLanguage(next.language);
      }
    });
  }

  /** Read the controller hotkeys from the service again. */
  refreshHotkeys(): Promise<boolean> {
    return this.attempt(api.hotkeysGet().then((h) => (this.hotkeys = h)));
  }

  /** Change some controller hotkeys; like the settings, writes go one after the other. */
  setHotkeys(patch: Partial<Hotkeys> | ((h: Hotkeys) => Partial<Hotkeys>)): Promise<void> {
    return this.hotkeyWrites(async () => {
      if (!this.hotkeys) return;
      const next = { ...this.hotkeys, ...(typeof patch === 'function' ? patch(this.hotkeys) : patch) };
      if (await this.attempt(api.hotkeysSet(next))) this.hotkeys = next;
    });
  }

  async refreshLibrary() {
    this.library = await api.libraryScan();
  }

  /**
   * The library moved from `from` (`null`: another library is used and nothing moved) to `to`:
   * the open game and config, unsaved changes included, follow their files.
   */
  async libraryMoved(from: string | null, to: string) {
    if (from) {
      if (this.gameDir) this.gameDir = movedPath(this.gameDir, from, to);
      if (this.configPath) this.configPath = movedPath(this.configPath, from, to);
    } else if (this.configPath && !this.dirty) {
      this.closeConfig();
      this.gameDir = null;
    }
    await this.refreshLibrary();
  }

  /** List the library again and open `path`, a config that was just made. */
  async openCreated(path: string) {
    await this.refreshLibrary();
    await this.openConfig(path);
  }

  /** Open the config `made` resolves to once it is on disk; failures are shown. */
  createConfig(made: Promise<string>): Promise<boolean> {
    return this.attempt(made.then((path) => this.openCreated(path)));
  }

  /** Ask for a name, then make an empty config in the open game and open it. */
  async newConfig(suggested = '') {
    const dir = this.gameDir;
    if (!dir) return;
    const name = await this.ask(t('header.new_config_name'), suggested);
    if (name) await this.createConfig(api.configCreate(dir, name));
  }

  /** Delete a game after asking; its config closes when it is open. */
  async deleteGame(g: Game) {
    if (!(await this.confirmAsk(t('profiles.delete_confirm', { name: g.meta.name })))) return;
    await this.attempt(
      api.gameDelete(g.dir).then(async () => {
        if (this.gameDir === g.dir) {
          this.closeConfig();
          this.gameDir = null;
        }
        await this.refreshLibrary();
      }),
    );
  }

  /** List the library again and open `g`, a game that was just made. */
  async openCreatedGame(g: Game) {
    await this.refreshLibrary();
    await this.selectGame(g.dir);
  }

  /** Make a game called `name` and open it. */
  createGame(name: string): Promise<boolean> {
    return this.attempt(
      api.gameCreate(name).then((g) => this.openCreatedGame(g)),
      t('notices.game_created', { name }),
    );
  }

  /** Game bundles opened from Explorer: each is imported after asking. */
  async openFiles(files: string[]) {
    for (const file of files) {
      const name = file.split(/[\\/]/).pop() ?? file;
      if (await this.confirmAsk(t('profiles.open_confirm', { name }))) await this.importGame(file);
    }
  }

  /** Unpack a game bundle into a new game and open it, asking first about programs it starts. */
  importGame(file: string): Promise<boolean> {
    return this.attempt(
      api.gameLaunches(file).then(async (found) => {
        const keep = await this.keepLaunches(found);
        if (keep === null) return;
        const g = await api.gameImport(file, keep);
        await this.openCreatedGame(g);
        this.toast(t('profiles.imported', { name: g.meta.name }), 'ok');
      }),
    );
  }

  /** Quit reWASD so Doppelpad can read the controllers. */
  async quitRewasd() {
    if (this.rewasdQuitting) return;
    this.rewasdQuitting = true;
    try {
      const quit = await api.rewasdQuit();
      this.toast(t(quit ? 'rewasd.quit_done' : 'rewasd.declined'), quit ? 'ok' : 'info');
    } catch (e) {
      this.toast(t('rewasd.quit_failed', { error: errorText(e) }), 'error');
    } finally {
      this.rewasdQuitting = false;
      this.rewasd = await api.rewasdStatus().catch(() => this.rewasd);
    }
  }

  toast(text: string, kind: Toast['kind'] = 'info') {
    const id = ++this.toastId;
    this.toasts = [...this.toasts, { id, text, kind }];
    setTimeout(() => (this.toasts = this.toasts.filter((x) => x.id !== id)), kind === 'error' ? 7000 : 3500);
  }

  /** Ask for a text in the app's dialog (PromptDialog); `null` when cancelled or left empty. */
  ask(title: string, value = '', multiline = false): Promise<string | null> {
    return this.dialogs.ask(title, value, multiline);
  }

  /**
   * Ask an OK/Cancel question in the app's own dialog (ConfirmDialog); `true` for OK. With
   * `options.id` it offers "Don't ask again", and a silenced id answers OK at once.
   */
  confirmAsk(message: string, options: QuestionOptions = {}): Promise<boolean> {
    return this.dialogs.confirmAsk(message, options);
  }

  /** `confirmAsk` that tells which answer, for a question with a second one (`options.alt`). */
  choose(message: string, options: QuestionOptions = {}): Promise<Answer> {
    return this.dialogs.choose(message, options);
  }

  /** Tell something in the app's dialog; with an `id` it offers "Don't show again", and a silenced id shows nothing. */
  inform(message: string, id?: QuestionId): Promise<void> {
    return this.dialogs.inform(message, id);
  }

  /** Stop asking a question (its answer is OK from now on), or (`false`) ask it again. */
  silence(id: string, on = true) {
    return this.setSettings((s) => ({
      silenced: on ? [...s.silenced.filter((x) => x !== id), id] : s.silenced.filter((x) => x !== id),
    }));
  }

  /**
   * Whether an import keeps the launch commands it brings (`found`), which start programs:
   * asked in the app's dialog, where leaving them out is the default; `null` when the import
   * is cancelled. Without any there is no question, and `false`.
   */
  async keepLaunches(found: Launch[]): Promise<boolean | null> {
    if (found.length === 0) return false;
    const answer = await this.choose(t('launches.message'), {
      title: t('launches.title'),
      items: launchRows(found, this.family).map((r) => ({ code: r.command, notes: r.places })),
      ok: t('launches.drop'),
      alt: t('launches.keep'),
    });
    return answer === null ? null : answer === 'alt';
  }

  /** Ask before discarding unsaved changes. */
  async confirmDiscard(): Promise<boolean> {
    return !this.dirty || (await this.confirmAsk(t('header.discard_confirm')));
  }

  /** Open a game with the config it starts with; the open game's config stays as it is. */
  async selectGame(dir: string) {
    if (dir === this.gameDir && this.configPath) return;
    if (!(await this.confirmDiscard())) return;
    this.gameDir = dir;
    const g = this.library.games.find((x) => x.dir === dir);
    const target =
      g?.configs.find((c) => c.path === this.live?.profile_path) ??
      g?.configs.find((c) => g.meta.default_config && c.path.endsWith(g.meta.default_config)) ??
      g?.configs[0];
    if (target) await this.openConfig(target.path, true);
    else this.closeConfig();
  }

  closeConfig() {
    this.configPath = null;
    this.profile = null;
    this.savedJson = '';
    this.draftJson = '';
    this.clearHistory();
    this.selected = null;
    this.stick = null;
  }

  async openConfig(path: string, force = false) {
    if (!force && path !== this.configPath && !(await this.confirmDiscard())) return;
    try {
      const loaded = await api.configLoad(path);
      this.configPath = path;
      this.profile = loaded.profile;
      this.savedJson = this.draftJson = JSON.stringify(loaded.profile);
      this.readOnly = loaded.read_only;
      this.clearHistory();
      this.diagnostics = loaded.diagnostics;
      this.layer = 0;
      this.selected = null;
      this.stick = null;
      this.macroTarget = null;
      this.radialMenu = 0;
      this.radialSector = null;
      if (this.view === 'macro') this.view = 'front';
    } catch (e) {
      this.toast(errorText(e), 'error');
    }
  }

  /** Replace the draft profile with one computed from it (all edits go through here or `edit`). */
  update(p: Profile) {
    if (this.readOnly) return;
    if (!this.profile) {
      this.profile = p;
      this.draftJson = JSON.stringify(p);
      return;
    }
    this.commit(p);
  }

  /** Change the draft profile; nothing happens without one or when it is read-only. `change` may change it in place. */
  edit(change: (p: Profile) => Profile) {
    if (!this.profile || this.readOnly) return;
    this.commit(change(this.profile));
  }

  /** Make `next` the draft; the draft before becomes an undo step if anything changed. */
  private commit(next: Profile) {
    const before = this.draftJson;
    const json = JSON.stringify(next);
    if (json !== before) {
      this.history.record(before, this.editKey(), performance.now());
      this.syncHistory();
      this.checkSoon();
    }
    this.profile = next;
    this.draftJson = json;
  }

  private checkTimer: ReturnType<typeof setTimeout> | undefined;

  /** Validate the draft again shortly after the last change (the diagnostics bar and the marks follow). */
  private checkSoon() {
    clearTimeout(this.checkTimer);
    this.checkTimer = setTimeout(async () => {
      const path = this.configPath;
      if (!this.profile) return;
      try {
        const found = await api.configCheck($state.snapshot(this.profile) as Profile);
        if (this.configPath === path) this.diagnostics = found ?? [];
      } catch {
        // The last findings stay; saving validates again anyway.
      }
    }, 300);
  }

  /** What the change being made belongs to (see `EditKey`). */
  private editKey(): EditKey {
    if (this.group !== null) return { kind: 'gesture', id: this.group };
    if (pointer.down) return { kind: 'gesture', id: `pointer:${pointer.id}` };
    const el = typeof document === 'undefined' ? null : document.activeElement;
    return isStreamingField(el) ? { kind: 'field', id: el } : null;
  }

  /** From now until `endEditGroup`, every change is part of one undo step (e.g. a macro recording). */
  beginEditGroup() {
    this.group = Symbol('edit group');
  }

  endEditGroup() {
    this.group = null;
  }

  async renameConfig() {
    if (!this.profile || this.readOnly) return;
    const name = await this.ask(t('header.rename_config'), this.profile.name ?? '');
    if (name) this.edit((p) => ({ ...p, name }));
  }

  /** Save, then copy the open config into a new one next to it and open that. */
  async duplicateConfig() {
    if (!this.configPath) return;
    if (this.dirty && !(await this.save())) return;
    await this.createConfig(api.configDuplicate(this.configPath));
  }

  /** Open a context menu at a window point, or below `from` when the point is unknown (keyboard). */
  openContextMenu(items: MenuItem[], at: { x: number; y: number } | null, from: HTMLElement | null) {
    const r = from?.getBoundingClientRect();
    const x = at?.x ?? r?.left ?? 0;
    const y = at?.y ?? r?.bottom ?? 0;
    this.contextMenu = { items, x, y, from };
  }

  undo() {
    this.travel((current) => this.history.undo(current));
  }

  redo() {
    this.travel((current) => this.history.redo(current));
  }

  private travel(step: (current: string) => string | null) {
    if (!this.profile || this.readOnly) return;
    const target = step(this.draftJson);
    this.syncHistory();
    if (target === null) return;
    this.profile = JSON.parse(target) as Profile;
    this.draftJson = target;
    if (!layerIds(this.profile).includes(this.layer)) this.layer = 0;
    this.checkSoon();
  }

  private clearHistory() {
    this.history.clear();
    this.group = null;
    this.syncHistory();
  }

  private syncHistory() {
    this.canUndo = this.history.canUndo;
    this.canRedo = this.history.canRedo;
  }

  async save(): Promise<boolean> {
    if (!this.profile || !this.configPath || this.readOnly) return false;
    // What is saved is the draft now; changes made while the save is under way stay unsaved.
    const json = this.draftJson;
    const sent = $state.snapshot(this.profile) as Profile;
    try {
      const diags = await api.configSave(this.configPath, sent);
      this.diagnostics = diags;
      if (diags.some((d) => d.severity === 'error')) {
        this.toast(t('notices.save_errors'), 'error');
        return false;
      }
      this.savedJson = json;
      await this.refreshLibrary();
      this.toast(t('notices.saved'), 'ok');
      return true;
    } catch (e) {
      this.toast(errorText(e), 'error');
      return false;
    }
  }

  /** Save, put the config into the chosen slot and activate it. */
  async apply() {
    if (!this.configPath) {
      this.toast(t('remap.no_config'), 'error');
      return;
    }
    if (this.dirty && !(await this.save())) return;
    const slot = this.applySlot;
    await this.attempt(api.applyToSlot(slot, this.configPath, this.slotSet.device), t('remap.applied', { n: slot + 1 }));
  }

  /** This layer's mapping for a trigger + activator, else the one inherited from Main. */
  mappingAt(layer: number, combo: Combo, activator: Activator): { own?: Mapping; inherited?: Mapping } {
    return this.profile ? mappingAt(this.profile, layer, combo, activator) : {};
  }

  /** Change one mapping (see `editMappingIn`). */
  editMapping(layer: number, combo: Combo, activator: Activator, change: (m: Mapping) => void, keepEmpty = false) {
    this.edit((p) => editMappingIn(p, layer, combo, activator, change, keepEmpty));
  }

  select(c: Combo | null) {
    this.selected = c;
    this.stick = null;
    this.macroTarget = null;
    // Views without a binding panel (the macro editor, the radial menus) give way.
    if (c && !VIEWS[this.view].selection) this.view = 'front';
  }

  /**
   * Show a place in the config (a found mapping, a problem, an arrow of the layer map): a
   * mapping selected in the view that has its control, on its layer and activator; a layer in
   * a view that shows layers; a radial menu (and sector) in its view.
   */
  reveal(place: Place) {
    if (place.kind === 'radial') {
      this.view = 'radial';
      this.radialMenu = place.menu;
      this.radialSector = place.sector;
      return;
    }
    this.layer = place.layer;
    if (place.kind === 'layer') {
      if (!VIEWS[this.view].layer) this.view = 'front';
      return;
    }
    const view = viewFor(place.trigger, this.view);
    const single = comboList(place.trigger);
    // A trigger shows on the back of some controllers only.
    this.view =
      (view === 'front' || view === 'back') && single.length === 1 ? sideShowing(drawingsFor(this.primary), view, single[0]!) : view;
    this.activator = place.activator;
    this.select(place.trigger);
  }

  /** Show what a whole stick does in the side panel instead of a control's mappings. */
  selectStick(s: Side) {
    this.select(null);
    this.stick = s;
  }

  /** Make the whole stick a mouse, WASD, the arrow keys or a flick stick on the current layer (`null`: nothing). */
  async setStickGroup(s: Side, group: StickGroup | null) {
    const p = this.profile;
    const def = this.catalog?.defaults.analog;
    if (!p || !def || this.readOnly) return;
    if (group && stickGroupOf(p, this.layer, s) === group) return this.selectStick(s);
    const n = stickGroupConflicts(p, this.layer, s);
    if (n > 0 && !(await this.confirmAsk(t('binding.whole_stick_replace', { n }), { id: 'whole_stick_replace' }))) return;
    // The config may have changed while the question was open.
    if (this.profile !== p) return;
    this.update(setStickGroup(p, this.layer, s, group, def));
    this.selectStick(s);
  }

  /** Show the analog view on one of its pages (from the controller drawing or the binding panel). */
  openAnalog(tab: AnalogTab) {
    this.analogTab = tab;
    this.view = 'analog';
    this.select(null);
  }

  selectedKey = $derived(this.selected ? comboKey(this.selected) : '');
}

export const app = new AppState();
