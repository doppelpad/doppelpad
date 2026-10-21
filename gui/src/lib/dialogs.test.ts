// `window.confirm` does not wait in the app's webview (the dialog plugin replaces it with an
// async function), so a question must go through `confirmAsk`.

import { describe, expect, it } from 'vitest';

const sources = import.meta.glob<string>('/src/**/*.{ts,svelte}', {
  query: '?raw',
  import: 'default',
  eager: true,
});

describe('dialogs', () => {
  it('nothing calls confirm()', () => {
    const files = Object.entries(sources).filter(([path]) => !path.endsWith('.test.ts'));
    expect(files.length).toBeGreaterThan(50);
    const calls = files.flatMap(([path, text]) =>
      text
        .split('\n')
        .map((line, i) => [`${path}:${i + 1}`, line] as const)
        .filter(([, line]) => /(?<![\w$])confirm\s*\(/.test(line))
        .map(([at]) => at),
    );
    expect(calls).toEqual([]);
  });
});
