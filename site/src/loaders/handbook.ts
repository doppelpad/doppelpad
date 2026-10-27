// The handbook pages, rendered straight from the repository's docs/ folder so the docs stay
// their only source. A file's first heading becomes the page title, links between handbook
// files go to their pages, and links to other files in the repository go to GitHub.

import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { docsLoader } from '@astrojs/starlight/loaders';
import type { Loader, LoaderContext } from 'astro/loaders';

export const REPO = 'https://github.com/doppelpad/doppelpad';

/** The docs/ files on the site and their page slugs below handbook/. */
const HANDBOOK: Record<string, string> = {
  'GUI.md': 'desktop-app',
  'PROFILES.md': 'profiles',
  'LINUX.md': 'linux',
  'MACOS.md': 'macos',
};

/** Starlight's pages from src/content/docs plus the handbook pages from ../docs. */
export function docsWithHandbook(): Loader {
  const pages = docsLoader();
  return {
    name: 'doppelpad-docs',
    async load(ctx) {
      await pages.load(ctx);
      const docs = new URL('../docs/', ctx.config.root);
      for (const file of Object.keys(HANDBOOK)) await loadPage(ctx, docs, file);
      ctx.watcher?.add(fileURLToPath(docs));
      ctx.watcher?.on('change', (path) => {
        const file = Object.keys(HANDBOOK).find((f) => fileURLToPath(new URL(f, docs)) === path);
        if (file) void loadPage(ctx, docs, file);
      });
    },
  };
}

async function loadPage(ctx: LoaderContext, docs: URL, file: string) {
  const url = new URL(file, docs);
  const source = (await readFile(url, 'utf8')).replace(/\r\n/g, '\n');
  const heading = source.match(/^# (.+)\n/m);
  if (!heading) throw new Error(`docs/${file} has no top-level heading to use as its title`);
  const body = rewriteLinks(source.replace(heading[0], ''), ctx.config.base);
  const id = `handbook/${HANDBOOK[file]}`;
  const data = await ctx.parseData({
    id,
    data: {
      title: heading[1].trim(),
      description: firstSentences(body),
      editUrl: `${REPO}/edit/main/docs/${file}`,
    },
  });
  ctx.store.set({
    id,
    data,
    body,
    rendered: await ctx.renderMarkdown(body, { fileURL: url }),
    digest: ctx.generateDigest(source),
  });
}

/** Rewrites relative link targets outside code blocks: handbook files to their pages, the rest to GitHub. */
function rewriteLinks(markdown: string, base: string): string {
  const prefix = base.endsWith('/') ? base : `${base}/`;
  const target = (href: string) => {
    if (/^([a-z]+:|#|\/)/i.test(href)) return href;
    const [path, hash = ''] = href.split('#');
    const resolved = new URL(path, 'https://repo/docs/').pathname.slice(1);
    const page = resolved.startsWith('docs/') ? HANDBOOK[resolved.slice('docs/'.length)] : undefined;
    const anchor = hash ? `#${hash}` : '';
    return page ? `${prefix}handbook/${page}/${anchor}` : `${REPO}/blob/main/${resolved}${anchor}`;
  };
  return markdown
    .split(/(^```[\s\S]*?^```$)/m)
    .map((part, i) =>
      i % 2
        ? part
        : part
            .replace(/\]\(([^)\s]+)\)/g, (_, href: string) => `](${target(href)})`)
            .replace(/^(\[[^\]]+\]:\s+)(\S+)/gm, (_, label: string, href: string) => label + target(href)),
    )
    .join('');
}

/** The first paragraph as plain text, cut after the last sentence that fits a meta description. */
function firstSentences(markdown: string): string {
  const para = markdown.trim().split(/\n\s*\n/)[0] ?? '';
  const text = para
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/[`*_]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  if (text.length <= 160) return text;
  const cut = text.slice(0, 160);
  const end = cut.lastIndexOf('. ');
  return end > 40 ? cut.slice(0, end + 1) : `${cut.slice(0, cut.lastIndexOf(' '))}…`;
}
