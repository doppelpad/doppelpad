// Central application state (Svelte 5 runes). Components read and mutate `app`.

import { api, confirmAsk, errorText } from '../api';
import { setLanguage, t } from '../i18n/index.svelte';
import type { Side } from '../model/analog';
import { controlsFromPad, drawingsFor, sideShowing } from '../model/devices';
import { setStickGroup, stickGroupConflicts, stickGroupOf, type StickGroup } from '../model/stickgroups';
import { comboKey, editMappingIn, mappingAt, type LayerCopy } from '../model/profile';
import { layerLabel } from '../model/labels';
import { clone } from '../model/util';
import { SETTINGS_PLACEHOLDER } from './settings';
import type {
  Activator,
  Catalog,
  Combo,
  ControlName,
  DiagMsg,
  Game,
  GuiSettings,
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
  diagnostics = $state<DiagMsg[]>([]);

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
  copiedLayer = $state.raw<LayerCopy | null>(null);

  /** Who gets the controller's presses instead of the editor (see `capturePresses`). */
  private pressListener: ((c: ControlName) => void) | null = null;
  /** Controls held on the primary controller; the drawing highlights exactly these. */
  pressed = $state.raw(new Set<ControlName>());
  private toastId = 0;

  dirty = $derived(this.profile !== null && JSON.stringify(this.profile) !== this.savedJson);
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
      await this.refreshLibrary();
      await api.liveSubscribe((s) => this.onLive(s));
      for (const n of await api.takeNotices()) this.toast(t(`notices.${n}`), 'info');
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
      if (game) this.selectGame(game.dir);
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

  async setSettings(patch: Partial<GuiSettings>) {
    const next = { ...this.settings, ...patch };
    if (await this.attempt(api.settingsSet(next))) {
      this.settings = next;
      setLanguage(next.language);
    }
  }

  async refreshLibrary() {
    this.library = await api.libraryScan();
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

  /** Ask before discarding unsaved changes. */
  async confirmDiscard(): Promise<boolean> {
    return !this.dirty || (await confirmAsk(t('header.discard_confirm')));
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

  /** Replace the draft profile with one computed from it (all edits go through here). */
  update(p: Profile) {
    if (this.readOnly) return;
    this.profile = p;
  }

  /** Change the draft profile; nothing happens without one or when it is read-only. */
  edit(change: (p: Profile) => Profile) {
    if (this.profile && !this.readOnly) this.profile = change(this.profile);
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
    if (n > 0 && !(await confirmAsk(t('binding.whole_stick_replace', { n })))) return;
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
