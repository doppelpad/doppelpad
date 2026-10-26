# Roadmap

State of version 0.6.0. "⏳ hardware testing" marks features that are implemented and
unit-tested but have not run against the real device yet.

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
- **Separate keyboards and mice on Windows** through the Interception driver: a third-party
  driver in the setup, with its licence to check first.

### Later

- Linux beyond the preview: virtual DualShock 4, the Steam Controller and the Steam Deck's own
  controls, native Wayland (the foreground app; overlays on GNOME), screenshots, DualSense
  trigger effects, the GameCube adapter, generic-pad layouts;
  tests with DualShock 4, Joy-Cons, Bluetooth and other distributions.
- macOS beyond the preview: tests on real hardware (keyboard and mouse, overlays over games,
  Bluetooth and other controllers), signing and notarization, release builds, virtual
  controllers (an entitlement from Apple), the GameCube adapter (see [Platforms](#platforms)).
- More controllers: the Switch 2 GameCube controller and Joy-Con 2 over Bluetooth, Wii Remote,
  DualShock 3, analog keyboards. Switch 2 controllers speak Bluetooth Low Energy with
  Nintendo's own protocol, not HID: no system pairs them as a controller, so Doppelpad
  connects them itself (done for the Pro Controller with CoreBluetooth on macOS, WinRT on
  Windows and an ATT socket of its own on Linux). The pads never ask for a faster connection,
  so the host's default sets the input rate: about 33 Hz on macOS; on Windows 10 unevenly, at
  times a steady 17 Hz, at times 15–105 reports a second (about 45 on average; measured, what
  switches between the two is not known); on Linux with BlueZ's defaults about 20 Hz, 17–19
  after the first 20 s (measured on a Steam Deck). Doppelpad asks Windows 11 for a faster one.
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
| Battery levels (DualShock 4, DualSense, Switch and Switch 2 controllers, Xbox) in the app, tray and notifications | ✅ Windows (⏳ hardware testing); Linux: Sony and Switch controllers through sysfs (tested with a DualSense); macOS: through the drivers (a Switch 2 Pro Controller's shown in the app) |
| Light bar colours and animations, the game's light bar colour, player LEDs per slot, DualSense mic LED | ✅ USB and Bluetooth (⏳ hardware testing); Linux: light bar and player LEDs through the driver's LED devices, no mic LED (tested with a DualSense) |
| DualSense adaptive trigger effects (presets and custom) | ✅ (⏳ hardware testing) |
| Vibration settings (motor strength, swap, minimum gap); rumble for Sony controllers over Bluetooth | ✅ (⏳ hardware testing) |
| Turn wireless controllers off (command and idle timeout) | ✅ Windows (⏳ hardware testing); Linux: Sony and Switch controllers over Bluetooth through BlueZ (tested with a DualSense), the Switch 2 Pro Controller over Bluetooth by ending its link (tried) |
| Keep the PC awake while mappings run, pause on the lock screen, optional pause when the game is not in front | ✅ Windows; Linux: keep-awake and the lock-screen pause through logind (tested on a Steam Deck) |
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
| Switch 2 controllers (Pro Controller, GameCube controller) | ✅ Windows over USB (⏳ hardware testing), and the Pro Controller over Bluetooth LE (tried on Windows 10: connecting, every button and trigger, both sticks, motion, battery, player light, rumble, letting it go 3 s after it was turned off, where Windows notices only after about 10 s; ⏳ Windows 11); macOS over USB (tried with a Pro Controller), and the Pro Controller over Bluetooth LE (tried in the app); Linux: the Pro Controller over Bluetooth LE (tried on a Steam Deck with the command-line tool and the app: connecting, every button and trigger, both sticks, motion, battery, player light, rumble, turning it off, letting it go when turned off), and on USB (tried with the command-line tool: every button and trigger, both sticks, motion, battery, player light, rumble, 250 reports a second, hiding, unplugging) |
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
| Installer | ✅ Windows setup (NSIS) that installs missing ViGEmBus/HidHide; ⏳ code signing, test on a clean Windows install. Linux: an AppImage, and the command-line tool as a `.tar.gz` (the AppImage tried on a Steam Deck) |
| Updates from GitHub Releases: check at start-up (can be turned off) and *Check now*, signed setups, release workflow for version tags | ✅ (0.4.0 updated itself to 0.5.0); Linux: the AppImage updates itself (⏳ from the first release with an AppImage to the next) |

## Platforms

1. **Windows**: current focus.
2. **Linux**: implemented in `dp-platform/src/linux`; unit-tested and tested on a Steam
   Deck. Setup and limitations are in [LINUX.md](LINUX.md), the hardware test plan and its
   results in [LINUX-TESTING.md](LINUX-TESTING.md).

   | Responsibility | Status |
   |---|---|
   | Read controllers | ✅ evdev, with DS4/DualSense/Switch motion and touchpad nodes merged, Joy-Cons paired; Switch 2 controllers on USB through hidraw, started over their USB command interface, and over Bluetooth on an ATT socket of Doppelpad's own, found by a BlueZ discovery |
   | Hide physical controllers | ✅ `EVIOCGRAB`; hidraw readers such as Steam Input and SDL HIDAPI still see the pad |
   | Virtual Xbox 360 pad with rumble feedback | ✅ uinput (`045e:028e`, xpad layout, `FF_RUMBLE`) |
   | Virtual DualShock 4 | ⏳ needs a `uhid` DS4 emulation |
   | Keyboard/mouse output | ✅ uinput, high-resolution wheel |
   | Keyboard/mouse capture | ✅ per-device grab + uinput passthrough; keyboard LEDs not mirrored yet |
   | Foreground app | ✅ X11 / XWayland; ⏳ native Wayland (compositor-specific protocols) |
   | Power commands, close window | ✅ `systemctl` / `loginctl`, EWMH |
   | Controller extras | ✅ batteries and lights through sysfs, turning off over Bluetooth through BlueZ, keep-awake and the lock-screen pause through logind (tried with a DualSense; Switch controllers not yet) |
   | Download | ✅ the app as an AppImage that updates itself, built on Ubuntu 22.04; the command-line tool as a `.tar.gz` (the AppImage tried on a Steam Deck) |
   | Not on Linux yet | screenshots, DualSense trigger effects and microphone LED, Steam controllers (the Deck's trackpads, ··· and analog triggers with Steam closed), generic-pad layouts, GameCube adapter; overlays that keep their place on GNOME's Wayland (it has no layer shell) |
   | Hardware testing | ✅ first pass on a Steam Deck (DualSense, Switch Pro Controller, keyboard and mouse, desktop app), second pass on SteamOS 3.9 (controller extras, DualSense over Bluetooth), a Switch 2 Pro Controller over Bluetooth with the command-line tool and the app; ⏳ DualShock 4, Joy-Cons, Switch controllers over Bluetooth, Game Mode, other distributions |

   It requires a udev rule for `/dev/uinput` and the input devices, except on SteamOS.
3. **macOS**: implemented in `dp-platform/src/macos`; unit-tested and tried with a DualSense,
   a Switch Pro Controller and a Switch 2 Pro Controller on USB. Setup and limitations are in [MACOS.md](MACOS.md).

   | Responsibility | Status |
   |---|---|
   | Read controllers | ✅ IOKit HID with the Windows backend's drivers (Sony, Nintendo, Steam, generic pads through their report descriptor), Switch 2 controllers started over their USB command channel; Apple's stand-in Xbox 360 pads left out (tried with a DualSense, a Switch Pro Controller and a Switch 2 Pro Controller on USB, and both Pro Controllers over Bluetooth) |
   | Hide physical controllers | ✅ seizing the device, which keeps it from the GameController framework too; for every config that asks, as there is no virtual controller (tried with a DualSense and a Switch Pro Controller) |
   | Virtual controller | ❌ needs Apple's entitlement for virtual HID devices; the GameController framework may still ignore one |
   | Keyboard/mouse output | ✅ Quartz events, media keys as system events (tried: motion, clicks, wheel, keys, media keys) |
   | Keyboard/mouse capture | ✅ event tap (tried: a key remapped) |
   | Foreground app | ✅ the window server's window list, Wine games by their `.exe` (tried); installed Steam and Epic games suggested |
   | Power commands, close window, screenshots | ✅ `loginwindow`, `pmset`, the accessibility API, `screencapture` (tried: sleep, close window, screenshots; ⏳ shut down, restart, log out); no hibernate |
   | Controller extras | ✅ batteries, lights, trigger effects and rumble through the drivers (tried: battery, DualSense light bar, player lights and rumble of Switch and Switch 2 controllers); keep-awake and the lock-screen pause (tried); ⏳ turning Sony pads off over Bluetooth |
   | Desktop app | ✅ menu bar, overlays over full-screen programs without taking the focus, as non-activating panels (tried: notifications over another program, crosshair and controller overlay over a video in full screen; ⏳ testing in games), start at login, `.dpgame` from the Finder, permissions in the driver list and welcome guide |
   | Download | ✅ every release after 0.6.0 has a universal `.dmg` that updates itself and the command-line tool, signed with a self-signed certificate so that the permissions should stay across updates (built and signed on GitHub once); ⏳ notarization, without which Gatekeeper asks before the first start, needs an Apple Developer ID |
   | Not on macOS yet | virtual controllers, the GameCube adapter, Xbox controllers over USB, the crosshair magnifier, cursor clipping |
