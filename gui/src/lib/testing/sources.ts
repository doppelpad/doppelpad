// The GUI's source files as text, for the tests that check a rule across all of them (no
// native dialogs, tooltips or drop-down lists; the catalogs and the code agree on the keys).
// Only tests import this module.

const raw = import.meta.glob<string>('/src/**/*.{ts,svelte}', { query: '?raw', import: 'default', eager: true });

/** Every source file but the tests and this one, as `[path, text]` (paths like `/src/App.svelte`). */
export const sources: [path: string, text: string][] = Object.entries(raw).filter(
  ([path]) => !path.endsWith('.test.ts') && !path.startsWith('/src/lib/testing/'),
);

/** The Svelte components among them. */
export const components = sources.filter(([path]) => path.endsWith('.svelte'));

/** `path:line` of each line of `sources` that `re` matches. */
export function linesMatching(files: [string, string][], re: RegExp): string[] {
  return files.flatMap(([path, text]) =>
    text
      .split('\n')
      .map((line, i) => [`${path}:${i + 1}`, line] as const)
      .filter(([, line]) => re.test(line))
      .map(([at]) => at),
  );
}
