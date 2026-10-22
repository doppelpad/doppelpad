// Control names as the desktop app shows them next to its drawings, from the app's catalogs.
// Used at build time only, so the catalogs stay out of the pages' scripts.

import { DEVICE_LAYOUTS, type DeviceId } from '../../../gui/src/assets/devices/layouts';
import de from '../../../gui/src/lib/i18n/de.json';
import en from '../../../gui/src/lib/i18n/en.json';

export type Lang = 'en' | 'de';
/** The app's name catalogs per controller family (`controls.<ns>.*`), most specific first. */
export type Family = 'xbox' | 'playstation' | 'nintendo' | 'joycon' | 'gamecube' | 'generic';

const NAMESPACES: Record<Family, string[]> = {
  xbox: [],
  generic: [],
  playstation: ['ps'],
  nintendo: ['nintendo'],
  joycon: ['joycon', 'nintendo'],
  gamecube: ['gamecube', 'nintendo'],
};

const CATALOGS: Record<Lang, unknown> = { en, de };

function lookup(cat: unknown, key: string): string | undefined {
  let node = cat;
  for (const part of key.split('.')) {
    if (!node || typeof node !== 'object' || !(part in node)) return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === 'string' ? node : undefined;
}

const t = (key: string, lang: Lang) => lookup(CATALOGS[lang], key) ?? lookup(en, key);

/** The name of drawn control `c` (`A`, `LB`, `Touchpad`, …) on a controller of `family`. */
function controlLabel(c: string, family: Family, lang: Lang): string {
  if (c === 'Touchpad') return t('views.groups.touchpad', lang) ?? c;
  for (const ns of NAMESPACES[family]) {
    const named = t(`controls.${ns}.${c}`, lang);
    if (named) return named;
  }
  return t(`controls.${c}`, lang) ?? c;
}

/** The names of every control the drawings show, by control. */
export function controlNames(drawings: DeviceId[], family: Family, lang: Lang): Record<string, string> {
  const controls = drawings.flatMap((d) => DEVICE_LAYOUTS[d].controls.map((lc) => lc.control));
  return Object.fromEntries(controls.map((c) => [c, controlLabel(c, family, lang)]));
}
