// `window.confirm` does not wait in the app's webview (the dialog plugin replaces it with an
// async function), and questions stay in the window anyway: they go through `app.confirmAsk`,
// the app's own dialog. Only the file pickers are the system's.

import { describe, expect, it } from 'vitest';
import { linesMatching, sources } from './testing/sources';

describe('dialogs', () => {
  it('nothing calls confirm()', () => {
    expect(sources.length).toBeGreaterThan(50);
    expect(linesMatching(sources, /(?<![\w$])confirm\s*\(/)).toEqual([]);
  });

  it('only the file pickers come from the dialog plugin', () => {
    const imports = sources.flatMap(([path, text]) =>
      [...text.matchAll(/import\s*\{([^}]*)\}\s*from\s*'@tauri-apps\/plugin-dialog'/g)]
        .flatMap((m) => m[1]!.split(',').map((name) => name.trim()))
        .filter((name) => name && name !== 'open' && name !== 'save')
        .map((name) => `${path}: ${name}`),
    );
    expect(imports).toEqual([]);
  });
});
