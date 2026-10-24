# Roadmap

State of `main` after version 0.5.0. "⏳ hardware testing" marks features that are
implemented and unit-tested but have not run against the real device yet.

## Road to 1.0

Version 1.0 is about Windows. The features it needs are in place, so what is left is proving
them on real hardware, fixing what that turns up, and settling the formats that 1.0 promises
to keep. Linux stays a preview in 1.0 (see [Platforms](#platforms)).

### Must

- **Windows hardware test**: a checklist with results, like [LINUX-TESTING.md](LINUX-TESTING.md),
  that clears every "⏳ hardware testing" below. First the code that writes to controllers and
  has never run on one: light bars and player lights, DualSense trigger effects and mic LED,
  turning controllers off, Sony rumble over Bluetooth, the desktop mode of the Steam
  Controller and Steam Deck, the GameCube adapter. Then the virtual DualShock 4 (touchpad,
  motion, rumble, light bar), several controllers with their own virtual pads, the Nintendo,
  Steam and generic controllers, the overlays over games, the DSU server in emulators and the
  lock-screen pause.
- **Clean install**: the setup on a fresh Windows 10 and Windows 11 without ViGEmBus or
  HidHide installs both, the app finds the controllers, and uninstalling leaves no controller
  hidden.
- **Stable formats**: profiles, layer presets and settings files already carry a format
  version, older ones are brought up to date as they are read
  ([Format versions](PROFILES.md#format-versions)), and the [local HTTP API](API.md) has a
  version. Left for 1.0: change the names that should not stay forever, then declare the
  profile format, `.dpgame` bundles and the HTTP API stable.
- **Outside testers**: a beta with people on other controllers, games and PCs, and the fixes
  from their reports.
- **Docs**: the README, [GUI.md](GUI.md), [PROFILES.md](PROFILES.md) and the website describe
  the app as it is.

### Should

- **Xbox Elite 2 paddles and Xbox trigger rumble**: XInput offers neither. GameInput reads both
  in the background, but needs its own bindings and a controller to test with.
- **Code signing** for the setup and the programs, so that SmartScreen stops warning.
- **Nintendo face buttons**: they follow their position (Switch B acts as Xbox A). Check that
  on real controllers, and decide whether to offer a layout that follows the labels.
- **Separate keyboards and mice on Windows** through the Interception driver: a third-party
  driver in the setup, with its licence to check first.

### Later

- Linux beyond the preview: virtual DualShock 4, the Steam Controller and the Steam Deck's own
  controls, native Wayland (foreground app, overlays), screenshots, DualSense trigger effects,
  Switch 2 controllers, the GameCube adapter, generic-pad layouts; tests with DualShock 4,
  Joy-Cons, Bluetooth and other distributions.
- macOS (see [Platforms](#platforms)).
- More controllers: Switch 2 over Bluetooth, Wii Remote, DualShock 3, analog keyboards.
- More from imported JSON profiles: crosshair shapes, radial-menu icons, mouse regions, rumble
  mode, virtual gyro stick values.
- Overlays in `doppelpad daemon`, not only in the app.
- A community catalogue of configs, the phone as a controller, adapters for consoles.

## Features

### Mapping

| Area | Status |
|---|---|
| Button remaps, unmaps, gamepad → keyboard/mouse | ✅ |
| Keyboard/mouse → virtual controller | ✅ |
| Scripted input (for example AutoHotkey) as a separate input source | ✅ Windows; Linux passes grabbed input through |
| Combos (up to 4 inputs, 50 ms window, largest wins) | ✅ |
| Activators: single / long / double / triple / start / release | ✅ |
| Zone activators: stick and trigger zones low / med / high (radial or elliptic), gyro zones, mouse speed zones | ✅ |
| Directions: 4-way or 8-way with adjustable diagonal width (sticks, touchpad, mouse) | ✅ |
| Modes: hold / turbo / toggle / once / repeat / while held, optional random intervals | ✅ |
| Macros: pauses, key delay, break, rumble, launch app with arguments, absolute and relative stick items, anti-recoil preset, touchpad gestures on the virtual DualShock 4; recording from keyboard or controller | ✅ |
| Commands: remap off, gyro on/off/toggle, tilt reset, shut down, restart, sleep, hibernate, log off, screenshot, close window, crosshair, radial menu, overlays, turn controller off | ✅ (Linux: no screenshot yet) |
| Shift layers: hold / toggle / switch, inheritance, unlock layers, delayed and postponed jumps; per-layer stick, trigger, virtual stick, light bar and trigger-effect settings | ✅ |
| Stick deadzone, anti-deadzone, curves, rotation, invert | ✅ |
| Trigger deadzone / full pull | ✅ |
| Virtual stick shaping for buttons and keys: ramp (initial deflection, speed, acceleration, maximum), output curve, jitter, motion shake | ✅ |
| Stick → mouse, mouse → stick (spring, noise filter, snap), gyro → mouse/stick, touchpad → mouse (trackball), mouse acceleration, cursor placement and clipping | ✅ |
| Whole stick as mouse, WASD, arrow keys or flick stick in one click; stick → mouse response presets with a graph | ✅ |
| Flick stick, with a test turn to calibrate | ✅ |
| Gyro: motion or tilt mode (lean left/right), roll or yaw steering, player space, automatic and manual calibration | ✅ (⏳ hardware testing) |
| Touchpad: taps, one- and two-finger swipes, pinch zoom, grid zones, split into two trackpads | ✅ (⏳ hardware testing) |
| Keyboard repeat modes (typematic): native, fast, slow, off, custom interval | ✅ |
| Controller hotkeys: slots 1–4, remap on/off, stop macros, emergency stop (hold time, set in the app) | ✅ |
| Slots (4 profiles per controller) | ✅ in the app, by controller hotkey and from the command line |
| Per-game auto-switching (foreground process) | ✅ |
| JSON profile import | ✅ (crosshair shapes become a cross; radial-menu icons, mouse regions, rumble mode and virtual gyro stick values are not imported) |

### Output, overlays and devices

| Area | Status |
|---|---|
| Virtual Xbox 360 / DualShock 4 | ✅ via ViGEmBus, with the game's rumble fed back; DualShock 4 with touchpad, motion and light bar (⏳ hardware testing); Linux: Xbox 360 via uinput, with rumble (tested on a Steam Deck) |
| Multiple controllers / multiple virtual pads | ✅ each controller can have its own slots, config and virtual pad (⏳ hardware testing) |
| Hide physical controller | ✅ via HidHide; Linux: `EVIOCGRAB` (tested on a Steam Deck) |
| Ignore list and controller groups (several pads as one) | ✅ |
| On-screen overlays: notifications (remap, slot, layer, controllers, battery), crosshairs with magnifier, radial menus, controller overlay, mappings sheet | ✅ Windows, in the app (not in `doppelpad daemon`; ⏳ testing in games) |
| Battery levels (DualShock 4, DualSense, Switch controllers, Xbox) in the app, tray and notifications | ✅ Windows (⏳ hardware testing); Linux: Sony and Switch controllers through sysfs (⏳ hardware testing) |
| Light bar colours and animations, the game's light bar colour, player LEDs per slot, DualSense mic LED | ✅ USB and Bluetooth (⏳ hardware testing); Linux: light bar and player LEDs through the driver's LED devices, no mic LED (⏳ hardware testing) |
| DualSense adaptive trigger effects (presets and custom) | ✅ (⏳ hardware testing) |
| Vibration settings (motor strength, swap, minimum gap); rumble for Sony controllers over Bluetooth | ✅ (⏳ hardware testing) |
| Turn wireless controllers off (command and idle timeout) | ✅ Windows (⏳ hardware testing); Linux: Sony and Switch controllers over Bluetooth through BlueZ (⏳ hardware testing) |
| Keep the PC awake while mappings run, pause on the lock screen, optional pause when the game is not in front | ✅ Windows; Linux: keep-awake and the lock-screen pause through logind (⏳ hardware testing) |
| DSU (cemuhook) motion server for emulators | ✅ (⏳ testing with emulators) |
| Local HTTP API with event stream ([API.md](API.md)) | ✅ off by default; localhost only, network access with a token |
| Per-device keyboard/mouse capture | ⏳ Windows needs the Interception driver backend; Linux grabs per device already |
| Virtual Xbox One / DualSense / Switch Pro / DS3 output | ❌ no open driver exists |
| Bluetooth/ESP32/GIMX hardware adapters for consoles | ❌ out of scope for now |
| Mobile remote control app | ❌ out of scope for now |

### Input controllers

| Controller | Status |
|---|---|
| Xbox and XInput controllers | ✅ |
| DualShock 4, DualSense, DualSense Edge | ✅ raw HID |
| Nintendo Switch controllers (Pro Controller, Joy-Con paired or sideways, NSO and Switch-style wired pads; gyro, rumble) | ✅ Windows via raw HID (⏳ hardware testing); Linux through hid-nintendo |
| Switch 2 controllers (Pro Controller, GameCube controller) | ✅ Windows over USB (⏳ hardware testing); ⏳ Bluetooth; ⏳ Linux |
| Steam Controller (USB, wireless receiver) and Steam Deck, with Steam's desktop mode turned off while remapping | ✅ Windows (⏳ hardware testing); ⏳ Bluetooth LE; ⏳ Linux |
| Generic HID gamepads and joysticks, with a wizard that identifies the buttons | ✅ Windows (⏳ hardware testing); Linux reads them through evdev |
| GameCube USB adapter (official and Wii U-mode clones) | ✅ Windows (⏳ hardware testing); ⏳ Linux |
| Xbox Elite 2 paddles, Xbox trigger rumble | ⏳ XInput hides both; needs GameInput and a controller to test |
| Wii Remote, DualShock 3, analog keyboards | ⏳ not started |

### App and installer

| Area | Status |
|---|---|
| Desktop app: editor, tray, 4 slots per controller, autodetect, English/German, analog editors, controller drawings | ✅ |
| First-run guide and config templates (virtual Xbox, virtual DualShock 4, desktop, keyboard/mouse) | ✅ |
| Sharing: games as `.dpgame` bundles, printable bindings sheet, layer presets | ✅ |
| Installer | ✅ Windows setup (NSIS) that installs missing ViGEmBus/HidHide; ⏳ code signing, test on a clean Windows install |
| Updates from GitHub Releases: check at start-up (can be turned off) and *Check now*, signed setups, release workflow for version tags | ✅ (0.4.0 updated itself to 0.5.0) |

## Platforms

1. **Windows**: current focus.
2. **Linux**: implemented in `dp-platform/src/linux`; unit-tested and tested on a Steam
   Deck. Setup and limitations are in [LINUX.md](LINUX.md), the hardware test plan and its
   results in [LINUX-TESTING.md](LINUX-TESTING.md).

   | Responsibility | Status |
   |---|---|
   | Read controllers | ✅ evdev, with DS4/DualSense/Switch motion and touchpad nodes merged, Joy-Cons paired |
   | Hide physical controllers | ✅ `EVIOCGRAB`; hidraw readers such as Steam Input and SDL HIDAPI still see the pad |
   | Virtual Xbox 360 pad with rumble feedback | ✅ uinput (`045e:028e`, xpad layout, `FF_RUMBLE`) |
   | Virtual DualShock 4 | ⏳ needs a `uhid` DS4 emulation |
   | Keyboard/mouse output | ✅ uinput, high-resolution wheel |
   | Keyboard/mouse capture | ✅ per-device grab + uinput passthrough; keyboard LEDs not mirrored yet |
   | Foreground app | ✅ X11 / XWayland; ⏳ native Wayland (compositor-specific protocols) |
   | Power commands, close window | ✅ `systemctl` / `loginctl`, EWMH |
   | Controller extras | ✅ batteries and lights through sysfs, turning off over Bluetooth through BlueZ, keep-awake and the lock-screen pause through logind (tried with a DualSense; Switch controllers not yet) |
   | Not on Linux yet | screenshots, DualSense trigger effects and microphone LED, Steam controllers (the Deck's trackpads, ··· and analog triggers with Steam closed), Switch 2 controllers, generic-pad layouts, GameCube adapter; overlays that keep their place and stay over fullscreen games on Wayland |
   | Hardware testing | ✅ first pass on a Steam Deck (DualSense, Switch Pro Controller, keyboard and mouse, desktop app), second pass on SteamOS 3.9 (controller extras, DualSense over Bluetooth); ⏳ DualShock 4, Joy-Cons, Switch controllers over Bluetooth, Game Mode, other distributions |

   It requires a udev rule for `/dev/uinput` and the input devices.
3. **macOS**:
   - Keyboard/mouse output and capture with `CGEventPost`/`CGEventTap`, which needs
     Accessibility permission.
   - Controller input via GameController.framework.
   - A virtual gamepad needs a signed DriverKit HID extension, which requires an Apple
     entitlement.
