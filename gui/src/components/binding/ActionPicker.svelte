<script lang="ts">
  // Choose one output action: keyboard key (list or press-to-capture), mouse, controller
  // or a built-in command.
  import { Keyboard, Mouse, Gamepad2, Terminal, X } from '@lucide/svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { confirmAsk } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { keyFromEvent } from '../../lib/model/keymap';
  import { controlLabel } from '../../lib/model/controls';
  import { POWER_COMMANDS, RADIAL_COMMANDS, SIMPLE_COMMANDS } from '../../lib/model/profile';
  import { app } from '../../lib/state/app.svelte';
  import type { Action, Dir, MouseButtonName, PadButtonName, ShakeAxis, StickName, TouchOutput, TriggerName } from '../../lib/types';

  let { onpick, onclose }: { onpick: (a: Action) => void; onclose: () => void } = $props();

  type Tab = 'keyboard' | 'mouse' | 'gamepad' | 'commands';
  let tab = $state<Tab>('keyboard');
  let search = $state('');
  let capturing = $state(false);

  const TABS: { id: Tab; icon: typeof Keyboard }[] = [
    { id: 'keyboard', icon: Keyboard },
    { id: 'mouse', icon: Mouse },
    { id: 'gamepad', icon: Gamepad2 },
    { id: 'commands', icon: Terminal },
  ];

  const GROUPS: { id: string; test: (k: string) => boolean }[] = [
    { id: 'letters', test: (k) => /^[A-Z]$/.test(k) },
    { id: 'digits', test: (k) => /^Digit\d$/.test(k) },
    { id: 'modifiers', test: (k) => /(Ctrl|Shift|Alt|Win|Meta)$/.test(k) },
    { id: 'function', test: (k) => /^F\d+$/.test(k) },
    { id: 'navigation', test: (k) => /^(Up|Down|Left|Right|Home|End|PageUp|PageDown|Insert|Delete)$/.test(k) },
    { id: 'numpad', test: (k) => k.startsWith('Numpad') },
    { id: 'media', test: (k) => /^(Media|Volume|Browser|Launch)/.test(k) },
  ];

  function keyLabel(k: string): string {
    if (/^Digit\d$/.test(k)) return k.slice(5);
    if (k.startsWith('Numpad')) return 'Num ' + k.slice(6);
    return k;
  }

  const keyGroups = $derived.by(() => {
    const keys = (app.catalog?.keys ?? []).filter((k) => !search || k.toLowerCase().includes(search.toLowerCase()));
    const groups = new Map<string, string[]>();
    for (const k of keys) {
      const g = GROUPS.find((g) => g.test(k))?.id ?? 'other';
      groups.set(g, [...(groups.get(g) ?? []), k]);
    }
    return [...GROUPS.map((g) => g.id), 'other'].filter((g) => groups.has(g)).map((g) => ({ id: g, keys: groups.get(g)! }));
  });

  const MOUSE_BUTTONS: MouseButtonName[] = ['left', 'right', 'middle', 'x1', 'x2'];
  const DIRS: Dir[] = ['up', 'down', 'left', 'right'];
  const SHAKES: ShakeAxis[] = ['+x', '-x', '+y', '-y', '+z', '-z'];
  /** Turn actions offered: a quarter left or right, or around. */
  const TURNS = [-90, 90, 180];
  /** Gestures a touch action can play on the virtual touchpad. */
  const TOUCH_OUTPUTS: TouchOutput[] = ['swipe_up', 'swipe_down', 'swipe_left', 'swipe_right', 'swipe2_up', 'swipe2_down', 'swipe2_left', 'swipe2_right', 'zoom_in', 'zoom_out', 'tap', 'tap_left', 'tap_right', 'multi_tap'];
  const PAD_BUTTONS = $derived((app.catalog?.pad_buttons ?? []) as PadButtonName[]);

  function stickAction(stick: StickName, d: Dir): Action {
    const v = { up: [0, 1], down: [0, -1], left: [-1, 0], right: [1, 0] }[d];
    return { stick, x: v[0], y: v[1] };
  }

  function onCaptureKey(e: KeyboardEvent) {
    if (!capturing) return;
    e.preventDefault();
    e.stopPropagation();
    const key = keyFromEvent(e);
    if (key) {
      capturing = false;
      onpick({ key });
    }
  }

  async function pickLaunch() {
    const path = await open({ title: t('picker.launch_path'), multiple: false, directory: false });
    if (typeof path !== 'string') return;
    const args = (await app.ask(t('picker.launch_args'), '')) ?? '';
    onpick({ command: { launch: args ? { path, args } : { path } } });
  }

  /** Power commands end the session or the PC's uptime: confirm before adding one. */
  async function pickPower(c: (typeof POWER_COMMANDS)[number]) {
    if (await confirmAsk(t('picker.power_confirm', { command: t(`picker.${c}`) }))) onpick({ command: c });
  }

  async function pickPause() {
    const v = await app.ask(t('picker.pause_ms'), '100');
    const n = v ? parseInt(v, 10) : NaN;
    if (n > 0) onpick({ pause: n });
  }

  async function pickRumble() {
    const v = await app.ask(t('picker.rumble_ms'), '200');
    const n = v ? parseInt(v, 10) : NaN;
    if (n > 0) onpick({ rumble: n });
  }
</script>

<svelte:window onkeydowncapture={onCaptureKey} />

<div class="picker">
  <div class="tabs">
    {#each TABS as tb (tb.id)}
      <button class="tab" class:active={tab === tb.id} title={t(`picker.${tb.id}`)} onclick={() => (tab = tb.id)}>
        <tb.icon size={16} />
        <span>{t(`picker.${tb.id}`)}</span>
      </button>
    {/each}
    <button class="icon-btn close" title={t('common.close')} onclick={onclose}><X size={15} /></button>
  </div>

  {#if tab === 'keyboard'}
    <div class="row">
      <button class="btn capture" class:on={capturing} onclick={() => (capturing = !capturing)}>
        <Keyboard size={14} />
        {capturing ? t('picker.press_now') : t('picker.press_key')}
      </button>
      <input class="search" placeholder={t('picker.search')} bind:value={search} />
    </div>
    <div class="scroll">
      {#each keyGroups as g (g.id)}
        <div class="group">{t(`picker.groups.${g.id}`)}</div>
        <div class="grid">
          {#each g.keys as k (k)}
            <button class="chip" onclick={() => onpick({ key: k })}>{keyLabel(k)}</button>
          {/each}
        </div>
      {/each}
    </div>
  {:else if tab === 'mouse'}
    <div class="scroll">
      <div class="group">{t('picker.mouse_buttons')}</div>
      <div class="grid">
        {#each MOUSE_BUTTONS as b (b)}
          <button class="chip" onclick={() => onpick({ mouse: b })}>{t(`controls.mouse.${b}`)}</button>
        {/each}
      </div>
      <div class="group">{t('picker.mouse_wheel')}</div>
      <div class="grid">
        {#each DIRS as d (d)}
          <button class="chip" onclick={() => onpick({ wheel: d })}>{t(`dirs.${d}`)}</button>
        {/each}
      </div>
      <div class="group">{t('picker.mouse_move')}</div>
      <div class="grid">
        {#each DIRS as d (d)}
          <button class="chip" onclick={() => onpick({ mouse_move: d })}>{t(`dirs.${d}`)}</button>
        {/each}
      </div>
      <div class="group">{t('picker.cursor')}</div>
      <div class="grid">
        <button class="chip" title={t('picker.cursor_hint')} onclick={() => onpick({ mouse_to: [0.5, 0.5] })}>
          {t('picker.cursor_centre')}
        </button>
        <button class="chip" title={t('picker.cursor_hint')} onclick={() => onpick({ mouse_clip: [0.25, 0.25, 0.75, 0.75] })}>
          {t('picker.cursor_clip')}
        </button>
      </div>
      <div class="group">{t('picker.turn')}</div>
      <div class="grid">
        {#each TURNS as deg (deg)}
          <button class="chip" title={t('picker.turn_hint')} onclick={() => onpick({ turn: deg })}>{t(`picker.turn_${deg < 0 ? 'left' : deg === 180 ? 'around' : 'right'}`)}</button>
        {/each}
      </div>
    </div>
  {:else if tab === 'gamepad'}
    <div class="scroll">
      <div class="group">{t('picker.pad_buttons')}</div>
      <div class="grid">
        {#each PAD_BUTTONS as b (b)}
          <button class="chip" onclick={() => onpick({ pad: b })}>{controlLabel(b, 'xbox')}</button>
        {/each}
      </div>
      <div class="group">{t('picker.pad_sticks')}</div>
      {#each ['left', 'right'] as const as s (s)}
        <div class="grid">
          {#each DIRS as d (d)}
            <button class="chip" onclick={() => onpick(stickAction(s, d))}>
              {t(s === 'left' ? 'controls.LeftStick' : 'controls.RightStick')} {t(`dirs.${d}`)}
            </button>
          {/each}
        </div>
      {/each}
      <div class="grid">
        <button class="chip" title={t('picker.anti_recoil_hint')} onclick={() => onpick({ stick: 'right', x: 0, y: -0.15 })}>
          {t('picker.anti_recoil')}
        </button>
      </div>
      <div class="group">{t('picker.pad_triggers')}</div>
      <div class="grid">
        {#each ['left', 'right'] as const as tr (tr)}
          <button class="chip" onclick={() => onpick({ trigger: tr as TriggerName })}>
            {t(tr === 'left' ? 'controls.LT' : 'controls.RT')}
          </button>
        {/each}
      </div>
      <div class="group">{t('picker.pad_touch')}</div>
      <div class="grid">
        {#each TOUCH_OUTPUTS as g (g)}
          <button class="chip" title={t('picker.pad_touch_hint')} onclick={() => onpick({ touch: g })}>
            {t(`picker.touch_out.${g}`)}
          </button>
        {/each}
      </div>
      <div class="group">{t('picker.pad_shake')}</div>
      <div class="grid">
        {#each SHAKES as s (s)}
          <button class="chip" title={t('picker.pad_shake_hint')} onclick={() => onpick({ shake: s })}>
            {t('picker.shake_axis', { axis: s.toUpperCase() })}
          </button>
        {/each}
      </div>
    </div>
  {:else}
    <div class="list">
      {#each SIMPLE_COMMANDS as c (c)}
        <button class="cmd" onclick={() => onpick({ command: c })}>{t(`picker.${c}`)}</button>
      {/each}
      {#each app.profile?.crosshairs ?? [] as x, i (i)}
        <button class="cmd" onclick={() => onpick({ command: { crosshair: i + 1 } })}>
          {t('picker.crosshair', { n: i + 1 })}{x.name ? ` · ${x.name}` : ''}
        </button>
      {/each}
      {#if app.profile?.crosshairs?.length}
        <button class="cmd" onclick={() => onpick({ command: 'toggle_crosshair' })}>{t('picker.toggle_crosshair')}</button>
      {/if}
      {#if app.profile?.radial_menus?.length}
        {#each RADIAL_COMMANDS as c (c)}
          <button class="cmd" onclick={() => onpick({ command: c })}>{t(`picker.${c}`)}</button>
        {/each}
      {/if}
      {#each POWER_COMMANDS as c (c)}
        <button class="cmd power" onclick={() => pickPower(c)}>{t(`picker.${c}`)}…</button>
      {/each}
      <button class="cmd" onclick={pickLaunch}>{t('picker.launch')}…</button>
      <button class="cmd" onclick={pickPause}>{t('picker.pause')}…</button>
      <button class="cmd" onclick={pickRumble}>{t('picker.rumble')}…</button>
      <button class="cmd" onclick={() => onpick({ break: true })}>{t('picker.break')}</button>
    </div>
  {/if}
</div>

<style>
  .picker {
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-m);
    background: var(--bg-0);
    display: flex;
    flex-direction: column;
    max-height: 360px;
    overflow: hidden;
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    border-bottom: 1px solid var(--line);
  }
  .tab {
    flex: 1;
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 5px 2px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
    font-size: 10.5px;
  }
  .tab:hover {
    background: var(--bg-2);
  }
  .tab.active {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .close {
    flex: none;
    width: 26px;
    height: 26px;
  }
  .row {
    display: flex;
    gap: 6px;
    padding: 8px 8px 4px;
  }
  .capture {
    white-space: nowrap;
    font-size: 12px;
  }
  .capture.on {
    border-color: var(--accent);
    color: var(--accent);
    animation: pulse 1.2s infinite;
  }
  @keyframes pulse {
    50% {
      box-shadow: 0 0 0 3px var(--accent-soft);
    }
  }
  .search {
    flex: 1;
    min-width: 0;
  }
  .scroll {
    overflow-y: auto;
    padding: 4px 8px 10px;
  }
  .group {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
    margin: 8px 0 4px;
  }
  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 4px;
  }
  .chip {
    padding: 4px 8px;
    border-radius: var(--radius-s);
    background: var(--bg-2);
    border: 1px solid var(--line);
    font-size: 12px;
  }
  .chip:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .list {
    display: flex;
    flex-direction: column;
    padding: 6px;
  }
  .cmd {
    text-align: left;
    padding: 8px 10px;
    border-radius: var(--radius-s);
  }
  .cmd:hover {
    background: var(--bg-2);
  }
</style>
