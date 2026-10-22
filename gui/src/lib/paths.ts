// File paths as the bridge sends them (Windows or Unix separators).

const trimEnd = (p: string) => p.replace(/[\\/]+$/, '');

/** `path` in a folder moved from `from` to `to`; unchanged when it is not inside `from`. */
export function movedPath(path: string, from: string, to: string): string {
  const base = trimEnd(from);
  if (path === base) return trimEnd(to);
  const rest = path.slice(base.length);
  return path.startsWith(base) && /^[\\/]/.test(rest) ? trimEnd(to) + rest : path;
}
