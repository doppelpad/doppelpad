// Renders the app and tray icons from the SVGs in src-tauri/icons/source/ into
// src-tauri/icons/. Run it after editing a source SVG and commit the results.
//
//   npm run icons
//
// `tauri icon` scales one image to every size, which smears the controls at 16-32 px.
// So the small sizes come from their own sources:
//
//   app-icon.svg        48 px and up: icon.icns, icon.png, 64-256 px PNGs, ICO entries
//   app-icon-small.svg  24 and 32 px: 32x32.png, ICO entries
//   app-icon-tiny.svg   16 px: ICO entry
//   tray-icon.svg       tray.png (32 px), the system tray icon (src-tauri/src/tray.rs)
//
// icon.ico lists its largest entry first: tauri-codegen 2.6 takes the first entry as the
// window icon, later versions take the largest.

import { createRequire } from 'node:module';
import { copyFile, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const tauri = require('@tauri-apps/cli/main.js');

const SOURCE = fileURLToPath(new URL('../src-tauri/icons/source/', import.meta.url));
const ICONS = fileURLToPath(new URL('../src-tauri/icons/', import.meta.url));

const tmp = await mkdtemp(join(tmpdir(), 'doppelpad-icons-'));
try {
  // Render `source` at `sizes` (or the full default set) into a folder of its own.
  const render = async (source, sizes) => {
    const out = join(tmp, `${source}-${sizes ? sizes.join('-') : 'all'}`);
    const args = ['icon', join(SOURCE, `${source}.svg`), '-o', out];
    if (sizes) args.push('-p', sizes.join(','));
    await tauri.run(args, 'tauri');
    return (name) => join(out, name);
  };

  const all = await render('app-icon');
  for (const name of ['icon.icns', 'icon.png', '64x64.png', '128x128.png', '128x128@2x.png']) {
    await copyFile(all(name), join(ICONS, name));
  }

  const large = await render('app-icon', [48, 64, 256]);
  const small = await render('app-icon-small', [24, 32]);
  const tiny = await render('app-icon-tiny', [16]);
  const tray = await render('tray-icon', [32]);
  await copyFile(small('32x32.png'), join(ICONS, '32x32.png'));
  await copyFile(tray('32x32.png'), join(ICONS, 'tray.png'));

  const entries = [
    [256, large('256x256.png')],
    [64, large('64x64.png')],
    [48, large('48x48.png')],
    [32, small('32x32.png')],
    [24, small('24x24.png')],
    [16, tiny('16x16.png')],
  ];
  const pngs = await Promise.all(entries.map(([, path]) => readFile(path)));
  await writeFile(join(ICONS, 'icon.ico'), packIco(entries.map(([size], i) => [size, pngs[i]])));
  console.log(`icons: wrote ${ICONS}`);
} finally {
  await rm(tmp, { recursive: true, force: true });
}

// An ICO file whose entries are PNG images, which Windows supports since Vista.
function packIco(images) {
  const header = Buffer.alloc(6 + 16 * images.length);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(images.length, 4);
  let offset = header.length;
  images.forEach(([size, png], i) => {
    const entry = 6 + 16 * i;
    header.writeUInt8(size % 256, entry); // width, 0 means 256
    header.writeUInt8(size % 256, entry + 1); // height
    header.writeUInt8(0, entry + 2); // no palette
    header.writeUInt8(0, entry + 3); // reserved
    header.writeUInt16LE(1, entry + 4); // colour planes
    header.writeUInt16LE(32, entry + 6); // bits per pixel
    header.writeUInt32LE(png.length, entry + 8);
    header.writeUInt32LE(offset, entry + 12);
    offset += png.length;
  });
  return Buffer.concat([header, ...images.map(([, png]) => png)]);
}
