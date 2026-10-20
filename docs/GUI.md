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
| **Toolbar** | Views: *Controller*, *Back side* (the shoulder buttons and the back buttons the connected model has, e.g. GL / GR on the Switch 2 Pro Controller, none on the first one), *Keyboard*, *Mouse*. Layer chips: *Main* plus shift layers, which you can add, rename, copy, paste, clear and remove (together with their analog routes), and whose *Layer settings* say what they inherit from *Main*. On the right: *Analog* (see [Sticks, triggers and analog routes](#sticks-triggers-and-analog-routes)), *Shortcuts* and *Config settings*. |
| **Centre** | The selected view. Click a control to edit it. On the controller views, pressing a button on the controller selects it. On the keyboard view, pressing a key does the same. |
| **Binding panel** (right) | See below. With nothing selected, it lists the mappings of the current layer. |
| **Bottom bar** | Connected controllers (click one to use it), driver warnings, **Remap ON/OFF**, *Apply to slot n* (saves, puts the config in that slot and activates it), *Autodetect* (switch configs by the game in the foreground; only while remapping is on), and the four slots (click to activate, which also turns remapping on; × to unload). |

## Editing a control

1. Pick the layer in the toolbar and click the control (or press it). A stick asks for its
   direction (or the click, or *any direction* for zones). The touchpad asks for its click
   or one of five regions (top, bottom, left, right, centre). Controllers with motion
   sensors have a **Gyro** chip at the top left of the drawing for the four gyro
   directions. A trigger's binding panel offers its zones and *Full pull*.
2. Choose the **activator** from the six icons: single press, long press, double press,
   triple press, start press and release. A dot under an icon means it is already mapped.
3. **Add** actions: keyboard (from the list, or *Press a key…* and press it), mouse
   buttons, wheel and movement, controller buttons, sticks and triggers, or commands
   (turn remapping off, gyro on/off/toggle, screenshot, close the active window, shut
   down, restart, sleep, hibernate, sign out, launch a program, pause, rumble; power
   commands ask once before they are added). Several actions
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
   **Layer settings** (in the layer menu) turn inheriting off for the whole layer, or keep
   single controls of *Main* out of it: an unticked control does nothing on that layer
   unless the layer maps it.

**Shortcuts** are mappings that fire when 2–4 controls are pressed together. Open
*Shortcuts* and choose *New shortcut*. Then click or press the controls, confirm, and edit
the shortcut in the binding panel like any single control.

The **macro editor** opens from the binding panel. It has four modes:

- *Hold until release*: steps stay pressed while the control is held.
- *Execute at once*: runs the steps once.
- *Repeat*: runs N times, with a pause between runs.
- *Repeat while held*: runs again for as long as the control is held.

You can add steps with the pickers, add pauses and rumble, reorder or delete steps, and
choose *down*, *up* or *tap* for keys and buttons. Steps with parameters show them in their
row: stick amount and trigger value in %, wheel clicks, mouse speed, rumble length and the
strength of each motor, a program's path and arguments. In the binding panel, click such
an action to change the same values. **Record** captures keyboard presses together with
the pauses between them; **Record controller** does the same for the buttons, stick
directions and triggers of the controller in use. A **Break** step cuts the macro into
parts: each press plays the next part, then it starts over (turbo, toggle and repeating
are not available for such macros).

**Config settings** sets the output device: the virtual controller (Xbox 360, DualShock 4,
or none for keyboard and mouse only). It also covers hiding the physical controller from games, passing
unmapped controls through, and blocking the original keys of remapped controls. Missing
drivers are shown here and in the bottom bar. A virtual DualShock 4 also passes the
touchpad fingers and the motion sensors through (unless a mapping or analog route uses
them), and the game's rumble and light bar colour come back to the physical controller
(light bar: DualShock 4 and DualSense over USB).

**Held keys repeat** sets how keys that mappings hold down repeat: *Like a keyboard*
(after half a second, about 30 times per second), *Fast*, *Slow*, *Never* or a custom
interval. Only the last pressed key repeats.

Its **Timing** section holds the config-wide defaults: the shortcut window (how long the
controls of a shortcut may take to be pressed together), the long-press time, the double-
and triple-press windows, the turbo period and the delay between keys. A mapping's own
timing wins over these; empty fields use the built-in defaults.

## Sticks, triggers and analog routes

**Analog** in the toolbar opens the analog settings. The controller drawing leads there
too: clicking a stick offers *Stick settings* under the direction picker, and the binding
panel of a stick direction, stick click or trigger links to the matching page. With a
controller connected, the previews follow its sticks and triggers live. The view has four
tabs.

**Left stick** and **Right stick** shape the physical stick for the whole config (every
layer):

- *Deadzone*: below this deflection the output is zero, and the rest is rescaled. 0 passes
  the raw stick through, so the game applies its own deadzone.
- *Full deflection at*: deflection treated as fully pushed (the outer deadzone).
- *Anti-deadzone*: the smallest output once the stick leaves the deadzone, to skip a
  game's own deadzone.
- *Sensitivity*: output multiplier; the output never exceeds full deflection.
- *Rotation* (degrees, counter-clockwise) and *Invert horizontal* / *Invert vertical*.
- *Direction threshold*: deflection at which the stick's directions (`LeftStickUp`, …)
  count as pressed for mappings, which is where the low zone starts (`zones.low`). The
  medium and high zones, and separate horizontal and vertical thresholds, are set under
  *Zone thresholds* in the binding panel once a zone (*Low*, *Medium*, *High*) of a stick
  direction is selected. While the low zone has separate axes, this slider shows the
  horizontal value and is locked.
- *Response curve*: points between the deadzone and full deflection, linear in between.
  Drag a point to move it, double-click the graph to add one (up to eight), double-click a
  point to remove it. A focused point also moves with the arrow keys (Shift for bigger
  steps) and is removed with Delete. Presets: *Linear*, *Precise*, *Very precise*, *Fast*.

The top view shows the deadzone (grey disc), full deflection (dashed ring), the zone
thresholds (dotted; the low one, which is the direction threshold, the most visible), where
pushing the stick up ends up after rotation or inversion
(orange), the stick (ring) and the output (dot). The graph draws the complete response from
stick to output, and a dashed line follows the stick. Inversion and rotation also apply to
analog routes that read the stick; the other settings only shape what the virtual
controller receives, because routes have their own deadzone and curve.

**Triggers** sets, per trigger, the *Deadzone* (pulls below it read as zero, the rest is
rescaled), the *Press point* at which `LT` / `RT` mappings fire (where the low zone starts),
and the *Full pull*, which reads as fully pressed and fires `LTFull` / `RTFull` mappings.
The bars show the live pull with the press point (blue line), the medium and high zone
thresholds (faint lines; set in the zone editor) and the output; *Pressed* and *Full pull*
light up when mappings would fire.

**Routes** lists the analog routes of the current layer. *Add route* offers common ones:
right stick → mouse, gyro → mouse, gyro → right stick while `LT` is held, mouse → right
stick, touchpad → mouse. Each route can then take any source (left or right stick, gyro,
mouse, touchpad) and target (mouse, left or right stick; the mouse cannot drive the mouse).
A route shows only the settings it uses:

| Setting | Used by |
|---|---|
| *Sensitivity*, *Vertical ratio*, *Invert* | every route; the hint under *Sensitivity* says what 1 means for the route |
| *Curve* (exponent on the deflection; 1 = linear) | stick → mouse, mouse → stick, touchpad → stick |
| *Deadzone* | stick → mouse, stick → stick, touchpad → stick |
| *Anti-deadzone* | routes to a stick |
| *Smoothing* | stick → mouse, mouse → stick, gyro → mouse or stick |
| *Recentre after* | mouse → stick |
| *Trackball friction* (100 % = no glide after lift-off) | touchpad → mouse |
| *Keep the stick from also reaching the virtual controller* / *Keep the mouse from also moving the cursor* | stick and mouse sources |

*Only while held* lists controls that must all be held for the route to run, for example
gyro aiming only while `LT` is held. Pick controller buttons, triggers or mouse buttons
from the list, or choose *Press a button or key…* and press one on the controller or the
keyboard.

On a shift layer, Main's routes keep working for every source the layer does not route
itself (unless the layer does not inherit). They are listed below the layer's own routes,
and *Change on this layer* copies one onto the layer, where it replaces Main's.

The settings are stored as `[sticks.left]`, `[sticks.right]`, `[triggers.left]`,
`[triggers.right]` and `[[analog]]` in the profile; see [PROFILES.md](PROFILES.md).

## Remapping while the window is open

Remapping pauses while the Doppelpad window has keyboard focus. Mapped keys therefore
don't land in the editor, and pressing a button selects it instead of triggering its
mapping. The bottom bar then shows *Paused while editing*. Switch to the game (or hide
the window) and the active slot takes over again. *Settings → Keep remapping active while
this window is focused* turns this off.

## Controller hotkeys

*Settings → Controller hotkeys* sets button combos that work in every config, with
remapping off, and without the window (`doppelpad daemon` reads them too): **Slot 1–4**
(activates that slot and turns remapping on; empty slots do nothing), **Remapping on/off**,
**Stop macros** (stops running macros, turbo and toggles and releases what they hold) and
**Emergency stop** (the same, then remapping off). Hold all buttons of a combo for the
*hold time* (500 ms by default, 50–2500 ms). Defaults: LB + RB + X / Y / B / A for slots
1–4, Back + Start + LB + RB for the emergency stop; the other two are off.

To record a combo, click the record button, hold the buttons on the controller and let go;
× clears it. Hotkeys are paused while recording. As soon as all buttons of a hotkey are
down, the game no longer sees them (each stays hidden until it is released); a combo you
need in a game must therefore not be a hotkey. A combo inside a larger one (LB + RB inside
LB + RB + X) waits for the larger one.

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

*Settings → Start with Windows, in the tray* (Windows only) starts Doppelpad when you sign
in, with only the tray icon showing; remapping runs as it was left. The setting is the
`Doppelpad` entry under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, which runs
`doppelpad-gui.exe --hidden`. Task Manager's *Startup* tab lists it too, and switching it
off there shows in the settings. If reWASD runs at sign-in and *When reWASD is running at
start-up* is set to *Ask*, the window opens to ask.

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
| `%APPDATA%\doppelpad\hotkeys.toml` | Controller hotkeys and their hold time (also read by `doppelpad daemon`) |
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
tests. They check that the frontend's key table, JSON shapes and default values match the
Rust model, using the fixtures in `gui/src/lib/model/fixtures`. Regenerate them with
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
(profiles and settings) and `%LOCALAPPDATA%\doppelpad\logs`. Uninstalling always removes
the *Start with Windows* entry of the user who uninstalls; updates keep it.

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
