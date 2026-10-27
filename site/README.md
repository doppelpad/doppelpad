# Website

Doppelpad's website: a landing page in English and German, the handbook and the legal pages,
built with [Astro](https://astro.build) and [Starlight](https://starlight.astro.build) and
published on GitHub Pages by `.github/workflows/pages.yml`.

```bash
npm ci
npm run dev       # http://localhost:4321/doppelpad/
npm run build     # into dist/
npm run preview   # serves dist/
npm run check
```

| What | Where |
|---|---|
| Landing pages | `src/content/docs/index.mdx`, `src/content/docs/de/index.mdx` |
| Handbook | Rendered from `../docs` (`GUI.md`, `PROFILES.md`, `LINUX.md`, `MACOS.md`) by `src/loaders/handbook.ts`. Edit the docs there; links between them become handbook links, other relative links go to GitHub. |
| Wordmark and header logo | `src/assets/wordmark.svg` (currentColor) and `logo-{dark,light}.svg` (app icon + wordmark): Space Grotesk Bold as outlined paths, made by `npm run wordmark` (`scripts/wordmark.mjs`). No font file is loaded for them. |
| Controller drawings | The app's own (`../gui/src/assets/devices`), with control names from its catalogs, shown by `src/components/ControllerDrawing.svelte` |
| Impressum, privacy policy | `src/content/docs/{,de/}{impressum,datenschutz}.mdx`; the operator's name, address and contact are in `src/lib/operator.ts` |
| Address | `PUBLIC_URL` in `astro.config.mjs`. For a custom domain, change it and add `public/CNAME`. |

The site makes no third-party requests: the font is bundled (`@fontsource-variable/inter`),
and there are no analytics and no CDNs. Keep it that way, since the privacy policy says so.
Inter's license travels with its font files as `/licenses/inter.txt`
(`src/pages/licenses/inter.txt.ts`, taken from the package), linked in the footer.
