# Doppelpad

**Free, open-source controller remapping for Windows, with Linux in preview and macOS planned, written in Rust.**

Doppelpad turns any supported controller, keyboard or mouse into whatever a game expects:
remap buttons, build combos and macros, add shift layers, turbo, toggles, long/double/triple
presses, use your keyboard and mouse as an Xbox controller, aim with gyro, or drive the
desktop with a gamepad.

Doppelpad uses only open drivers ([ViGEmBus](https://github.com/nefarius/ViGEmBus) and
[HidHide](https://github.com/nefarius/HidHide)) and can import JSON profiles (schema 3.0)
from other remapping tools.

> **Status: early preview (0.2).** The engine, profile format, importer and Windows
> backends are implemented and unit-tested, and real DualSense input has been verified.
> The desktop app (editor + tray service) is new and still being polished. End-to-end
> testing with ViGEmBus/HidHide in games is the next milestone. A Linux backend
> (evdev/uinput) is implemented but not yet tested on real hardware. Expect rough edges.

## Features

| | |
|---|---|
| **Inputs** | Xbox controllers (XInput), DualShock 4 and DualSense incl. gyro, touchpad and Edge paddles (USB/Bluetooth), Nintendo Switch Pro Controller, Joy-Con (paired automatically, or sideways on their own), Nintendo Switch Online and Switch-style wired pads incl. gyro (USB/Bluetooth), Switch 2 Pro Controller and GameCube controller (USB), keyboard, mouse buttons, wheel, mouse motion |
| **Outputs** | Virtual Xbox 360 or DualShock 4 controller, keyboard, mouse buttons/motion/wheel, controller rumble |
| **Triggers** | Any button, stick direction, trigger pull/full pull, touchpad region, gyro direction, key, mouse button, wheel — alone or as combos of up to 4 |
| **Activators** | single, long press, double tap, triple tap, start (on press), release |
| **Modes** | hold, turbo (rapid fire), toggle, run-once macro, repeat N times, repeat while held |
| **Macros** | key/button/mouse/stick/trigger steps, pauses, wheel, rumble, launch program |
| **Layers** | shift layers (hold or toggle) with inheritance from the base layer |
| **Analog** | stick deadzones, anti-deadzone, response curves, rotation, inversion; stick → mouse, mouse → stick, gyro → mouse/stick, touchpad → mouse (with trackball glide) |
| **Profiles** | readable TOML, per-game auto-switching by foreground app, JSON profile import |
| **Safety** | hold Back + Start + LB + RB to stop every macro and release all outputs |

## Requirements (Windows)

- Windows 10/11 (x64)
- [ViGEmBus](https://github.com/nefarius/ViGEmBus/releases) — needed for virtual controllers
- [HidHide](https://github.com/nefarius/HidHide/releases) — optional, hides the physical controller so games don't see double input

The Windows setup installs both when they are missing (see
[docs/GUI.md](docs/GUI.md#installer)). Run `doppelpad doctor` to check both and list
detected controllers.

Switch 2 controllers connect over USB only for now. Doppelpad talks to their USB command
interface through WinUSB, which Windows normally sets up by itself. Only one program can
use it at a time. While Steam holds it, the controller works with default stick
calibration. Close Steam and Doppelpad takes the interface over within a few seconds,
without reconnecting the controller.

reWASD keeps controllers to itself while it runs. The app offers to quit it at start-up
(see [docs/GUI.md](docs/GUI.md#rewasd-running-at-start-up)).

## Build

```bash
cargo build --release
```

The binary is `target/release/doppelpad.exe`. Both the MSVC and the GNU toolchain work. With
`x86_64-pc-windows-gnu`, a full MinGW-w64 toolchain (for example WinLibs) must be on `PATH`,
because `windows-sys` needs `dlltool`/`as` to create import libraries.

## Desktop app

`doppelpad-gui` is a graphical editor and tray service. It has a game list, a controller,
keyboard and mouse view, shift layers, a binding panel with six activators, turbo/toggle,
shortcuts, a macro recorder, four config slots and per-game autodetect. Remapping keeps
running in the tray when the window is closed. The UI is in English and German.

```bash
cd gui && npm install && npx tauri build --no-bundle
target/release/doppelpad-gui.exe
```

See [docs/GUI.md](docs/GUI.md).

## Quick start

```bash
# Check drivers and controllers
doppelpad doctor

# Show live controller input (button names, stick values, gyro, touchpad)
doppelpad monitor

# Run a profile in the foreground (Ctrl+C restores everything)
doppelpad run profiles/examples/keyboard-mouse-to-xbox.toml

# Convert a JSON profile from another remapper
doppelpad import my-game.json -o my-game.toml

# Validate a profile
doppelpad check my-game.toml
```

### Background daemon and per-game profiles

```bash
# Profiles in %APPDATA%\doppelpad\profiles with `processes = ["game.exe"]`
# are applied automatically while that game has focus.
doppelpad daemon --profile profiles/examples/controller-desktop.toml

doppelpad status
doppelpad apply my-game.toml
doppelpad auto-switch off
doppelpad stop
doppelpad shutdown
```

## Profiles

Profiles are TOML. A minimal one:

```toml
name = "My game"
processes = ["MyGame.exe"]

[[mappings]]
trigger = "A"                     # gamepad A / Cross
actions = [{ key = "Space" }]

[[mappings]]
trigger = ["LB", "RB"]            # combo
activator = "long"
mode = "once"
actions = [{ key = "Escape", action = "tap" }, { pause = 50 }, { pad = "Start" }]

[[analog]]
source = "right_stick"
target = "mouse"
sensitivity = 1.2
```

The full reference is in [docs/PROFILES.md](docs/PROFILES.md), and there are commented
examples in [`profiles/examples/`](profiles/examples).

## Platform support

| | Windows | Linux | macOS |
|---|---|---|---|
| Engine, profiles, CLI | ✅ | ✅ | ✅ (compiles) |
| Read controllers | ✅ XInput, raw HID (Sony, Nintendo) | 🧪 evdev (incl. DS4/DualSense/Switch gyro + touchpad) | planned |
| Virtual controller | ✅ Xbox 360, DualShock 4 (ViGEmBus) | 🧪 Xbox 360 (uinput) | — |
| Hide physical controller | ✅ HidHide | 🧪 `EVIOCGRAB` | planned |
| Keyboard/mouse output | ✅ `SendInput` | 🧪 uinput | planned |
| Keyboard/mouse capture | ✅ hooks | 🧪 evdev grab + passthrough | planned |
| Per-game auto-switching | ✅ | 🧪 X11 / XWayland | planned |

🧪 = implemented and unit-tested, not yet tested on real hardware. Linux setup (udev rule for
`/dev/uinput` and `/dev/input`) is in [docs/LINUX.md](docs/LINUX.md).

Everything except `crates/dp-platform` is platform-independent. See
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/ROADMAP.md](docs/ROADMAP.md).

## Project layout

```text
crates/dp-core       input model, profile model, remapping engine (no OS code)
crates/dp-config     TOML loading/validation, JSON profile import
crates/dp-platform   OS backends behind traits (Windows: XInput, HID, ViGEm, HidHide, SendInput, hooks;
                     Linux: evdev, uinput, EVIOCGRAB, X11)
crates/dp-ipc        daemon control protocol (local socket, JSON lines)
crates/dp-service    remapping service shared by daemon and app (slots, auto-switch, live state)
crates/doppelpad      the `doppelpad` CLI / daemon
gui                  desktop app (Tauri 2 + Svelte), crate `dp-gui` in gui/src-tauri
docs                 architecture, profile reference, desktop app, roadmap, Linux setup
profiles/examples    example profiles
```

## Contributing

Contributions are welcome. Do not submit code, assets or text taken from proprietary
software. Base changes on public documentation, open-source drivers and your own
observations.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your
option.

Doppelpad is an independent project. All trademarks belong to their respective owners.
