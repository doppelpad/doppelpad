// Colours of the controllers' own tray icons. The palette and the pick by id match the tray
// (gui/src-tauri/src/tray.rs), so the settings show the colour the icon has.

export const PAD_COLORS = ['#3b82f6', '#ef4444', '#22c55e', '#ec4899', '#f59e0b', '#a855f7'];

/** The colour of a controller's tray icon: the chosen one, else one from the palette by its id (FNV-1a, as the tray does). */
export function padColor(id: string, chosen: Record<string, string>): string {
  const c = chosen[id];
  if (c && /^#[0-9a-fA-F]{6}$/.test(c)) return c;
  let h = 0x811c9dc5;
  for (const b of new TextEncoder().encode(id)) h = Math.imul((h ^ b) >>> 0, 0x01000193) >>> 0;
  return PAD_COLORS[h % PAD_COLORS.length]!;
}
