// Writes THIRD_PARTY_LICENSES.md in the repository root: the license texts and notices of
// every Rust crate and npm package compiled into doppelpad-gui and doppelpad. The file is
// committed, and the setup and the AppImage carry it next to the programs (bundle.resources).
//
//   node scripts/third-party-licenses.mjs           regenerate the file
//   node scripts/third-party-licenses.mjs --check   fail if the file misses a crate or package
//
// Regenerate after changing Rust or npm dependencies and commit the result. This needs
// cargo-about (`cargo install cargo-about --version 0.9.2 --locked --features cli`), which
// reads its settings from about.toml, and network access for the crates clarified there.
//
// `tauri build` runs --check as part of `beforeBundleCommand`. It needs neither cargo-about
// nor the network: it lists the crates with `cargo tree` and the npm packages with an
// in-memory `vite build`, and compares them with the tables in the file.

import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'vite';

const REPO_ROOT = fileURLToPath(new URL('../../', import.meta.url));
const GUI_DIR = fileURLToPath(new URL('../', import.meta.url));
const OUT_FILE = join(REPO_ROOT, 'THIRD_PARTY_LICENSES.md');

const CARGO_ABOUT = 'cargo-about 0.9.2';
// The Windows setup is built with the GNU toolchain. The file covers MSVC builds too, which
// use a few other crates, so that it stays complete for either, and the Linux AppImage.
// cargo-about resolves all the targets it gets at once, so a crate only Linux builds use
// would bring its Windows-only dependencies along; each group is resolved on its own.
const TARGET_GROUPS = [['x86_64-pc-windows-gnu', 'x86_64-pc-windows-msvc'], ['x86_64-unknown-linux-gnu']];
const TARGETS = TARGET_GROUPS.flat();
// `tauri build` turns this on.
const FEATURES = 'dp-gui/custom-protocol';

const RUST_HEADING = 'Rust crates';
const NPM_HEADING = 'npm packages';

// Modules that bundler plugins generate, by the package that generates them.
const VIRTUAL_MODULES = [
  [/^\0vite\//, 'vite'], // the module preload polyfill
  [/^\0rolldown\//, 'rolldown'], // runtime helpers
];

// npm packages whose license files need help.
const NPM_OVERRIDES = {
  // The rest of Vite's LICENSE.md covers the code bundled into Vite itself. The app only gets
  // Vite's own module preload polyfill.
  vite: { cutAt: '\n# Licenses of bundled dependencies' },
  // Ships no license text (only LICENSE.spdx). It is built from the same directory of
  // tauri-apps/plugins-workspace as the crate of the same name and version, which does.
  '@tauri-apps/plugin-dialog': { crate: 'tauri-plugin-dialog' },
};

const LICENSE_FILE = /^(licen[cs]e|copying)([-_.]|$)/i;
const NOTICE_FILE = /^(notice|copyright)([-_.]|$)|third[-_]?party/i;
const NOT_A_TEXT = /\.(spdx|json)$/i;

const fail = (msg) => {
  console.error(`third-party-licenses: ${msg}`);
  process.exit(1);
};

const isInside = (file, dir) => {
  const rel = relative(dir, file);
  return rel !== '' && !rel.startsWith('..') && !isAbsolute(rel);
};

// Line endings, trailing spaces and blank lines at either end differ between copies of the
// same license, so they are dropped. Grouping also ignores how the text is wrapped.
const tidy = (text) =>
  text
    .replace(/^\uFEFF/, '')
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((line) => line.trimEnd())
    .join('\n')
    .replace(/^\n+/, '')
    .trimEnd();
const groupKey = (kind, text) => `${kind}\0${text.replace(/\s+/g, ' ')}`;

// License and notice files in a package directory, sorted by name.
function textFiles(dir) {
  const files = readdirSync(dir)
    .filter((name) => !NOT_A_TEXT.test(name) && statSync(join(dir, name)).isFile())
    .sort();
  return {
    licenses: files.filter((name) => LICENSE_FILE.test(name) && !NOTICE_FILE.test(name)),
    notices: files.filter((name) => NOTICE_FILE.test(name)),
  };
}

// ---------------------------------------------------------------------------------------
// Rust

// Crates as "name version" that the workspace members reach through normal dependencies
// (not dev- or build-dependencies), without the workspace members themselves.
//
// linked: true lists only the crates linked into the programs. false adds the proc-macro
// crates and what they use at compile time, which cargo-about lists too, though not always
// all of them: it resolves features differently for code that runs at compile time.
function shippedCrates(targets, { linked }) {
  const edges = linked ? 'normal,no-proc-macro' : 'normal';
  const args = ['tree', '--workspace', '--locked', '--features', FEATURES, '--edges', edges];
  for (const t of targets) args.push('--target', t);
  args.push('--prefix', 'none', '--format', '{p}');
  const out = execFileSync('cargo', args, { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  const crates = new Set();
  for (const line of out.split('\n')) {
    // "name v1.2.3", then maybe " (proc-macro)", " (*)" or, for a path dependency, its path.
    const m = /^(\S+) v(\S+)(?: \((.*?)\))?/.exec(line.trim());
    if (m && !(m[3] && isAbsolute(m[3]) && isInside(m[3], REPO_ROOT))) crates.add(`${m[1]} ${m[2]}`);
  }
  return crates;
}

// cargo-about's JSON for every target group, merged; its files go to `dir`.
async function runCargoAbout(dir) {
  let version;
  try {
    version = execFileSync('cargo', ['about', '--version'], { encoding: 'utf8' }).trim();
  } catch {
    version = 'not installed';
  }
  if (version !== CARGO_ABOUT) {
    fail(
      `needs ${CARGO_ABOUT} (found: ${version}). Install it with\n` +
        `  cargo install cargo-about --version ${CARGO_ABOUT.split(' ')[1]} --locked --features cli`,
    );
  }
  const results = [];
  for (const [i, group] of TARGET_GROUPS.entries()) {
    const outFile = join(dir, `about-${i}.json`);
    const args = ['about', 'generate', '--format', 'json', '--config', 'about.toml', '--workspace'];
    args.push('--features', FEATURES, '--locked', '--fail', '--output-file', outFile);
    for (const t of group) args.push('--target', t);
    console.log(`third-party-licenses: cargo ${args.join(' ')}`);
    execFileSync('cargo', args, { cwd: REPO_ROOT, stdio: ['ignore', 'inherit', 'inherit'] });
    results.push(JSON.parse(await readFile(outFile, 'utf8')));
  }
  return mergeAbout(results);
}

// One cargo-about result from several: each crate once, and each license text once with
// every crate that uses it.
function mergeAbout(results) {
  const crates = new Map();
  const licenses = new Map();
  for (const r of results) {
    for (const c of r.crates) crates.set(c.package.id, c);
    for (const l of r.licenses) {
      const key = `${l.id}\0${l.text}`;
      const known = licenses.get(key);
      if (!known) {
        licenses.set(key, { ...l, used_by: [...l.used_by] });
        continue;
      }
      for (const u of l.used_by) {
        if (!known.used_by.some((k) => k.crate.id === u.crate.id)) known.used_by.push(u);
      }
    }
  }
  return { crates: [...crates.values()], licenses: [...licenses.values()] };
}

// Crates and their texts from cargo-about's JSON output.
function rustFromAbout(about) {
  const crates = new Map();
  for (const { package: p } of about.crates) {
    if (!p.source) continue; // a workspace member: Doppelpad itself
    crates.set(p.id, {
      name: p.name,
      version: p.version,
      license: p.license ?? 'see license text',
      usedUnder: new Set(),
      dir: dirname(p.manifest_path),
    });
  }
  const texts = [];
  for (const l of about.licenses) {
    const users = l.used_by.map((u) => crates.get(u.crate.id)).filter(Boolean);
    for (const c of users) c.usedUnder.add(l.id);
    if (users.length) texts.push({ kind: 'license', title: l.id, text: l.text, users: users.map(label) });
  }
  for (const c of crates.values()) {
    if (!c.usedUnder.size) fail(`cargo-about found no license text for ${label(c)}`);
    for (const name of textFiles(c.dir).notices) {
      texts.push({ kind: 'notice', title: name, text: readFileSync(join(c.dir, name), 'utf8'), users: [label(c)] });
    }
  }
  return { crates: [...crates.values()], texts };
}

// ---------------------------------------------------------------------------------------
// npm

// The npm packages that end up in dist/: the modules and assets of a production build of
// the frontend, done in memory so that dist/ stays as it is.
async function shippedPackages() {
  const ids = new Set();
  await build({
    root: GUI_DIR,
    logLevel: 'error',
    build: { write: false },
    plugins: [
      {
        name: 'third-party-licenses',
        generateBundle(_, bundle) {
          for (const out of Object.values(bundle)) {
            if (out.type === 'chunk') out.moduleIds.forEach((id) => ids.add(id));
            else (out.originalFileNames ?? []).forEach((f) => ids.add(f));
          }
        },
      },
    ],
  });

  const dirs = new Set();
  for (const id of ids) {
    if (id.startsWith('\0')) {
      const hit = VIRTUAL_MODULES.find(([re]) => re.test(id));
      if (!hit) fail(`module ${JSON.stringify(id)} comes from an unknown plugin: add it to VIRTUAL_MODULES`);
      dirs.add(join(GUI_DIR, 'node_modules', hit[1]));
      continue;
    }
    const file = resolve(GUI_DIR, id.replace(/[?#].*$/, ''));
    const parts = file.split(sep);
    const i = parts.lastIndexOf('node_modules');
    if (i >= 0) dirs.add(parts.slice(0, i + (parts[i + 1].startsWith('@') ? 3 : 2)).join(sep));
    else if (!isInside(file, GUI_DIR)) fail(`module ${file} is neither part of gui/ nor of an npm package`);
  }

  return [...dirs].map((dir) => {
    const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
    const license =
      typeof pkg.license === 'string' ? pkg.license : (pkg.license?.type ?? (pkg.licenses ?? []).map((l) => l.type ?? l).join(' OR '));
    return { name: pkg.name, version: pkg.version, license: license || 'see license text', dir };
  });
}

// The license id a file of an npm package holds, for the heading.
function npmLicenseId(file, expression) {
  if (/(^|[-_.])apache/i.test(file)) return 'Apache-2.0';
  if (/(^|[-_.])mit([-_.]|$)/i.test(file)) return 'MIT';
  return /^[\w.+-]+$/.test(expression) ? expression : 'License';
}

function npmTexts(pkg, crates) {
  const override = NPM_OVERRIDES[pkg.name] ?? {};
  let dir = pkg.dir;
  if (override.crate) {
    const c = crates.find((c) => c.name === override.crate && c.version === pkg.version);
    if (!c) fail(`${label(pkg)}: crate ${override.crate} ${pkg.version} is not in the build (NPM_OVERRIDES)`);
    dir = c.dir;
  }
  const { licenses, notices } = textFiles(dir);
  if (!licenses.length) fail(`${label(pkg)} has no license file: add it to NPM_OVERRIDES`);
  const texts = [];
  for (const name of licenses) {
    let text = readFileSync(join(dir, name), 'utf8');
    if (override.cutAt) {
      const at = text.indexOf(override.cutAt);
      if (at < 0) fail(`${label(pkg)}: "${override.cutAt.trim()}" not found in ${name} (NPM_OVERRIDES)`);
      text = text.slice(0, at);
    }
    texts.push({ kind: 'license', title: npmLicenseId(name, pkg.license), text, users: [label(pkg)] });
  }
  for (const name of notices) {
    texts.push({ kind: 'notice', title: name, text: readFileSync(join(dir, name), 'utf8'), users: [label(pkg)] });
  }
  return texts;
}

// ---------------------------------------------------------------------------------------
// The file

const label = (p) => `${p.name} ${p.version}`;
const byLabel = (a, b) => a.name.localeCompare(b.name, 'en') || a.version.localeCompare(b.version, 'en', { numeric: true });

// One block per distinct text, listing everything it applies to.
function groupTexts(texts) {
  const groups = new Map();
  for (const t of texts) {
    const text = tidy(t.text);
    const key = groupKey(t.kind, text);
    const g = groups.get(key) ?? { kind: t.kind, title: t.title, text, users: new Set() };
    t.users.forEach((u) => g.users.add(u));
    groups.set(key, g);
  }
  const sorted = [...groups.values()].map((g) => ({ ...g, users: [...g.users].sort() }));
  return sorted.sort(
    (a, b) =>
      a.kind.localeCompare(b.kind) ||
      a.title.localeCompare(b.title, 'en') ||
      b.users.length - a.users.length ||
      a.users[0].localeCompare(b.users[0], 'en'),
  );
}

function fenced(text) {
  const longest = Math.max(2, ...[...text.matchAll(/`+/g)].map((m) => m[0].length));
  const fence = '`'.repeat(longest + 1);
  return [`${fence}text`, text, fence];
}

function render(crates, packages, groups) {
  const out = [
    '# Third-party licenses',
    '',
    '<!-- Generated by gui/scripts/third-party-licenses.mjs. Do not edit: run `node scripts/third-party-licenses.mjs` in gui/. -->',
    '',
    "Doppelpad's programs, `doppelpad-gui` and `doppelpad` (`.exe` on Windows), contain the",
    'open-source Rust crates and npm packages listed below. The tables give the license each one',
    'is distributed under; where it offers a choice, *Used under* names the license Doppelpad',
    'uses. The license texts and notices follow, each with the crates and packages it covers.',
    '',
    'The source code of every crate is available on <https://crates.io> and of every package on',
    '<https://www.npmjs.com>, at the version listed. The lists cover 64-bit Windows builds with',
    'the GNU and the MSVC toolchain and 64-bit Linux builds, so some crates appear that only',
    'one of them uses.',
    '',
    'The drivers that the setup installs, the WebView2 loader, the runtime libraries of the',
    'compiler and the system libraries in the AppImage are covered in THIRD_PARTY_NOTICES.md.',
    '',
    `## ${RUST_HEADING}`,
    '',
    `${crates.length} crates.`,
    '',
    '| Crate | Version | License | Used under |',
    '|---|---|---|---|',
  ];
  for (const c of [...crates].sort(byLabel)) {
    const usedUnder = [...c.usedUnder].sort().join(' AND ');
    out.push(`| [${c.name}](https://crates.io/crates/${c.name}/${c.version}) | ${c.version} | ${c.license} | ${usedUnder} |`);
  }
  out.push(
    '',
    `## ${NPM_HEADING}`,
    '',
    "The app's user interface, which is embedded in `doppelpad-gui`, contains these.",
    '',
    '| Package | Version | License |',
    '|---|---|---|',
  );
  for (const p of [...packages].sort(byLabel)) {
    out.push(`| [${p.name}](https://www.npmjs.com/package/${p.name}/v/${p.version}) | ${p.version} | ${p.license} |`);
  }
  for (const [kind, heading, intro] of [
    ['license', 'License texts', null],
    ['notice', 'Notices', 'Further notices that come with some of the crates and packages.'],
  ]) {
    out.push('', `## ${heading}`);
    if (intro) out.push('', intro);
    for (const g of groups.filter((g) => g.kind === kind)) {
      out.push('', `### ${g.title}`, '', `${kind === 'license' ? 'Used by' : 'From'}: ${g.users.join(', ')}`, '');
      out.push(...fenced(g.text));
    }
  }
  return out.join('\n') + '\n';
}

// "name version" of every row in the table under `## heading`.
function listed(md, heading) {
  const start = md.indexOf(`\n## ${heading}\n`);
  if (start < 0) fail(`THIRD_PARTY_LICENSES.md has no "${heading}" section`);
  const end = md.indexOf('\n## ', start + 1);
  const rows = md.slice(start, end < 0 ? undefined : end).matchAll(/^\| \[([^\]]+)\]\([^)]*\) \| ([^ |]+) \|/gm);
  return new Set([...rows].map((m) => `${m[1]} ${m[2]}`));
}

// What `required` has and the file lacks is an error. What the file lists beyond
// `possible` is only stale, so a warning.
function compare(what, inFile, required, possible) {
  const extra = [...inFile].filter((x) => !possible.has(x)).sort();
  if (extra.length) console.warn(`third-party-licenses: listed but no longer used (${what}): ${extra.join(', ')}`);
  return [...required]
    .filter((x) => !inFile.has(x))
    .sort()
    .map((x) => `${what} ${x}`);
}

async function check() {
  let md;
  try {
    md = (await readFile(OUT_FILE, 'utf8')).replace(/\r\n/g, '\n');
  } catch (e) {
    if (e.code === 'ENOENT') fail('THIRD_PARTY_LICENSES.md is missing: run `node scripts/third-party-licenses.mjs`');
    throw e;
  }
  // `tauri build` passes the target it builds for.
  const host = /^host: (\S+)/m.exec(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }))?.[1];
  const target = process.env.TAURI_ENV_TARGET_TRIPLE || host;
  if (!TARGETS.includes(target)) fail(`THIRD_PARTY_LICENSES.md does not cover the target ${target} (TARGETS)`);
  const linked = shippedCrates([target], { linked: true });
  const all = shippedCrates(TARGETS, { linked: false });
  const npm = new Set((await shippedPackages()).map(label));
  const missing = [
    ...compare('crate', listed(md, RUST_HEADING), linked, all),
    ...compare('npm package', listed(md, NPM_HEADING), npm, npm),
  ];
  if (missing.length) {
    fail(
      `THIRD_PARTY_LICENSES.md is out of date. Not listed:\n  ${missing.join('\n  ')}\n` +
        'Run `node scripts/third-party-licenses.mjs` in gui/ and commit the result.',
    );
  }
  console.log(`third-party-licenses: THIRD_PARTY_LICENSES.md covers ${target}`);
}

async function generate() {
  const tmp = await mkdtemp(join(tmpdir(), 'doppelpad-licenses-'));
  let about;
  try {
    about = await runCargoAbout(tmp);
  } finally {
    await rm(tmp, { recursive: true, force: true });
  }
  const rust = rustFromAbout(about);
  const packages = await shippedPackages();
  const texts = [...rust.texts, ...packages.flatMap((p) => npmTexts(p, rust.crates))];

  // --check must accept the new file without warnings.
  const fromAbout = new Set(rust.crates.map(label));
  const unlisted = [...shippedCrates(TARGETS, { linked: true })].filter((x) => !fromAbout.has(x));
  if (unlisted.length) fail(`cargo-about did not list these crates: ${unlisted.sort().join(', ')}`);
  const all = shippedCrates(TARGETS, { linked: false });
  const unknown = [...fromAbout].filter((x) => !all.has(x));
  if (unknown.length) fail(`cargo-about lists crates that cargo tree does not: ${unknown.sort().join(', ')}`);

  const groups = groupTexts(texts);
  await writeFile(OUT_FILE, render(rust.crates, packages, groups));
  console.log(
    `third-party-licenses: wrote THIRD_PARTY_LICENSES.md (${rust.crates.length} crates, ` +
      `${packages.length} npm packages, ${groups.length} texts)`,
  );
}

const args = process.argv.slice(2);
if (args.some((a) => a !== '--check')) fail(`unknown argument: ${args.find((a) => a !== '--check')}`);
await (args.includes('--check') ? check() : generate());
