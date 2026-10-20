import { describe, expect, it } from 'vitest';
import en from './en.json';
import de from './de.json';

type Tree = { [k: string]: string | Tree };

function keys(o: Tree, prefix = ''): string[] {
  return Object.entries(o).flatMap(([k, v]) => (typeof v === 'string' ? [prefix + k] : keys(v, `${prefix}${k}.`)));
}

function placeholders(o: Tree): Map<string, string> {
  const out = new Map<string, string>();
  const walk = (n: Tree, p: string) => {
    for (const [k, v] of Object.entries(n)) {
      if (typeof v === 'string') out.set(p + k, [...v.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(','));
      else walk(v, `${p}${k}.`);
    }
  };
  walk(o, '');
  return out;
}

describe('translations', () => {
  it('German has exactly the English keys', () => {
    expect(keys(de as Tree).sort()).toEqual(keys(en as Tree).sort());
  });

  it('placeholders match', () => {
    const e = placeholders(en as Tree);
    const d = placeholders(de as Tree);
    for (const [k, v] of e) expect(d.get(k), k).toBe(v);
  });
});
