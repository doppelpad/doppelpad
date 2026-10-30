<script lang="ts">
  // On-screen ANSI keyboard. Keys are controls `Key:<name>`; clicking or pressing a
  // physical key selects it for the binding panel.
  import { SvelteSet } from 'svelte/reactivity';
  import { t } from '../../lib/i18n/index.svelte';
  import { keyFromEvent } from '../../lib/model/keymap';
  import { shownSummary } from '../../lib/model/labels';
  import { keyboardOwned } from '../../lib/dom';
  import { app } from '../../lib/state/app.svelte';
  import { controlMenu } from '../binding/controlMenu';
  import { isFlagged } from '../../lib/model/lints';
  import { tip } from '../../lib/tooltip.svelte';

  interface KeyBox {
    k: string;
    x: number;
    y: number;
    w: number;
    h: number;
  }

  /** Lay out one row: entries are [name, width] or a number for a gap. */
  function row(y: number, x: number, items: ([string, number?] | number)[]): KeyBox[] {
    const out: KeyBox[] = [];
    for (const it of items) {
      if (typeof it === 'number') {
        x += it;
        continue;
      }
      const w = it[1] ?? 1;
      out.push({ k: it[0], x, y, w, h: 1 });
      x += w;
    }
    return out;
  }

  const letters = (s: string) => s.split('').map((c) => [c] as [string]);
  const digits = '1234567890'.split('').map((d) => [`Digit${d}`] as [string]);
  const fkeys = (a: number, b: number) => Array.from({ length: b - a + 1 }, (_, i) => [`F${a + i}`] as [string]);

  const NAV = 15.5;
  const PAD = 19.5;
  const KEYS: KeyBox[] = [
    ...row(0, 0, [['Escape'], 1, ...fkeys(1, 4), 0.5, ...fkeys(5, 8), 0.5, ...fkeys(9, 12)]),
    ...row(0, NAV, [['PrintScreen'], ['ScrollLock'], ['Pause']]),
    ...row(1.5, 0, [['Grave'], ...digits, ['Minus'], ['Equal'], ['Backspace', 2]]),
    ...row(1.5, NAV, [['Insert'], ['Home'], ['PageUp']]),
    ...row(1.5, PAD, [['NumLock'], ['NumpadDivide'], ['NumpadMultiply'], ['NumpadSubtract']]),
    ...row(2.5, 0, [['Tab', 1.5], ...letters('QWERTYUIOP'), ['LeftBracket'], ['RightBracket'], ['Backslash', 1.5]]),
    ...row(2.5, NAV, [['Delete'], ['End'], ['PageDown']]),
    ...row(2.5, PAD, [['Numpad7'], ['Numpad8'], ['Numpad9']]),
    { k: 'NumpadAdd', x: PAD + 3, y: 2.5, w: 1, h: 2 },
    ...row(3.5, 0, [['CapsLock', 1.75], ...letters('ASDFGHJKL'), ['Semicolon'], ['Apostrophe'], ['Enter', 2.25]]),
    ...row(3.5, PAD, [['Numpad4'], ['Numpad5'], ['Numpad6']]),
    ...row(4.5, 0, [['LeftShift', 2.25], ...letters('ZXCVBNM'), ['Comma'], ['Period'], ['Slash'], ['RightShift', 2.75]]),
    ...row(4.5, NAV + 1, [['Up']]),
    ...row(4.5, PAD, [['Numpad1'], ['Numpad2'], ['Numpad3']]),
    { k: 'NumpadEnter', x: PAD + 3, y: 4.5, w: 1, h: 2 },
    ...row(5.5, 0, [
      ['LeftCtrl', 1.25],
      ['LeftMeta', 1.25],
      ['LeftAlt', 1.25],
      ['Space', 6.25],
      ['RightAlt', 1.25],
      ['RightMeta', 1.25],
      ['ContextMenu', 1.25],
      ['RightCtrl', 1.25],
    ]),
    ...row(5.5, NAV, [['Left'], ['Down'], ['Right']]),
    ...row(5.5, PAD, [['Numpad0', 2], ['NumpadDecimal']]),
  ];
  const WIDTH = PAD + 4;
  const HEIGHT = 6.5;

  const LABELS: Record<string, string> = {
    Grave: '`',
    Minus: '-',
    Equal: '=',
    LeftBracket: '[',
    RightBracket: ']',
    Backslash: '\\',
    Semicolon: ';',
    Apostrophe: "'",
    Comma: ',',
    Period: '.',
    Slash: '/',
    Up: '↑',
    Down: '↓',
    Left: '←',
    Right: '→',
    NumpadDivide: '/',
    NumpadMultiply: '*',
    NumpadSubtract: '-',
    NumpadAdd: '+',
    NumpadDecimal: '.',
    Space: '',
  };

  /** Keys whose caps carry a word, by catalog key (`keys.*`): German keyboards say Strg, Entf, … */
  const WORDS: Record<string, string> = {
    Escape: 'esc',
    PrintScreen: 'print',
    ScrollLock: 'scroll',
    Pause: 'pause',
    Backspace: 'backspace',
    Insert: 'insert',
    Home: 'home',
    PageUp: 'page_up',
    NumLock: 'num',
    Tab: 'tab',
    Delete: 'delete',
    End: 'end',
    PageDown: 'page_down',
    CapsLock: 'caps',
    Enter: 'enter',
    NumpadEnter: 'enter',
    LeftShift: 'shift',
    RightShift: 'shift',
    LeftCtrl: 'ctrl',
    RightCtrl: 'ctrl',
    LeftMeta: 'win',
    RightMeta: 'win',
    LeftAlt: 'alt',
    RightAlt: 'alt_gr',
    ContextMenu: 'menu',
  };

  function label(k: string): string {
    if (k in WORDS) return t(`keys.${WORDS[k]}`);
    if (k in LABELS) return LABELS[k];
    if (k.startsWith('Digit')) return k.slice(5);
    if (k.startsWith('Numpad')) return k.slice(6);
    return k;
  }

  const summary = (k: string) => app.profile && shownSummary(app.profile, app.layer, `Key:${k}`, app.layerLabel, app.settings.show_notes);

  const held = new SvelteSet<string>();

  function onKeydown(e: KeyboardEvent) {
    if (e.ctrlKey || keyboardOwned(e.target)) return;
    const k = keyFromEvent(e);
    if (!k) return;
    e.preventDefault();
    held.add(k);
    if (!e.repeat) app.select(`Key:${k}`);
  }

  function onKeyup(e: KeyboardEvent) {
    const k = keyFromEvent(e);
    if (k) held.delete(k);
  }
</script>

<svelte:window onkeydown={onKeydown} onkeyup={onKeyup} onblur={() => held.clear()} />

<div class="wrap">
  <div class="board" style="--w: {WIDTH}; --h: {HEIGHT}">
    {#each KEYS as key (key.k)}
      {@const s = summary(key.k)}
      <button
        class="key"
        class:mapped={!!s}
        class:inh={s?.inherited}
        class:note-mark={s?.hiddenNote}
        class:flag-mark={isFlagged(app.flags, app.profile, app.layer, `Key:${key.k}`)}
        class:selected={app.selectedKey === `Key:${key.k}`}
        class:held={held.has(key.k)}
        style="--x: {key.x}; --y: {key.y}; --kw: {key.w}; --kh: {key.h}"
        use:tip={s ? `${key.k}: ${s.text}` : key.k}
        use:controlMenu={`Key:${key.k}`}
        onclick={() => app.select(`Key:${key.k}`)}
      >
        <span class="cap">{label(key.k)}</span>
        {#if s}<span class="out">{s.text}</span>{/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .wrap {
    padding: 28px 24px;
    container-type: inline-size;
  }
  .board {
    /* One key unit, from the board's width: the view's width up to the widest board. */
    --u: calc(min(100cqw, 1100px) / var(--w));
    position: relative;
    width: calc(var(--u) * var(--w));
    height: calc(var(--u) * var(--h));
    margin: 0 auto;
  }
  .key {
    position: absolute;
    left: calc(var(--u) * var(--x) + 2px);
    top: calc(var(--u) * var(--y) + 2px);
    width: calc(var(--u) * var(--kw) - 4px);
    height: calc(var(--u) * var(--kh) - 4px);
    border-radius: 6px;
    background: var(--bg-2);
    border: 1px solid var(--line);
    box-shadow: inset 0 -2px 0 rgba(0, 0, 0, 0.35);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: space-between;
    padding: 3px 5px;
    overflow: hidden;
    text-align: left;
  }
  .key:hover {
    border-color: var(--line-strong);
    background: var(--bg-3);
  }
  .cap {
    font-size: clamp(9px, calc(var(--u) * 0.24), 12px);
    color: var(--text-dim);
    white-space: nowrap;
  }
  .out {
    width: 100%;
    font-size: clamp(8px, calc(var(--u) * 0.2), 11px);
    color: var(--accent);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .key.mapped {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--line));
  }
  .key.mapped .cap {
    color: var(--text);
  }
  .key.inh .out {
    color: var(--text-faint);
  }
  .key.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .key.held {
    box-shadow: 0 0 0 2px var(--accent);
  }
</style>
