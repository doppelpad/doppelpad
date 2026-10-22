// Minimal reactive translations: `t('binding.turbo')`, `t('slots.slot', { n: 2 })`.
// English is the reference catalog; missing German keys fall back to English.

import en from './en.json';
import de from './de.json';

type Catalog = Record<string, unknown>;

export const LANGUAGES = { en: 'English', de: 'Deutsch' } as const;
export type Language = keyof typeof LANGUAGES;

export const catalogs: Record<Language, Catalog> = { en, de };

let current = $state<Language>('en');

export function setLanguage(lang: string) {
  current = (lang in catalogs ? lang : 'en') as Language;
}

export function language(): Language {
  return current;
}

function lookup(cat: Catalog, key: string): string | undefined {
  let node: unknown = cat;
  for (const part of key.split('.')) {
    if (node && typeof node === 'object' && part in (node as Catalog)) {
      node = (node as Catalog)[part];
    } else {
      return undefined;
    }
  }
  return typeof node === 'string' ? node : undefined;
}

/** True if the key exists in the reference catalog. */
export function has(key: string): boolean {
  return lookup(en, key) !== undefined;
}

export function t(key: string, params?: Record<string, string | number>): string {
  const text = lookup(catalogs[current], key) ?? lookup(en, key) ?? key;
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (_, name: string) => String(params[name] ?? `{${name}}`));
}
