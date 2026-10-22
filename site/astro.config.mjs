// @ts-check
import starlight from '@astrojs/starlight';
import svelte from '@astrojs/svelte';
import { defineConfig } from 'astro/config';

// Where the site is served. For a custom domain, change this line to the domain's root
// (e.g. 'https://doppelpad.example/') and add public/CNAME with the bare domain.
const PUBLIC_URL = new URL('https://doppelpad.github.io/doppelpad/');

const REPO = 'https://github.com/doppelpad/doppelpad';

export default defineConfig({
  site: PUBLIC_URL.origin,
  base: PUBLIC_URL.pathname,
  trailingSlash: 'always',
  integrations: [
    starlight({
      title: 'Doppelpad',
      description:
        'Free, open-source controller remapping for Windows, with Linux in preview: remaps, combos, macros, shift layers, gyro aiming and virtual Xbox or DualShock 4 controllers.',
      // App icon + wordmark, made by scripts/wordmark.mjs; the title stays as hidden text.
      logo: { dark: './src/assets/logo-dark.svg', light: './src/assets/logo-light.svg', replacesTitle: true },
      favicon: '/favicon.svg',
      defaultLocale: 'root',
      locales: {
        root: { label: 'English', lang: 'en' },
        de: { label: 'Deutsch', lang: 'de' },
      },
      social: [{ icon: 'github', label: 'GitHub', href: REPO }],
      editLink: { baseUrl: `${REPO}/edit/main/site/` },
      sidebar: [
        {
          label: 'Handbook',
          translations: { de: 'Handbuch' },
          items: [
            { slug: 'handbook/desktop-app', translations: { de: 'Desktop-App' } },
            { slug: 'handbook/profiles', translations: { de: 'Profil-Referenz' } },
            { slug: 'handbook/linux' },
          ],
        },
        {
          label: 'Download',
          translations: { de: 'Herunterladen' },
          link: `${REPO}/releases/latest`,
          attrs: { rel: 'noopener' },
        },
      ],
      components: {
        Hero: './src/components/Hero.astro',
        Footer: './src/components/Footer.astro',
      },
      customCss: ['@fontsource-variable/inter/index.css', './src/styles/theme.css'],
    }),
    svelte(),
  ],
  vite: {
    // The controller drawings are imported from the desktop app in ../gui. Its tsconfig needs
    // the app's own dependencies, so the site's applies to those files too.
    server: { fs: { allow: ['..'] } },
    tsconfig: 'tsconfig.json',
  },
});
