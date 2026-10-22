<script lang="ts">
  // On-screen ANSI keyboard. Keys are controls `Key:<name>`; clicking or pressing a
  // physical key selects it for the binding panel.
  import { keyFromEvent } from '../../lib/model/keymap';
  import { effectiveSummary } from '../../lib/model/labels';
  import { modalOpen } from '../ui/Dialog.svelte';
  import { app } from '../../lib/state/app.svelte';

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
    Escape: 'Esc', Grave: '`', Minus: '-', Equal: '=', LeftBracket: '[', RightBracket: ']', Backslash: '\\',
    Semicolon: ';', Apostrophe: "'", Comma: ',', Period: '.', Slash: '/', LeftShift: 'Shift', RightShift: 'Shift',
    LeftCtrl: 'Ctrl', RightCtrl: 'Ctrl', LeftAlt: 'Alt', RightAlt: 'Alt', LeftMeta: 'Win', RightMeta: 'Win',
    ContextMenu: 'Menu', PrintScreen: 'PrtSc', ScrollLock: 'ScrLk', Insert: 'Ins', Delete: 'Del', PageUp: 'PgUp',
    PageDown: 'PgDn', Up: '↑', Down: '↓', Left: '←', Right: '→', NumLock: 'Num', NumpadDivide: '/',
    NumpadMultiply: '*', NumpadSubtract: '-', NumpadAdd: '+', NumpadEnter: 'Enter', NumpadDecimal: '.',
    CapsLock: 'Caps', Space: '', Backspace: 'Backspace',
  };

  function label(k: string): string {
    if (k in LABELS) return LABELS[k];
    if (k.startsWith('Digit')) return k.slice(5);
    if (k.startsWith('Numpad')) return k.slice(6);
    return k;
  }

  const summary = (k: string) => app.profile && effectiveSummary(app.profile, app.layer, `Key:${k}`, app.layerLabel);

  let held = $state(new Set<string>());

  function typing(e: KeyboardEvent): boolean {
    const el = e.target as HTMLElement | null;
    return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable);
  }

  /** A dialog or an open menu owns the keyboard. */
  function overlayOpen(): boolean {
    return modalOpen() || document.querySelector('[role=menu]') !== null;
  }

  function onKeydown(e: KeyboardEvent) {
    if (typing(e) || e.ctrlKey || overlayOpen()) return;
    const k = keyFromEvent(e);
    if (!k) return;
    e.preventDefault();
    held = new Set(held).add(k);
    if (!e.repeat) app.select(`Key:${k}`);
  }

  function onKeyup(e: KeyboardEvent) {
    const k = keyFromEvent(e);
    if (k && held.has(k)) {
      const next = new Set(held);
      next.delete(k);
      held = next;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} onkeyup={onKeyup} onblur={() => (held = new Set())} />

<div class="wrap">
  <div class="board" style="--w: {WIDTH}; --h: {HEIGHT}">
    {#each KEYS as key (key.k)}
      {@const s = summary(key.k)}
      <button
        class="key"
        class:mapped={!!s}
        class:inh={s?.inherited}
        class:selected={app.selectedKey === `Key:${key.k}`}
        class:held={held.has(key.k)}
        style="--x: {key.x}; --y: {key.y}; --kw: {key.w}; --kh: {key.h}"
        title={s ? `${key.k}: ${s.text}` : key.k}
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
    --u: calc(100cqw / var(--w));
    position: relative;
    width: 100%;
    height: calc(var(--u) * var(--h));
    max-width: 1100px;
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
