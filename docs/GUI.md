# Desktop app

`doppelpad-gui` is the editor and the remapping service in one program. It runs the same
engine as `doppelpad daemon`. While it runs, `doppelpad status`, `apply` and `stop` talk to
it over the usual IPC socket. Closing the window keeps remapping running in the tray (see
[Tray](#tray)), and **Quit** in the tray menu stops it and shows hidden controllers again.

The app is built with [Tauri 2](https://tauri.app) and a Svelte 5 + TypeScript frontend
(`gui/`). The Rust side (`gui/src-tauri`, crate `dp-gui`) hosts `dp-service`. The frontend
only edits profiles and sends commands.

## Window layout

| Area | What it does |
|---|---|
| **Games** (left) | One entry per game. A game has one or more configs and an optional list of programs (`game.exe`) that switch to it automatically. A green dot marks the game whose config is running. Configs outside a game folder appear under *Other configs*. |
| **Header** | *Game › Config* picker (switch, create, import a JSON profile), **Save** (Ctrl+S; a dot means unsaved changes), options (rename, duplicate, show in Explorer, clear, delete), settings. Imported JSON files are read-only; duplicate them to edit. The header and the top of the games list also act as the title bar: drag them to move the window, double-click to maximize. On Windows and Linux the app draws its own minimize, maximize and close buttons at the right. On macOS the system buttons sit at the top left. |
| **Toolbar** | Views: *Controller*, *Back buttons*, *Keyboard*, *Mouse*. Layer chips: *Main* plus shift layers, which you can add, rename, copy, paste, clear and remove. On the right: *Shortcuts* and *Output device*. |
| **Centre** | The selected view. Click a control to edit it. On the controller views, pressing a button on the controller selects it. On the keyboard view, pressing a key does the same. |
| **Binding panel** (right) | See below. With nothing selected, it lists the mappings of the current layer. |
| **Bottom bar** | Connected controllers (click one to use it), driver warnings, **Remap ON/OFF**, *Apply to slot n* (saves, puts the config in that slot and activates it), *Autodetect* (switch configs by the game in the foreground; only while remapping is on), and the four slots (click to activate, which also turns remapping on; × to unload). |

## Editing a control

1. Pick the layer in the toolbar and click the control (or press it).
2. Choose the **activator** from the six icons: single press, long press, double press,
   triple press, start press and release. A dot under an icon means it is already mapped.
3. **Add** actions: keyboard (from the list, or *Press a key…* and press it), mouse
   buttons, wheel and movement, controller buttons, sticks and triggers, or commands
   (turn remapping off, toggle gyro, launch a program, pause, rumble). Several actions
   fire together, as a chord.
4. Optional:
   - **Turbo** repeats the actions while the control is held, at the given period.
   - **Toggle** makes the first press turn the actions on and the next press turn them off.
   - **Block this control** makes it do nothing, and stops it from passing through.
   - **Layer** switches to another layer *while held*, as a *toggle*, or with *switch*
     (with an optional delay).
   - **Description** is a note for yourself.
   - **Timing** overrides the long-press, double- and triple-press windows or the key
     delay for this mapping only.
5. On a shift layer, controls without their own mapping inherit the one from *Main*. The
   panel says so. Editing such a control creates an override on the current layer.

**Shortcuts** are mappings that fire when 2–4 controls are pressed together. Open
*Shortcuts* and choose *New shortcut*. Then click or press the controls, confirm, and edit
the shortcut in the binding panel like any single control.

The **macro editor** opens from the binding panel. It has four modes:

- *Hold until release*: steps stay pressed while the control is held.
- *Execute at once*: runs the steps once.
- *Repeat*: runs N times, with a pause between runs.
- *Repeat while held*: runs again for as long as the control is held.

You can add steps with the pickers, add pauses and rumble, reorder or delete steps, and
choose *down*, *up* or *tap* for keys and buttons. **Record** captures keyboard presses
together with the pauses between them.

**Output device** sets the virtual controller (Xbox 360, DualShock 4, or none for keyboard
and mouse only). It also covers hiding the physical controller from games, passing
unmapped controls through, and blocking the original keys of remapped controls. Missing
drivers are shown here and in the bottom bar.

## Remapping while the window is open

Remapping pauses while the Doppelpad window has keyboard focus. Mapped keys therefore
don't land in the editor, and pressing a button selects it instead of triggering its
mapping. The bottom bar then shows *Paused while editing*. Switch to the game (or hide
the window) and the active slot takes over again. *Settings → Keep remapping active while
this window is focused* turns this off.

## Tray

The tray icon is greyed out while remapping is off. Its tooltip names the active config
and warns when no controller is connected. Left-click opens the window, middle-click
turns remapping on or off. The right-click menu has:

- **Open Doppelpad**
- the active config (*Game › Config*) or *No config active*
- **Remapping on** and **Autodetect games**, the same switches as in the bottom bar
- the four slots with their configs; choosing one activates it and turns remapping on
- **Quit**

The tray follows every change, whether it comes from the window, from `doppelpad`
commands, or from a mapping that turns remapping off.

## reWASD running at start-up

While reWASD runs, its service and driver keep the controllers to themselves, and
Doppelpad cannot read them. If the app finds reWASD running when it starts, it lists what
runs and offers to quit it: it stops the "Mapping Service" (`jlserv`), for which Windows
asks for administrator permission, then closes reWASD's programs (asked to close first,
terminated after two seconds). Doppelpad then looks for controllers again right away.
reWASD starts again with Windows or when it is opened.

*Remember my choice* stores the answer; *Settings → When reWASD is running at start-up*
changes it later (*Ask*, *Quit it*, *Leave it running*) and offers *Quit reWASD* while it
runs. The setting appears only when reWASD is installed. `doppelpad doctor` also reports a
running reWASD.

## Files

| File | Content |
|---|---|
| `%APPDATA%\doppelpad\profiles\<game>\game.toml` | Game name, programs for autodetect, default config |
| `%APPDATA%\doppelpad\profiles\<game>\<config>.toml` | One profile (the normal TOML format, see [PROFILES.md](PROFILES.md)) |
| `%APPDATA%\doppelpad\state.toml` | The four slots, active slot, remap on/off, autodetect |
| `%APPDATA%\doppelpad\gui.toml` | Language, focus pause, press-to-select |
| `%LOCALAPPDATA%\doppelpad\logs\doppelpad-gui.log` | Log of the current run; the previous run's log is kept as `doppelpad-gui.log.1` |

The app saves atomically (it writes a temporary file, then renames it). Before it
overwrites a profile that it did not write itself, it keeps one `.bak` copy. Deleting
moves files to the recycle bin.

### Log file

When reporting a problem, attach `doppelpad-gui.log` (and `doppelpad-gui.log.1` if the
problem happened before the app was last restarted). *Settings → Open log folder* shows
the file in Explorer. The log starts with the app version, the Windows version and
whether ViGEmBus and HidHide are available, and it records warnings, the controllers in
use and crashes. Each start replaces the log, and a single run stops logging after
16 MiB. `DOPPELPAD_LOG` sets the detail level (default `info`; for example
`DOPPELPAD_LOG=debug`). On Linux the log is in `$XDG_STATE_HOME/doppelpad/logs`
(default `~/.local/state/doppelpad/logs`).

## Building

Requirements: Rust (MSVC or GNU toolchain, see the [README](../README.md#build)) and
Node.js 20 or newer. The WebView2 runtime ships with Windows 10/11.

```bash
cd gui
npm install
npx tauri build --no-bundle   # → target/release/doppelpad-gui.exe
npx tauri build               # the same, plus the Windows setup (see Installer)
npx tauri dev                 # development build with hot reload
npm test                      # frontend unit tests
npm run check                 # type check
npm run icons                 # re-render src-tauri/icons/ from icons/source/*.svg
```

The GUI crate is not in the workspace's `default-members`, so `cargo build` and
`cargo test` at the root build only the CLI. Use `cargo test -p dp-gui` for the bridge
tests. They check that the frontend's key table and JSON shapes match the Rust model,
using the fixtures in `gui/src/lib/model/fixtures`. Regenerate them with
`DOPPELPAD_UPDATE_FIXTURES=1 cargo test -p dp-gui`.

## Installer

`npx tauri build` without `--no-bundle` also builds the Windows setup,
`target/release/bundle/nsis/Doppelpad_<version>_x64-setup.exe`. It installs for all users
to `C:\Program Files\Doppelpad`: `doppelpad-gui.exe`, the command-line tool `doppelpad.exe`,
the license files, `THIRD_PARTY_NOTICES.md` and `THIRD_PARTY_LICENSES.md`.

Before bundling, `beforeBundleCommand` checks `THIRD_PARTY_LICENSES.md` (see
[Third-party licenses](#third-party-licenses)) and runs two scripts. Their output folders
are ignored by git:

| Script | What it does |
|---|---|
| `scripts/fetch-drivers.mjs` | Downloads the official ViGEmBus 1.22.0 and HidHide 1.5.230 setups from GitHub into `src-tauri/installer/drivers/` and checks their pinned SHA-256 hashes. Files that are already there with the right hash are kept, so only the first build needs the network. |
| `scripts/stage-cli.mjs` | Builds `doppelpad.exe` with the app's profile and target and copies it to `src-tauri/installer/bin/`. |

The NSIS hooks in `src-tauri/installer/hooks.nsh` embed both into the setup.

### Third-party licenses

`THIRD_PARTY_LICENSES.md` in the repository root lists every Rust crate and npm package
compiled into the two programs, with their license texts and notices. `THIRD_PARTY_NOTICES.md`
covers the rest: the drivers, `WebView2Loader.dll` and the runtime libraries of the
compiler. The setup installs both through `bundle.resources`.

`THIRD_PARTY_LICENSES.md` is generated and committed. After changing Rust or npm
dependencies, regenerate it in `gui/` and commit the result:

```bash
node scripts/third-party-licenses.mjs
```

This needs cargo-about 0.9.2 (`cargo install cargo-about --version 0.9.2 --locked --features cli`),
which reads `about.toml` in the repository root, and network access for the crates whose
license text `about.toml` fetches from their repository. The crates are those of the
workspace for 64-bit Windows, GNU and MSVC toolchain, without dev- and build-dependencies.
The npm packages are those that a production build of the frontend puts into `dist/`: the
runtime dependencies, the Svelte runtime and Vite's module preload polyfill.

`node scripts/third-party-licenses.mjs --check`, the first step of `beforeBundleCommand`,
fails the build when a crate linked into the programs or an npm package in `dist/` is
missing from the file, for example after a dependency update. It needs neither cargo-about
nor the network.

### Drivers

The welcome page has a checkbox, on by default, that installs the drivers that are
missing. A driver counts as installed when its service (`ViGEmBus`, `HidHide`) is
registered and the driver file it points to exists. The setup unpacks the official driver
setup to a temporary folder and runs it with `/exenoui /qn /norestart`, which installs it
without any UI. Each driver setup writes a log to `%TEMP%\doppelpad-<driver>-setup.log`. If a
driver setup fails, Doppelpad is still installed, and a message says where to get the
driver.

HidHide hides controllers only after Windows restarts. After installing it, the finish
page offers to restart now or later.

Uninstalling Doppelpad never removes the drivers, because other programs may use them.
They are listed separately under *Installed apps* as "ViGEm Bus Driver" and "HidHide".
*Delete the application data* in the uninstaller also deletes `%APPDATA%\doppelpad`
(profiles and settings) and `%LOCALAPPDATA%\doppelpad\logs`.

| Command line | Drivers |
|---|---|
| `/S` (silent), `/P` (progress only) | Missing drivers are installed; Windows is never restarted |
| `/NODRIVERS` | Not installed |
| Updates (`/UPDATE`) | Left alone |

To move to another driver version, change its entry (URL, file name, SHA-256) in
`scripts/fetch-drivers.mjs` and the table in `THIRD_PARTY_NOTICES.md`.

The setup is not code-signed yet, so Windows SmartScreen warns when it is started.

## Languages

The UI is available in English and German (*Settings → Language*). Translations live in
`gui/src/lib/i18n/{en,de}.json`. A test makes sure both files have the same keys and
placeholders. To add a language, copy `en.json`, translate it and register it in
`gui/src/lib/i18n/index.svelte.ts`. The tray menu's texts are in `gui/src-tauri/src/tray.rs`.
