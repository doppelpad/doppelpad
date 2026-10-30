// The games' pictures as data URLs, fetched once per game folder and picture file and shared
// by the games list and the game dialog.

import { SvelteMap } from 'svelte/reactivity';
import { api } from '../api';

/** The pictures fetched so far; `null` while one is on its way or when it could not be read. */
const pictures = new SvelteMap<string, string | null>();
/** The pictures asked for: a plain set, so reading a picture in a template does not write state. */
const asked = new Set<string>();

const keyOf = (dir: string, image: string) => `${dir}|${image}`;

/** The picture `image` of the game in `dir` (fetched the first time it is asked for), or `null`. */
export function pictureOf(dir: string, image: string | undefined): string | null {
  if (!image) return null;
  const key = keyOf(dir, image);
  if (!asked.has(key)) {
    asked.add(key);
    // A picture that cannot be read shows as the game's initial, as one without a picture.
    api
      .gamePicture(dir)
      .then((url) => pictures.set(key, url))
      .catch(() => pictures.set(key, null));
  }
  return pictures.get(key) ?? null;
}

/** Forget the pictures of the game in `dir`: its picture changed, even if under the same name. */
export function forgetPicture(dir: string) {
  for (const key of [...asked]) {
    if (key.startsWith(`${dir}|`)) {
      asked.delete(key);
      pictures.delete(key);
    }
  }
}
