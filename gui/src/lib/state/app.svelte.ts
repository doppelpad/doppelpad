// Central application state (Svelte 5 runes). Components read and mutate `app`.

import { listen } from '@tauri-apps/api/event';
import { api, errorText } from '../api';
import { setLanguage, t } from '../i18n/index.svelte';
import type { Side } from '../model/analog';
import { controlsFromPad, drawingsFor, sideShowing } from '../model/devices';
import { setStickGroup, stickGroupConflicts, stickGroupOf, type StickGroup } from '../model/stickgroups';
import { comboKey, editMappingIn, layerIds, mappingAt, type LayerCopy } from '../model/profile';
import { layerLabel } from '../model/labels';
import { clone } from '../model/util';
import { isStreamingField, pointer } from '../dom';
import { movedPath } from '../paths';
import { History, type EditKey } from './history';
import { flagsOf, hotkeyClashes, type Flags } from '../model/lints';
import type { Copied } from '../model/clipboard';
import type { MenuItem } from '../../components/ui/Menu.svelte';
import { SETTINGS_PLACEHOLDER } from './settings';
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
  Library,
  LiveDevice,
  LiveState,
  Mapping,
  PadFamily,
  Profile,
  RewasdStatus,
} from '../types';

export type View = 'front' | 'back' | 'keyboard' | 'mouse' | 'shortcuts' | 'macro' | 'output' | 'analog';
/** Pages of the analog view. */
export type AnalogTab = 'left' | 'right' | 'triggers' | 'gyro' | 'touchpad' | 'routes';

export interface Toast {
  id: number;
  text: string;
  kind: 'info' | 'ok' | 'error';
}

interface PromptRequest {
  title: string;
  value: string;
  multiline: boolean;
  resolve: (v: string | null) => void;
}

interface QuestionRequest {
  message: string;
  /** A question that can be silenced ("Don't ask again"). */
  id?: QuestionId;
  resolve: (ok: boolean) => void;
}

/**
 * Questions that may be silenced: only ones whose change can be undone. Deleting games or
 * configs and discarding unsaved changes always ask.
 */
export const QUESTION_IDS = ['power_command', 'clear_mappings', 'whole_stick_replace'] as const;
export type QuestionId = (typeof QUESTION_IDS)[number];

export interface ContextMenuRequest {
  items: MenuItem[];
  /** Window coordinates of its top left corner (it moves to stay inside the window). */
  x: number;
  y: number;
  /** Gets the focus back when the menu closes without a choice. */
  from: HTMLElement | null;
}

class AppState {
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
  applySlot = $state(0);
  /** The controller whose own slots the bottom bar shows; `null` for the shared slots. */
  slotDevice = $state<string | null>(null);
  toasts = $state<Toast[]>([]);
  prompt = $state<PromptRequest | null>(null);
  question = $state<QuestionRequest | null>(null);
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
  /** Who gets the controller's presses instead of the editor (see `capturePresses`). */
  private pressListener: ((c: ControlName) => void) | null = null;
  /** Controls held on the primary controller; the drawing highlights exactly these. */
  pressed = $state.raw(new Set<ControlName>());
  private toastId = 0;
  /** The settings write in flight: the next one waits for it, so it builds on its result. */
  private settingsWrite: Promise<void> = Promise.resolve();

  dirty = $derived(this.profile !== null && JSON.stringify(this.profile) !== this.savedJson);
  /** Everything the diagnostics bar lists: the validator's findings and the hotkey clashes. */
  problems = $derived<DiagMsg[]>(this.profile ? [...this.diagnostics, ...hotkeyClashes(this.profile, this.hotkeys)] : []);
  /** The controls and layers with a warning mark. */
  flags = $derived<Flags>(flagsOf(this.profile, this.problems));
  game = $derived<Game | null>(this.library.games.find((g) => g.dir === this.gameDir) ?? null);
  primary = $derived<LiveDevice | null>(this.live?.devices.find((d) => d.id === this.live?.primary) ?? this.live?.devices[0] ?? null);
  family = $derived<PadFamily | null>(this.primary?.family ?? null);
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

  layerLabel = (id: number) => (this.profile ? layerLabel(this.profile, id) : String(id));

  async init() {
    try {
      this.settings = await api.settingsGet();
      setLanguage(this.settings.language);
      this.catalog = await api.catalog();
      this.hotkeys = await api.hotkeysGet();
      await this.refreshLibrary();
      await api.liveSubscribe((s) => this.onLive(s));
      for (const n of await api.takeNotices()) this.toast(t(`notices.${n}`), 'info');
      // Bundles opened from Explorer: the one the app started with now, later ones as they come.
      await listen<string[]>('open-files', (e) => this.openFiles(e.payload));
      const opened = await api.takeOpenFiles();
      this.rewasd = await api.rewasdStatus();
      if (this.rewasdRunning) {
        if (this.settings.rewasd_at_start === 'quit') this.quitRewasd();
        else if (this.settings.rewasd_at_start === 'ask') {
          this.rewasdAsk = true;
          // The app may have started hidden with Windows.
          await api.showWindow();
        }
      }
      // Open the game of the config that is currently applied (when the first live state is
      // already here), else the first game.
      const applied = this.live?.profile_path;
      const game = this.library.games.find((g) => g.configs.some((c) => c.path === applied)) ?? this.library.games[0];
      if (game) await this.selectGame(game.dir);
      if (opened.length) await this.openFiles(opened);
    } catch (e) {
      this.toast(errorText(e), 'error');
    }
  }

  private onLive(s: LiveState) {
    const firstSnapshot = this.live === null;
    this.live = s;
    if (firstSnapshot) this.applySlot = s.active_slot;
    const dev = this.primary;
    const now = dev && this.catalog ? controlsFromPad(dev, this.catalog.pad_buttons) : new Set<ControlName>();
    for (const c of now) {
      if (!this.pressed.has(c)) this.onPhysicalPress(c);
    }
    // Live state comes many times a second; only a change of the held controls is news.
    if (now.size !== this.pressed.size || [...now].some((c) => !this.pressed.has(c))) this.pressed = now;
  }

  private onPhysicalPress(c: ControlName) {
    if (this.pressListener) {
      this.pressListener(c);
      return;
    }
    if (!this.settings.hook_buttons || !this.profile || !document.hasFocus()) return;
    if (this.view !== 'front' && this.view !== 'back') return;
    this.select(c);
    this.view = sideShowing(drawingsFor(this.primary), this.view, c);
  }

  /**
   * Send the controller's presses to `fn` instead of selecting controls (without `fn` they just
   * stay away from the editor) until the returned function is called. That call does nothing
   * once another caller has taken the presses over.
   */
  capturePresses(fn: (c: ControlName) => void = () => {}): () => void {
    this.pressListener = fn;
    return () => {
      if (this.pressListener === fn) this.pressListener = null;
    };
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
    const write = this.settingsWrite.then(async () => {
      const next = { ...this.settings, ...(typeof patch === 'function' ? patch(this.settings) : patch) };
      if (await this.attempt(api.settingsSet(next))) {
        this.settings = next;
        setLanguage(next.language);
      }
    });
    // A write that throws must not stop the ones after it.
    this.settingsWrite = write.catch(() => {});
    return write;
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

  /** Unpack a game bundle into a new game and open it. */
  importGame(file: string): Promise<boolean> {
    return this.attempt(
      api.gameImport(file).then(async (g) => {
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

  ask(title: string, value = '', multiline = false): Promise<string | null> {
    return new Promise((resolve) => (this.prompt = { title, value, multiline, resolve }));
  }

  /**
   * Ask an OK/Cancel question in the app's own dialog (ConfirmDialog); `true` for OK. Not
   * `window.confirm`: in the webview it returns a promise without asking.
   */
  /** Ask OK/Cancel in the app's own dialog. With an `id` it offers "Don't ask again", and a silenced id answers OK at once. */
  confirmAsk(message: string, id?: QuestionId): Promise<boolean> {
    if (id && this.settings.silenced.includes(id)) return Promise.resolve(true);
    this.question?.resolve(false);
    return new Promise((resolve) => (this.question = { message, id, resolve }));
  }

  /** Stop asking a question (its answer is OK from now on), or (`false`) ask it again. */
  silence(id: string, on = true) {
    return this.setSettings((s) => ({
      silenced: on ? [...s.silenced.filter((x) => x !== id), id] : s.silenced.filter((x) => x !== id),
    }));
  }

  /** Ask before discarding unsaved changes. */
  async confirmDiscard(): Promise<boolean> {
    return !this.dirty || (await this.confirmAsk(t('header.discard_confirm')));
  }

  async selectGame(dir: string) {
    if (dir !== this.gameDir && !(await this.confirmDiscard())) return;
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
      this.savedJson = JSON.stringify(loaded.profile);
      this.readOnly = loaded.read_only;
      this.clearHistory();
      this.diagnostics = loaded.diagnostics;
      this.layer = 0;
      this.selected = null;
      this.stick = null;
      this.macroTarget = null;
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
      return;
    }
    this.commit(JSON.stringify(this.profile), p);
  }

  /** Change the draft profile; nothing happens without one or when it is read-only. */
  edit(change: (p: Profile) => Profile) {
    if (!this.profile || this.readOnly) return;
    // Taken first: `change` may change the draft in place.
    const before = JSON.stringify(this.profile);
    this.commit(before, change(this.profile));
  }

  /** Make `next` the draft; `before` (the draft before, as JSON) becomes an undo step if anything changed. */
  private commit(before: string, next: Profile) {
    if (JSON.stringify(next) !== before) {
      this.history.record(before, this.editKey(), performance.now());
      this.syncHistory();
      this.checkSoon();
    }
    this.profile = next;
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
    const target = step(JSON.stringify(this.profile));
    this.syncHistory();
    if (target === null) return;
    this.profile = JSON.parse(target) as Profile;
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
    try {
      const diags = await api.configSave(this.configPath, $state.snapshot(this.profile) as Profile);
      this.diagnostics = diags;
      if (diags.some((d) => d.severity === 'error')) {
        this.toast(t('notices.save_errors'), 'error');
        return false;
      }
      this.savedJson = JSON.stringify(this.profile);
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
    if (c && this.view === 'macro') this.view = 'front';
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
    if (n > 0 && !(await this.confirmAsk(t('binding.whole_stick_replace', { n }), 'whole_stick_replace'))) return;
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
