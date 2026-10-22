// The launch commands of a file being imported, as the question before the import lists them.

import { t } from '../i18n/index.svelte';
import type { Launch, PadFamily } from '../types';
import { comboLabel, defaultLayerName } from './labels';

/** One program with its arguments, and every place in the import that starts it. */
export interface LaunchRow {
  command: string;
  places: string[];
}

/** The command line `path args`, the path quoted when it has spaces (or is empty). */
export function commandLine(path: string, args: string): string {
  const program = path === '' || /\s/.test(path) ? `"${path}"` : path;
  return args ? `${program} ${args}` : program;
}

/** *Layer › Trigger* or *Menu (radial menu) › Sector*, after the config's name if asked. */
function placeLabel(l: Launch, family: PadFamily | null, withConfig: boolean): string {
  const p = l.place;
  const parts =
    p.kind === 'mapping'
      ? [p.layer_name || defaultLayerName(p.layer), comboLabel(p.trigger, family)]
      : [
          t('launches.radial', { name: p.name || t('radial.default_name', { n: p.menu }) }),
          p.label || t('launches.sector', { n: p.sector }),
        ];
  return (withConfig ? [l.config, ...parts] : parts).join(' › ');
}

/** One row per command line, in the order found; places name the config when there are several. */
export function launchRows(found: Launch[], family: PadFamily | null): LaunchRow[] {
  const withConfig = new Set(found.map((l) => l.file)).size > 1;
  const rows = new Map<string, string[]>();
  for (const l of found) {
    const command = commandLine(l.path, l.args);
    rows.set(command, [...(rows.get(command) ?? []), placeLabel(l, family, withConfig)]);
  }
  return [...rows].map(([command, places]) => ({ command, places }));
}
