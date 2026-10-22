// Inter's copyright notice and license (SIL Open Font License 1.1), which has to travel with the
// font files the site serves. Taken from the font package, so it stays in step with it. The path
// is relative on purpose: a package import is left to Node in the server build, which on Linux
// then loads the license as JavaScript; a relative one is read and inlined by Vite.
import type { APIRoute } from 'astro';
import license from '../../../node_modules/@fontsource-variable/inter/LICENSE?raw';

export const GET: APIRoute = () => new Response(license, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
