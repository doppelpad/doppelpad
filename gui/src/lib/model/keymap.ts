// Browser `KeyboardEvent.code` → Doppelpad key name (profile syntax). Codes are
// physical key positions, so this is independent of the keyboard layout.
// A Rust test in gui/src-tauri checks that every name here is a valid key.

const LETTERS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('');

export const KEY_CODE_MAP: Record<string, string> = {
  ...Object.fromEntries(LETTERS.map((l) => [`Key${l}`, l])),
  ...Object.fromEntries([...Array(10).keys()].map((d) => [`Digit${d}`, `Digit${d}`])),
  ...Object.fromEntries([...Array(24).keys()].map((i) => [`F${i + 1}`, `F${i + 1}`])),
  ...Object.fromEntries([...Array(10).keys()].map((d) => [`Numpad${d}`, `Numpad${d}`])),
  Escape: 'Escape',
  Minus: 'Minus',
  Equal: 'Equal',
  Backspace: 'Backspace',
  Tab: 'Tab',
  BracketLeft: 'LeftBracket',
  BracketRight: 'RightBracket',
  Enter: 'Enter',
  ControlLeft: 'LeftCtrl',
  ControlRight: 'RightCtrl',
  Semicolon: 'Semicolon',
  Quote: 'Apostrophe',
  Backquote: 'Grave',
  ShiftLeft: 'LeftShift',
  ShiftRight: 'RightShift',
  Backslash: 'Backslash',
  IntlBackslash: 'IntlBackslash',
  Comma: 'Comma',
  Period: 'Period',
  Slash: 'Slash',
  AltLeft: 'LeftAlt',
  AltRight: 'RightAlt',
  MetaLeft: 'LeftMeta',
  MetaRight: 'RightMeta',
  ContextMenu: 'ContextMenu',
  Space: 'Space',
  CapsLock: 'CapsLock',
  NumLock: 'NumLock',
  ScrollLock: 'ScrollLock',
  Pause: 'Pause',
  PrintScreen: 'PrintScreen',
  Insert: 'Insert',
  Delete: 'Delete',
  Home: 'Home',
  End: 'End',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  NumpadAdd: 'NumpadAdd',
  NumpadSubtract: 'NumpadSubtract',
  NumpadMultiply: 'NumpadMultiply',
  NumpadDivide: 'NumpadDivide',
  NumpadDecimal: 'NumpadDecimal',
  NumpadEnter: 'NumpadEnter',
  NumpadEqual: 'NumpadEqual',
  NumpadComma: 'NumpadComma',
  AudioVolumeMute: 'VolumeMute',
  AudioVolumeDown: 'VolumeDown',
  AudioVolumeUp: 'VolumeUp',
  MediaPlayPause: 'MediaPlayPause',
  MediaStop: 'MediaStop',
  MediaTrackNext: 'MediaNext',
  MediaTrackPrevious: 'MediaPrevious',
  BrowserHome: 'BrowserHome',
  BrowserSearch: 'BrowserSearch',
  BrowserFavorites: 'BrowserFavorites',
  BrowserRefresh: 'BrowserRefresh',
  BrowserStop: 'BrowserStop',
  BrowserForward: 'BrowserForward',
  BrowserBack: 'BrowserBack',
  LaunchMail: 'LaunchMail',
  LaunchApp1: 'LaunchMyComputer',
  LaunchApp2: 'LaunchCalculator',
  IntlRo: 'IntlRo',
  IntlYen: 'IntlYen',
  Convert: 'Convert',
  NonConvert: 'NonConvert',
  KanaMode: 'Kana',
};

/** Key name for a keyboard event, or null for keys Doppelpad does not know. */
export function keyFromEvent(e: { code: string }): string | null {
  return KEY_CODE_MAP[e.code] ?? null;
}

/** The key picker's groups in their order; keys that fit none are "other". */
const KEY_GROUPS: { id: string; test: (k: string) => boolean }[] = [
  { id: 'letters', test: (k) => /^[A-Z]$/.test(k) },
  { id: 'digits', test: (k) => /^Digit\d$/.test(k) },
  { id: 'modifiers', test: (k) => /(Ctrl|Shift|Alt|Win|Meta)$/.test(k) },
  { id: 'function', test: (k) => /^F\d+$/.test(k) },
  { id: 'navigation', test: (k) => /^(Up|Down|Left|Right|Home|End|PageUp|PageDown|Insert|Delete)$/.test(k) },
  { id: 'numpad', test: (k) => k.startsWith('Numpad') },
  { id: 'media', test: (k) => /^(Media|Volume|Browser|Launch)/.test(k) },
];

/** `keys` that contain `search` (any case), by group; empty groups are left out. */
export function keyGroups(keys: string[], search: string): { id: string; keys: string[] }[] {
  const found = keys.filter((k) => !search || k.toLowerCase().includes(search.toLowerCase()));
  const groups = new Map<string, string[]>();
  for (const k of found) {
    const g = KEY_GROUPS.find((g) => g.test(k))?.id ?? 'other';
    groups.set(g, [...(groups.get(g) ?? []), k]);
  }
  return [...KEY_GROUPS.map((g) => g.id), 'other'].filter((g) => groups.has(g)).map((g) => ({ id: g, keys: groups.get(g)! }));
}

/** A key as its picker button shows it ("1" for `Digit1`, "Num 1" for `Numpad1`). */
export function keyLabel(k: string): string {
  if (/^Digit\d$/.test(k)) return k.slice(5);
  if (k.startsWith('Numpad')) return 'Num ' + k.slice(6);
  return k;
}
