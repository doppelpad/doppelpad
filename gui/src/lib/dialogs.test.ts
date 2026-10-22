// `window.confirm` does not wait in the app's webview (the dialog plugin replaces it with an
// async function), and questions stay in the window anyway: they go through `app.confirmAsk`,
// the app's own dialog. Only the file pickers are the system's.

import { describe, expect, it } from 'vitest';

const sources = import.meta.glob<string>('/src/**/*.{ts,svelte}', {
  query: '?raw',
  import: 'default',
  eager: true,
});
const files = Object.entries(sources).filter(([path]) => !path.endsWith('.test.ts'));

describe('dialogs', () => {
  it('nothing calls confirm()', () => {
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

  it('only the file pickers come from the dialog plugin', () => {
    const imports = files.flatMap(([path, text]) =>
      [...text.matchAll(/import\s*\{([^}]*)\}\s*from\s*'@tauri-apps\/plugin-dialog'/g)]
        .flatMap((m) => m[1]!.split(',').map((name) => name.trim()))
        .filter((name) => name && name !== 'open' && name !== 'save')
        .map((name) => `${path}: ${name}`),
    );
    expect(imports).toEqual([]);
  });
});
