// Inter's copyright notice and license (SIL Open Font License 1.1), which has to travel with the
// font files the site serves. Taken from the font package, so it stays in step with it.
import type { APIRoute } from 'astro';
import license from '@fontsource-variable/inter/LICENSE?raw';

export const GET: APIRoute = () => new Response(license, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
