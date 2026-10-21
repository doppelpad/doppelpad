// Central application state (Svelte 5 runes). Components read and mutate `app`.

import { api, errorText } from '../api';
import { setLanguage, t } from '../i18n/index.svelte';
import type { Side } from '../model/analog';
import { controlsFromPad, drawingsFor, sideShowing } from '../model/devices';
import { setStickGroup, stickGroupConflicts, stickGroupOf, type StickGroup } from '../model/stickgroups';
import {
  activatorOf,
  comboKey,
  effectiveMappings,
  findMapping,
  layerName,
  modeOf,
  newMapping,
  removeMapping,
  tidyMapping,
  upsertMapping,
  type LayerCopy,
} from '../model/profile';
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
  settings = $state<GuiSettings>({
    language: 'en',
    stay_live: false,
    hook_buttons: true,
    rewasd_at_start: 'ask',
    osd: {
      enabled: true,
      remap: true,
      slot: true,
      layer: true,
      keep_layer: false,
      controller: true,
      battery: true,
      monitor: 0,
      corner: 'bottom_right',
      scale: 100,
      opacity: 80,
      seconds: 5,
    },
    pad_overlay: { corner: 'top_left', size: 70, opacity: 90, hide_after: 5 },
    mappings_overlay: { corner: 'top_right', opacity: 90 },
    // Not shown before the real settings say so.
    welcome_done: true,
  });
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
  copiedLayer: LayerCopy | null = null;

  /** Listeners for physical controller presses (shortcut recording overrides selection). */
  pressListener: ((c: ControlName) => void) | null = null;
  /** Controls held on the primary controller; the drawing highlights exactly these. */
  pressed = $state.raw(new Set<ControlName>());
  private toastId = 0;

  dirty = $derived(this.profile !== null && JSON.stringify(this.profile) !== this.savedJson);
  game = $derived<Game | null>(this.library.games.find((g) => g.dir === this.gameDir) ?? null);
  primary = $derived<LiveDevice | null>(
    this.live?.devices.find((d) => d.id === this.live?.primary) ?? this.live?.devices[0] ?? null,
  );
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

  layerLabel = (id: number) =>
    this.profile ? layerName(this.profile, id, t('layers.main'), (n) => t('layers.shift', { n })) : String(id);

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
      // Open the game of the config that is currently applied, else the first game.
      const first = this.library.games[0];
      if (first) this.selectGame(first.dir);
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
    this.pressed = now;
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

  async setSettings(patch: Partial<GuiSettings>) {
    const next = { ...this.settings, ...patch };
    try {
      await api.settingsSet(next);
      this.settings = next;
      setLanguage(next.language);
    } catch (e) {
      this.toast(errorText(e), 'error');
    }
  }

  async refreshLibrary() {
    this.library = await api.libraryScan();
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
  confirmDiscard(): boolean {
    return !this.dirty || confirm(t('header.unsaved') + ' — ' + t('common.close') + '?');
  }

  async selectGame(dir: string) {
    if (dir !== this.gameDir && !this.confirmDiscard()) return;
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
    if (!force && path !== this.configPath && !this.confirmDiscard()) return;
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

  /** Replace the draft profile (all edits go through here). */
  update(p: Profile) {
    if (this.readOnly) return;
    this.profile = p;
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
    try {
      await api.applyToSlot(this.applySlot, this.configPath, this.slotSet.device);
      this.toast(t('remap.applied', { n: this.applySlot + 1 }), 'ok');
    } catch (e) {
      this.toast(errorText(e), 'error');
    }
  }

  /** This layer's mapping for a trigger + activator, else the one inherited from Main. */
  mappingAt(layer: number, combo: Combo, activator: Activator): { own?: Mapping; inherited?: Mapping } {
    if (!this.profile) return {};
    const own = findMapping(this.profile, layer, combo, activator);
    if (own) return { own };
    const eff = effectiveMappings(this.profile, layer, combo);
    return { inherited: eff.inherited ? eff.mappings.find((m) => activatorOf(m) === activator) : undefined };
  }

  /**
   * Change one mapping. Starts from this layer's mapping, else a copy of the inherited one,
   * else a new mapping. An edit that leaves nothing behind removes the mapping, unless
   * `keepEmpty` is set (an empty mapping blocks the control).
   */
  editMapping(layer: number, combo: Combo, activator: Activator, change: (m: Mapping) => void, keepEmpty = false) {
    if (!this.profile || this.readOnly) return;
    const { own, inherited } = this.mappingAt(layer, combo, activator);
    const from = own ?? inherited;
    const m: Mapping = from ? structuredClone($state.snapshot(from) as Mapping) : newMapping(layer, combo, activator);
    change(m);
    const tidy = tidyMapping({ ...m, layer, activator });
    const empty = (tidy.actions ?? []).length === 0 && !tidy.jump && !tidy.description && modeOf(tidy) === 'hold';
    this.update(
      empty && !keepEmpty ? removeMapping(this.profile, layer, combo, activator) : upsertMapping(this.profile, tidy),
    );
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
  setStickGroup(s: Side, group: StickGroup | null) {
    const p = this.profile;
    const def = this.catalog?.defaults.analog;
    if (!p || !def || this.readOnly) return;
    if (group && stickGroupOf(p, this.layer, s) === group) return this.selectStick(s);
    const n = stickGroupConflicts(p, this.layer, s);
    if (n > 0 && !confirm(t('binding.whole_stick_replace', { n }))) return;
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
