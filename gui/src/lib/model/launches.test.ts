import { describe, expect, it } from 'vitest';
import { setLanguage } from '../i18n/index.svelte';
import type { Combo, Launch, LaunchPlace } from '../types';
import { commandLine, launchRows } from './launches';

setLanguage('en');

const at = (file: string | null, place: LaunchPlace, path: string, args = ''): Launch => ({
  file,
  config: file === 'b.toml' ? 'Other' : 'Shared',
  place,
  path,
  args,
});
const main = (trigger: Combo): LaunchPlace => ({ kind: 'mapping', layer: 0, layer_name: '', trigger });

describe('launch commands', () => {
  it('reads as a command line', () => {
    expect(commandLine('powershell', '-enc ZQBj')).toBe('powershell -enc ZQBj');
    expect(commandLine('C:\\Program Files\\x.exe', '')).toBe('"C:\\Program Files\\x.exe"');
    expect(commandLine('', '/c')).toBe('"" /c');
  });

  it('lists each command once, with every place that starts it', () => {
    const rows = launchRows(
      [
        at(null, main('A'), 'calc.exe'),
        at(null, { kind: 'mapping', layer: 2, layer_name: 'Aim', trigger: ['LB', 'B'] }, 'cmd', '/c del x'),
        at(null, main('Y'), 'calc.exe'),
        at(null, { kind: 'radial', menu: 1, name: '', sector: 3, label: '' }, 'notepad.exe'),
        at(null, { kind: 'radial', menu: 2, name: 'Tools', sector: 1, label: 'Notes' }, 'notepad.exe'),
      ],
      'xbox',
    );
    expect(rows).toEqual([
      { command: 'calc.exe', places: ['Main › A', 'Main › Y'] },
      { command: 'cmd /c del x', places: ['Aim › B + Left bumper'] },
      { command: 'notepad.exe', places: ['Menu 1 (radial menu) › Sector 3', 'Tools (radial menu) › Notes'] },
    ]);
  });

  it('names the config when the import has several', () => {
    const rows = launchRows([at('a.toml', main('A'), 'calc.exe'), at('b.toml', main('A'), 'calc.exe')], null);
    expect(rows).toEqual([{ command: 'calc.exe', places: ['Shared › Main › A', 'Other › Main › A'] }]);
  });
});
