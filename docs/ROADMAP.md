# Roadmap

## Features

| Area | Status |
|---|---|
| Button remaps, unmaps, gamepad → keyboard/mouse | ✅ |
| Keyboard/mouse → virtual controller | ✅ |
| Combos (up to 4 inputs, 50 ms window, largest wins) | ✅ |
| Activators: single / long / double / triple / start / release | ✅ |
| Zone activators: stick (any direction or per direction) and trigger zones low / med / high, radial or elliptic | ✅ |
| Modes: hold / turbo / toggle / once / repeat / while held | ✅ |
| Macros with pauses, key delay, rumble, launch app | ✅ |
| Shift layers (hold / toggle / switch) with inheritance | ✅ |
| Stick deadzone, anti-deadzone, curves, rotation, invert | ✅ |
| Trigger deadzone / full pull | ✅ |
| Stick → mouse, mouse → stick, gyro → mouse/stick, touchpad → mouse (trackball) | ✅ |
| Per-game auto-switching (foreground process) | ✅ |
| JSON profile import | ✅ (radial menus, crosshairs, right trackpad, flick stick not yet) |
| Emergency stop combo | ✅ |
| Virtual Xbox 360 / DualShock 4 | ✅ via ViGEmBus, with the game's rumble fed back; DualShock 4 with touchpad, motion and light bar (⏳ hardware testing); Linux: Xbox 360 via uinput (untested on hardware) |
| Hide physical controller | ✅ via HidHide; Linux: `EVIOCGRAB` (untested on hardware) |
| Desktop app (editor, tray, 4 slots, autodetect, English/German, stick/trigger/curve editor, analog and gyro routes) | ✅ (controller drawings ⏳) |
| Installer | ✅ Windows setup (NSIS) that installs missing ViGEmBus/HidHide; ⏳ code signing, test on a clean Windows install |
| Radial menu overlay, crosshair overlay | ⏳ planned |
| Flick stick, tilt mode | ⏳ planned |
| Zones for gyro, mouse-as-stick and pressure buttons; separate diagonal directions | ⏳ planned |
| Digital → stick ramp (initial deflection, speed, acceleration) | ⏳ planned |
| Slots (4 profiles per controller) | ✅ in the app; ⏳ slot hotkeys |
| Multiple controllers / multiple virtual pads | ⏳ planned (first controller only) |
| Keyboard repeat modes (typematic) | ⏳ planned |
| Nintendo Switch controllers as input (Pro Controller, Joy-Con paired or sideways, NSO and Switch-style wired pads; gyro, rumble) | ✅ Windows via raw HID (⏳ hardware testing); Linux through hid-nintendo |
| Switch 2 controllers as input (Pro Controller, GameCube controller) | ✅ Windows over USB (⏳ hardware testing); ⏳ Bluetooth; ⏳ Linux |
| Generic DirectInput controllers as input | ⏳ planned on Windows; Linux reads them through evdev |
| Per-device keyboard/mouse capture | ⏳ Windows needs the Interception driver backend; Linux grabs per device already |
| Virtual Xbox One / DualSense / Switch Pro / DS3 output | ❌ no open driver exists |
| Bluetooth/ESP32/GIMX hardware adapters for consoles | ❌ out of scope for now |
| Mobile remote control app | ❌ out of scope for now |

## Platforms

1. **Windows**: current focus.
2. **Linux**: implemented in `dp-platform/src/linux`; compile-checked and unit-tested, not
   yet tested on real hardware. Setup and limitations are in [LINUX.md](LINUX.md).

   | Responsibility | Status |
   |---|---|
   | Read controllers | ✅ evdev, with DS4/DualSense/Switch motion and touchpad nodes merged, Joy-Cons paired |
   | Hide physical controllers | ✅ `EVIOCGRAB`; hidraw readers such as SDL HIDAPI still see the pad |
   | Virtual Xbox 360 pad with rumble feedback | ✅ uinput (`045e:028e`, xpad layout, `FF_RUMBLE`) |
   | Virtual DualShock 4 | ⏳ needs a `uhid` DS4 emulation |
   | Keyboard/mouse output | ✅ uinput, high-resolution wheel |
   | Keyboard/mouse capture | ✅ per-device grab + uinput passthrough; keyboard LEDs not mirrored yet |
   | Foreground app | ✅ X11 / XWayland; ⏳ native Wayland (compositor-specific protocols) |
   | Hardware testing | ⏳ needed |

   It requires a udev rule for `/dev/uinput` and the input devices.
3. **macOS**:
   - Keyboard/mouse output and capture with `CGEventPost`/`CGEventTap`, which needs
     Accessibility permission.
   - Controller input via GameController.framework.
   - A virtual gamepad needs a signed DriverKit HID extension, which requires an Apple
     entitlement.
