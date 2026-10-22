// Builds the `doppelpad` command-line tool with the same profile and target as the app and
// copies it to src-tauri/installer/bin/ (ignored by git). The installer hooks install it
// next to doppelpad-gui.exe.
//
//   node scripts/stage-cli.mjs
//
// `tauri build` runs this as part of `beforeBundleCommand` and sets TAURI_ENV_DEBUG and
// TAURI_ENV_TARGET_TRIPLE. Without them it builds a release binary for the host.

import { execFileSync, spawn } from 'node:child_process';
import { copyFile, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = fileURLToPath(new URL('../../', import.meta.url));
const OUT_DIR = fileURLToPath(new URL('../src-tauri/installer/bin/', import.meta.url));

const debug = process.env.TAURI_ENV_DEBUG === 'true';
const target = process.env.TAURI_ENV_TARGET_TRIPLE;
const host = /^host: (\S+)/m.exec(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }))?.[1];

const args = ['build', '-p', 'doppelpad', '--bin', 'doppelpad', '--message-format=json-render-diagnostics'];
if (!debug) args.push('--release');
// Cross builds need the app's target. Where the binary ends up does not matter: cargo
// reports its path below.
if (target && target !== host) args.push('--target', target);

console.log(`stage-cli: cargo ${args.join(' ')}`);
const cargo = spawn('cargo', args, { cwd: REPO_ROOT, stdio: ['ignore', 'pipe', 'inherit'] });
let stdout = '';
cargo.stdout.on('data', (chunk) => (stdout += chunk));
const code = await new Promise((resolve, reject) => {
  cargo.on('error', reject);
  cargo.on('close', resolve);
});
if (code !== 0) {
  console.error(`stage-cli: cargo exited with code ${code}`);
  process.exit(1);
}

// With --message-format=json, cargo reports the path of every binary it produced.
const exe = stdout
  .split('\n')
  .filter((line) => line.startsWith('{'))
  .map((line) => JSON.parse(line))
  .find((m) => m.reason === 'compiler-artifact' && m.target?.name === 'doppelpad' && m.executable)?.executable;
if (!exe) {
  console.error('stage-cli: cargo did not report the doppelpad executable');
  process.exit(1);
}

await mkdir(OUT_DIR, { recursive: true });
await copyFile(exe, `${OUT_DIR}doppelpad.exe`);
console.log(`stage-cli: ${exe} -> ${OUT_DIR}doppelpad.exe`);
