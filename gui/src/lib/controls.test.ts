// The webview's own tooltips and drop-down lists do not look like the app: tooltips go through
// `use:tip` (lib/tooltip.svelte.ts) and choices through ui/Select.svelte.

import { describe, expect, it } from 'vitest';
import { components as files } from './testing/sources';

/** Whether `text` from `from` to `to` stays inside one start tag (no `>` outside `{…}` and quotes). */
function staysInTag(text: string, from: number, to: number): boolean {
  let depth = 0;
  let quote = '';
  for (let i = from; i < to; i++) {
    const c = text[i];
    if (quote) {
      if (c === quote) quote = '';
    } else if (c === '"' || c === "'" || c === '`') quote = c;
    else if (c === '{') depth++;
    else if (c === '}') depth--;
    else if (c === '>' && depth === 0) return false;
  }
  return true;
}

/** `path:line` of each match of `re` that sits in the start tag of an HTML (lowercase) element. */
function inHtmlTags(re: RegExp): string[] {
  return files.flatMap(([path, text]) =>
    [...text.matchAll(re)]
      .filter((m) => {
        const lt = text.lastIndexOf('<', m.index);
        const tag = /^<([A-Za-z][\w:-]*)/.exec(text.slice(lt))?.[1] ?? '';
        return /^[a-z]/.test(tag) && !tag.includes(':') && staysInTag(text, lt, m.index);
      })
      .map((m) => `${path}:${text.slice(0, m.index).split('\n').length}`),
  );
}

describe('native controls', () => {
  it('no element has a title attribute or an SVG title', () => {
    expect(files.length).toBeGreaterThan(50);
    expect(inHtmlTags(/\stitle=|\s\{title\}/g)).toEqual([]);
    expect(files.flatMap(([path, text]) => (/<title[\s>]/.test(text) ? [path] : []))).toEqual([]);
  });

  it('no native drop-down lists', () => {
    expect(files.flatMap(([path, text]) => (/<select[\s>]/.test(text) ? [path] : []))).toEqual([]);
  });
});
