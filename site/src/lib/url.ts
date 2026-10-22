/** A site path (`/handbook/profiles/`) below the base the site is served from; other links as they are. */
export function withBase(href: string): string {
  if (!href.startsWith('/')) return href;
  return import.meta.env.BASE_URL.replace(/\/$/, '') + href;
}
