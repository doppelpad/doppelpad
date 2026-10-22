import { describe, expect, it } from 'vitest';
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

/** Every source file of the GUI, as text. */
const sources = import.meta.glob('../../**/*.{ts,svelte}', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

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
    expect(Object.keys(sources).length).toBeGreaterThan(100);
    const missing: string[] = [];
    for (const [file, text] of Object.entries(sources)) {
      for (const m of text.matchAll(/(?<![.\w])(?:t|has)\(\s*'([^'$]+)'/g)) {
        if (!has(m[1])) missing.push(`${file}: ${m[1]}`);
      }
    }
    expect(missing).toEqual([]);
  });
});
