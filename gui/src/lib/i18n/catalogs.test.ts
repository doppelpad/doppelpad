import { describe, expect, it } from 'vitest';
import { sources } from '../testing/sources';
import en from './en.json';
import { catalogs, has } from './index.svelte';

type Tree = { [k: string]: string | Tree };

function keys(o: Tree, prefix = ''): string[] {
  return Object.entries(o).flatMap(([k, v]) => (typeof v === 'string' ? [prefix + k] : keys(v, `${prefix}${k}.`)));
}

function placeholders(o: Tree): Map<string, string> {
  const out = new Map<string, string>();
  const walk = (n: Tree, p: string) => {
    for (const [k, v] of Object.entries(n)) {
      if (typeof v === 'string')
        out.set(
          p + k,
          [...v.matchAll(/\{(\w+)\}/g)]
            .map((m) => m[1])
            .sort()
            .join(','),
        );
      else walk(v, `${p}${k}.`);
    }
  };
  walk(o, '');
  return out;
}

/**
 * Keys the GUI code does not read itself: the tray's texts, which the bridge reads from the
 * catalogs (gui/src-tauri/src/tray.rs).
 */
const READ_ELSEWHERE = ['tray.'];

describe('translations', () => {
  for (const [lang, catalog] of Object.entries(catalogs)) {
    if (lang === 'en') continue;
    it(`${lang} has exactly the English keys`, () => {
      expect(keys(catalog as Tree).sort()).toEqual(keys(en as Tree).sort());
    });

    it(`${lang} has the English placeholders`, () => {
      const e = placeholders(en as Tree);
      const other = placeholders(catalog as Tree);
      for (const [k, v] of e) expect(other.get(k), k).toBe(v);
    });
  }

  it('has every key the code asks for by name', () => {
    expect(sources.length).toBeGreaterThan(100);
    const missing: string[] = [];
    for (const [file, text] of sources) {
      for (const m of text.matchAll(/(?<![.\w])(?:t|has)\(\s*'([^'$]+)'/g)) {
        if (!has(m[1])) missing.push(`${file}: ${m[1]}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it('has no key the code does not use', () => {
    // A key is used when the code names it in quotes, or begins with the fixed part of a key
    // the code builds (`t(\`hotkeys.${field}\`)` uses every key that begins with `hotkeys.`).
    const text = sources.map(([, t]) => t).join('\n');
    const built = [...text.matchAll(/`([a-z0-9_.]+)\$\{/g)].map((m) => m[1]!);
    const used = (key: string) =>
      [`'${key}'`, `"${key}"`, `\`${key}\``].some((quoted) => text.includes(quoted)) ||
      [...built, ...READ_ELSEWHERE].some((prefix) => key.startsWith(prefix));
    expect(keys(en as Tree).filter((key) => !used(key))).toEqual([]);
  });
});
