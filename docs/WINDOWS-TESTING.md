# Windows hardware test checklist

Version 1.0 is about Windows ([ROADMAP.md](ROADMAP.md#road-to-10)). Its features are
implemented and unit-tested, but many of them have never run against a real device: the
roadmap marks those "⏳ hardware testing". This checklist turns each of them into steps with
the expected result. Results go in the [tables at the end](#results).

A person with the hardware can work through it alone, or together with a coding agent (such
as Claude Code) running on the test machine. In that case the agent runs the commands, reads
logs and fixes bugs, and the person presses buttons, watches the screen and plays the games.

## What it covers

| Roadmap entry marked ⏳ | Section |
|---|---|
| Light bar colours and animations, player LEDs per slot, DualSense mic LED | [2](#2-lights) |
| The game's light bar colour | [8](#8-virtual-dualshock-4) |
| DualSense adaptive trigger effects | [3](#3-dualsense-trigger-effects) |
| Vibration settings; rumble for Sony controllers over Bluetooth | [4](#4-rumble) |
| Turn wireless controllers off (command and idle timeout) | [5](#5-turning-controllers-off) |
| Steam Controller and Steam Deck, with Steam's desktop mode turned off while remapping | [6](#6-steam-controller-and-steam-deck) |
| GameCube USB adapter | [7](#7-gamecube-adapter) |
| Virtual DualShock 4 with touchpad, motion and light bar | [8](#8-virtual-dualshock-4) |
| Multiple controllers, multiple virtual pads | [9](#9-several-controllers) |
| Nintendo Switch controllers; Switch 2 controllers over USB; the Switch 2 Pro Controller over Bluetooth on Windows 11 | [10](#10-nintendo-controllers) |
| Generic HID gamepads and joysticks | [11](#11-generic-pads-and-joysticks) |
| Gyro; touchpad | [12](#12-gyro-and-touchpad) |
| Battery levels | [1](#1-basics) |
| On-screen overlays, tested in games | [13](#13-overlays-over-games) |
| DSU motion server, tested with emulators | [14](#14-dsu-motion-server) |
| The lock-screen pause (named in the "Must" list) | [15](#15-lock-screen-and-keeping-awake) |
| Installer on a clean Windows install ("Clean install" in the "Must" list) | [16](#16-clean-install) |

Not covered, because Windows has no code for them yet: Xbox Elite 2 paddles and Xbox trigger
rumble (they need GameInput), separate keyboards and mice (the Interception driver), the
Steam Controller over Bluetooth LE, and code signing.

### Which sections need which hardware

Pick the sections for the controllers at hand. [Setup](#0-setup) and [Basics](#1-basics)
come first in any case.

| Section | Needs |
|---|---|
| 1 Basics | any controller |
| 2 Lights | DualShock 4 or DualSense (light bar), DualSense or a Switch or Switch 2 controller (player lights), DualSense (mic LED); each over USB and Bluetooth |
| 3 Trigger effects | DualSense or DualSense Edge, over USB and Bluetooth |
| 4 Rumble | DualShock 4 or DualSense over Bluetooth, and over USB to compare; any controller with rumble for the vibration settings |
| 5 Turning off | DualShock 4, DualSense or Switch controller over Bluetooth; an Xbox controller over its wireless adapter or Bluetooth; one controller on a cable |
| 6 Steam | Steam Controller (cable or wireless receiver), Steam Deck with Windows |
| 7 GameCube adapter | the official adapter or a clone with a Wii U mode, GameCube controllers |
| 8 Virtual DualShock 4 | any controller; a DualShock 4 or DualSense for the touchpad, motion and light bar |
| 9 Several controllers | two controllers; two Xbox controllers for W9.5 |
| 10 Nintendo | Switch Pro Controller, Joy-Cons, Nintendo Switch Online pads, wired Switch pads, Switch 2 Pro Controller or GameCube controller; a Windows 11 PC with Bluetooth LE for W10.7–W10.10 |
| 11 Generic pads | a gamepad or joystick without a driver of its own (DirectInput) |
| 12 Gyro and touchpad | a controller with motion sensors, a controller with a touchpad |
| 13 Overlays | any controller, games that run windowed, borderless and in exclusive full screen |
| 14 DSU | a controller with motion sensors; Cemu, Dolphin or Ryujinx |
| 15 Lock screen | any controller |
| 16 Clean install | a fresh Windows 10 and a fresh Windows 11 (virtual machines will do), a few controllers |

## How to work through it

- Work on a branch (for example `windows-hardware-test`). Fixes are reviewed and merged from
  there.
- Go through [Setup](#0-setup) and [Basics](#1-basics) first. After that, sections 2 to 7
  matter most: they hold the code that writes to controllers and has never run on one.
- For each check, run it and have the person do the physical part. Then compare with
  **Expected** and fill in its row under [Results](#results): ✅ works, ⚠️ partly, ❌ fails,
  ➖ skipped (say why). Name the controller, how it was connected (USB, Bluetooth, wireless
  adapter) and the Windows version.
- When a check fails, keep the evidence:
  - the app's log with debug detail (see [Setup](#0-setup)), or the output of
    `doppelpad --log info,dp_platform=debug run …`;
  - what `doppelpad monitor` shows;
  - what `joy.cpl` shows (*Set up USB game controllers*: the controllers games see).
- Fix clear, local bugs right away, one commit per fix. If the bug is in code that every OS
  builds (`crates/dp-platform/src/sony`, `nintendo`, `steam.rs`, `generic.rs`, `hidhost.rs`,
  or `dp-service`), add a unit test there, so CI keeps the fix.
- Don't settle design questions alone, such as the [known issues](#known-issues) below.
  Write down what you saw and the options in the check's notes, and leave the decision to
  the maintainer.
- Before every commit, run what CI runs on Windows, and fix new warnings:

  ```bash
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets
  cargo test --workspace
  ```

  After changes to the app, also `cargo test -p dp-gui`, and `npm test`, `npm run check` and
  `npm run format:check` in `gui/`. With the GNU toolchain a MinGW-w64 toolchain (for example
  WinLibs) must be on `PATH` ([README](../README.md#build)).
- Base fixes only on public sources: open-source drivers and published protocol
  descriptions.
- House style:
  - Commit subjects are short imperative sentences, for example "Turn the Steam Controller
    off through its receiver".
  - Comments and docs are plain English.
  - A change to what a settings or profile file holds needs a format version and a
    migration ([Format versions](PROFILES.md#format-versions)).
- When you are done, update whatever the results confirm: the ⏳ marks in
  [ROADMAP.md](ROADMAP.md), the README's platform table, and [GUI.md](GUI.md) and
  [PROFILES.md](PROFILES.md) where they say something else.

> **Getting controllers back.** Hiding goes through HidHide, which keeps its lists across
> restarts, so a bug can leave a controller hidden from games. Quitting from the tray icon,
> or Ctrl+C for `doppelpad run`, shows hidden controllers again. If Doppelpad was killed or
> crashed, its next start does it (from the journal in
> `%LOCALAPPDATA%\doppelpad\hidhide-restore`). As a last resort,
> `"C:\Program Files\Nefarius Software Solutions\HidHide\x64\HidHideCLI.exe" --dev-list`
> lists what is hidden, and `--dev-unhide "<instance id>"` shows a device again. The
> emergency stop hotkey (Back + Start + LB + RB, held half a second) stops macros and turns
> remapping off. Virtual controllers go away when the process ends.

## 0. Setup

### Machines

- A PC with Windows 10 22H2 and one with Windows 11, or one PC with both. Most sections run
  on either; record which. W10.7 to W10.9 need Windows 11, and W10.7 to W10.10 a Bluetooth
  LE adapter.
- [Section 16](#16-clean-install) needs a fresh install of each, without ViGEmBus or HidHide.
  Virtual machines will do; controllers then need USB passthrough. Do section 16 first on
  such a machine: it installs Doppelpad.

### Install

1. Install Doppelpad with the setup under test: `Doppelpad_<version>_x64-setup.exe` from the
   GitHub release, or one built from the branch (`npx tauri build` in `gui/`, see
   [GUI.md § Installer](GUI.md#installer); without the updater key, add
   `--config '{"bundle":{"createUpdaterArtifacts":false}}'`). It installs ViGEmBus and
   HidHide when they are missing. Restart Windows when it asks.
2. The setup puts the command-line tool at `C:\Program Files\Doppelpad\doppelpad.exe`, which
   is not on `PATH`. In PowerShell, `$env:Path += ';C:\Program Files\Doppelpad'` adds it for
   that window; the checks call it `doppelpad`. To test a fix, build it as in the
   [README](../README.md#build) and use `target\release\doppelpad.exe`.
3. Close the programs that take controllers for themselves:
   - reWASD: the app offers to quit it at start-up, and `doppelpad doctor` reports it;
   - DS4Windows, BetterJoy;
   - Dolphin, which holds the GameCube adapter;
   - Steam: it opens Sony, Nintendo and Valve controllers itself. Quit it (Steam menu →
     Exit) unless a check says otherwise.

   While one of them holds a controller, the app's controller list says so, for example
   "Steam is using the DualSense".
4. Run `doppelpad doctor` once and keep its output for [W1.1](#1-basics).
5. Make the test configs a game in the app's library, so that the app and the command line
   use the same files. Create the folder `%APPDATA%\doppelpad\profiles\windows-test\` (or
   `windows-test` in the folder that *Settings → Files* names, if the configs were moved),
   with a file `game.toml` that holds the line `name = "Windows test"`. The sections below
   give each config's file name and contents; put them there.
   - In the app they appear under the game "Windows test", to open, change and apply to a
     slot.
   - For the command line, work in that folder, so the checks can name a config by its file
     name: `cd $env:APPDATA\doppelpad\profiles\windows-test` in PowerShell.
   - `doppelpad check <file>` finds typos.
6. In the app, tick *Settings → General → Keep remapping active while this window is
   focused*. Otherwise remapping pauses whenever the Doppelpad window is in front.
7. For debug detail in the app's log, quit the app from its tray icon and start it from
   PowerShell:

   ```powershell
   $env:DOPPELPAD_LOG = 'debug'; & 'C:\Program Files\Doppelpad\doppelpad-gui.exe'
   ```

   The log is `%LOCALAPPDATA%\doppelpad\logs\doppelpad-gui.log` (*Settings → Files → Open log
   folder*). Each start begins a new one.
8. Useful tools:
   - `joy.cpl`: the controllers that games see, and which of them move.
   - A text editor such as Notepad, to see the keys mappings type.
   - A gamepad test page in Chrome or Edge that has a vibration test (the Gamepad API), to
     send rumble to a virtual Xbox 360 controller like a game does.
   - Steam's controller settings, which show the touchpad and gyro of a PlayStation
     controller, also of a virtual one (optional).

**The app or the command line, not both.** `doppelpad run` and `doppelpad monitor` read the
controllers and make virtual ones themselves. Don't run them while the app runs: quit the app
from its tray icon first. While the app runs, `doppelpad apply <file> --slot 1` puts a config
into slot 1 and runs it, as *Apply to slot 1* does, and `doppelpad status` and
`doppelpad devices` ask the app.

Before you start, fill in [Test machine](#test-machine).

## 1. Basics

**W1.1 Doctor.** This uses the `doctor` output from setup step 4.
*Expected:*
- The first line names the Doppelpad version and the Windows version.
- `ViGEmBus (virtual controllers)` and `HidHide (hide physical pads)` say `ok`.
- `Bluetooth (Switch 2 pads)` says `ok` when a Bluetooth LE adapter is on, otherwise why not.
- The settings directory is `%APPDATA%\doppelpad`.
- There is no reWASD line and no `problem:` line, and the connected controllers are listed.

**W1.2 Detection.** Do W1.2 to W1.5 for every controller you have and record them in the
[Controllers](#controllers) table. Run `doppelpad devices` with the app closed, then look at
the app's controller list in the bottom bar and at *Settings → Controllers*.
*Expected:*
- Each controller appears once, with an id that says how it is read:
  - `xinput:<slot>` for Xbox controllers;
  - `hid:<instance id>` for Sony, Nintendo, Steam and generic pads;
  - `gcadapter:<instance>:<port>` for the GameCube adapter, one per occupied port;
  - `ble:057e:2069:<address>` for a Switch 2 Pro Controller over Bluetooth.
- The app names the connection after the controller's name: (USB), (Bluetooth),
  (USB grip), (wireless) for the Steam receiver, (built in) for the Steam Deck.
- The controller drawing shows the right model.

**W1.3 Buttons and axes.** Run `doppelpad monitor` and press everything.
*Expected:*
- Face buttons go by position, whatever is printed on them: bottom `A`, right `B`, left `X`,
  top `Y`. On a Nintendo controller, the bottom "B" shows as `A`.
- Shoulders, stick clicks, `Back`, `Start`, `Guide` and the D-pad each show under their own
  name. The DualShock 4's Share and the DualSense's Create are `Back`, the DualSense's Mute
  and the Switch Capture button are `Misc`, a touchpad click is `Touchpad`. Back paddles show
  as `PaddleUL` … `PaddleLR`.
- Sticks go from −1 to +1, with up and right positive, and rest near 0.
- Triggers go from 0 to 1. On Switch controllers and the Switch 2 Pro Controller they are
  digital: 0 or 1.
- No control is stuck, missing or doubled.

**W1.4 Reconnect.** Turn the controller off and on (wireless), or unplug it and plug it back
in (USB). Do this while `monitor` runs, and again in the app with a config in a slot.
*Expected:*
- The controller comes back within a few seconds with the same id, and no duplicate entry is
  left behind.
- The running config picks it up again.
- With the app's window in the background, the notifications say that the controller
  disconnected and connected.

**W1.5 Battery.** Look at each wireless controller's battery in the bottom bar and in the tray
icon's tooltip. Then plug it in.
*Expected:*
- DualShock 4 and DualSense show 5 to 95 % in steps of 10, or 100 %. Switch controllers show
  one of 0, 10, 30, 60 and 100 %. Xbox controllers show one of 5, 30, 60 and 100 %. Switch 2
  controllers and the Steam Controller on its receiver show a percentage.
- On the cable, Sony and Switch controllers show as charging. Xbox controllers never do
  (XInput doesn't say). Wired pads show no battery.
- If a battery drops below 40 % or 20 % during the tests, one notification says so (not
  while charging).

## 2. Lights

`extras.toml`, also used in sections 3 to 7:

```toml
name = "Windows test: controller extras"

[output]
virtual_pad = "xbox360"
hide_physical = false

# The light bar breathes, one colour per breath.
[lightbar]
mode = "breath"
colors = ["#14b8a6", "#8b5cf6"]
cycle = true
period_ms = 2000

# DualSense: resistance on the left trigger, a click on the right one.
[triggers.left]
effect = { kind = "resistance", start = 2, strength = 6 }

[triggers.right]
effect = { kind = "weapon", start = 3, end = 6, strength = 8 }

# While LB is held: an orange light bar, the left trigger free, the right one vibrating.
[[layers]]
id = 1
name = "Shift"
lightbar = { mode = "solid", colors = ["#f97316"] }

[layers.triggers.left]
effect = { kind = "off" }

[layers.triggers.right]
effect = { kind = "vibration", start = 0, amplitude = 6, frequency = 30 }

[[mappings]]
trigger = "LB"
jump = { layer = 1, mode = "hold" }

# Y rumbles the heavy motor for 300 ms, X the light one.
[[mappings]]
trigger = "Y"
actions = [{ rumble = 300, low = 1.0, high = 0.0 }]

[[mappings]]
trigger = "X"
actions = [{ rumble = 300, low = 0.0, high = 1.0 }]

# Back + Start turns the controller off.
[[mappings]]
trigger = ["Back", "Start"]
actions = [{ command = "power_off" }]
```

Do every check in this section over USB and over Bluetooth.

**W2.1 Light bar.** On a DualShock 4 or DualSense, run `doppelpad run extras.toml`. Hold LB
for a few seconds, then let go. Then change `mode` to `rainbow`, `strobe`, `heartbeat`,
`shimmer` and `off` in turn, and run it again each time.
*Expected:*
- The light bar breathes smoothly, teal then violet, one colour per two-second breath.
- While LB is held it is solid orange; after letting go it breathes again.
- Each mode does what its name says. `off` turns the light bar dark.
- Over Bluetooth it looks the same, without flicker, and input keeps arriving.
- *Record:* what the light bar shows after Ctrl+C and after the app quits. Doppelpad stops
  setting it then, but does not reset it.

Then in the app, apply the config to slot 1, change the light bar in *Config settings →
Light & rumble*, and choose *Apply to slot 1* again. *Expected:* the controller shows the new
light bar.

**W2.2 Player lights.** Use the app, with a DualSense and a Switch or Switch 2 controller
(the DualShock 4 and Xbox controllers have no player lights Doppelpad can set). Put a config
in each of the four slots. Then, for each choice of *Settings → Controllers → Controller
lights → Player lights*, click slots 1 to 4 and turn remapping off and on:
1. *Slot number while remapping is on* (the default)
2. *Always the slot number*
3. *Leave them to the controller*

*Expected:*
- DualSense: slot 1 lights the centre light, slot 2 the second and fourth, slot 3 the first,
  third and fifth, slot 4 all but the centre.
- Switch and Switch 2 controllers fill up from the first light: slot 1 lights light 1, slot 2
  lights 1 and 2, and so on. A Joy-Con pair shows it on both halves.
- With choice 1 the lights go dark while remapping is off; with choice 2 they keep the
  number; with choice 3 Doppelpad leaves them as they were.
- A controller with slots of its own ([section 9](#9-several-controllers)) shows its own
  active slot.

**W2.3 Microphone LED.** On a DualSense, tick *Microphone LED shows whether remapping is on
(DualSense)*. Turn remapping off and on with the remap button, then with a middle click on the
tray icon. Untick it again.
*Expected:*
- The LED is on while remapping is on, and off while it is off.
- *Record:* what the LED does when the Mute button is pressed, and what it shows after
  unticking (Doppelpad stops setting it then).

**W2.4 After a reconnect.** With extras.toml running and the player lights on, turn the
controller off and on, or plug it out and in. Then do it once more with Steam running.
*Expected:* the light bar and the player lights are back within a few seconds. *Record:*
whether Steam changes them when it takes the controller over (on Linux it did, K11 in
[LINUX-TESTING.md](LINUX-TESTING.md#known-issues)).

## 3. DualSense trigger effects

Use a DualSense or DualSense Edge, over USB and over Bluetooth.

**W3.1 Effects from the profile.** Run extras.toml. Pull both triggers slowly, then hold LB
and pull them again. Stop with Ctrl+C.
*Expected:*
- The left trigger pushes back from about a fifth of its travel.
- The right trigger has a click: it resists from about a third, then gives way near
  two thirds.
- While LB is held, the left trigger is free and the right one vibrates from the start.
  Letting go of LB brings the first two effects back.
- After Ctrl+C both triggers are free (Doppelpad sends `off` when a profile stops).

**W3.2 Presets in the app.** Apply extras.toml to a slot. In *Analog → Triggers → Adaptive
trigger*, try each preset (soft, medium, hard, half way, rifle, pulse, rumble) on both
triggers, with *Apply to slot 1* after each change. Then select the layer "Shift" in the
toolbar, give it its own effect, and turn Main's effect off on that layer.
*Expected:*
- Each preset feels as its name says.
- The layer's effect applies only while LB is held.

**W3.3 Leaving.** With an effect on the triggers and a second controller connected, pick the
other controller in the bottom bar. Pick the DualSense again, then quit the app from the tray
icon.
*Expected:* the DualSense's triggers are free again in both cases.

## 4. Rumble

**W4.1 Rumble from a mapping.** Run extras.toml. Press Y, then X, then each several times
quickly. Use a DualShock 4 or DualSense over Bluetooth, then over USB, then any other
controller with rumble.
*Expected:*
- Y runs the heavy motor and X the light one, each for about 300 ms, and then they stop.
- Over Bluetooth it works as over USB, and input keeps arriving while the motors run.
- Quick presses never leave a motor running.

**W4.2 Rumble from the game.** `vibration.toml`:

```toml
name = "Windows test: vibration"

[output]
virtual_pad = "xbox360"
hide_physical = false

[vibration]
left = 1.0
right = 1.0
swap = false
min_delay_ms = 20

# Y plays a rumble from a mapping, which the settings above do not change.
[[mappings]]
trigger = "Y"
actions = [{ rumble = 300, low = 1.0, high = 1.0 }]
```

Run it. Open the gamepad test page in Chrome or Edge and play its vibration test on the
virtual controller ("Xbox 360 Controller"). Then try a game with rumble.
*Expected:* the physical controller rumbles as long as the page or game asks, with both
motors, and stops afterwards. Over Bluetooth too.

**W4.3 Vibration settings.** Change vibration.toml and run it again after each change:
1. `left = 0.0`
2. `left = 1.0`, `right = 0.3`
3. `swap = true`
4. `min_delay_ms = 200`

*Expected:*
- With `left = 0.0` the heavy motor stays still for the game's rumble; with `right = 0.3` the
  light one is weaker.
- With `swap = true` each motor plays the other one's part.
- With `min_delay_ms = 200` quick changes in the game's rumble arrive coarser, but the last
  value always arrives.
- Y rumbles at full strength in every case.
- The same settings in *Config settings → Light & rumble → Vibration* do the same.

## 5. Turning controllers off

**W5.1 The command.** Run extras.toml and press Back + Start, once for each wireless
controller, connected without a cable. Then once on a cable.
*Expected:*
- A DualShock 4 or DualSense over Bluetooth switches off within a few seconds: Doppelpad drops
  its Bluetooth link. The log says `controller turned off`; if the link cannot be dropped,
  `Bluetooth disconnect failed`.
- A Switch controller over Bluetooth switches off. The log says `controller told to turn
  off`.
- An Xbox controller on its wireless adapter or over Bluetooth switches off.
- On a cable the controller stays on, and the log says `this controller cannot be turned off`.
- The Steam Controller on its receiver and the Switch 2 Pro Controller over Bluetooth: see
  [K1 and K2](#known-issues). Record what happens.
- The PS, Home or Xbox button turns the controller on again. It comes back with the same id,
  and the config picks it up.

**W5.2 From the app.** Choose **Turn off** in the cog menu next to the controller list.
*Expected:* the same as W5.1.

**W5.3 Idle timeout.** In *Settings → Controllers → Turn wireless controllers off after*, choose
the shortest time. Put a wireless controller down and wait. Then do it again, but press a
button halfway through.
*Expected:*
- It turns off after that time, and the log says `no input for a while`. Sticks that drift a
  little at rest don't keep it on.
- A button press starts the time again.
- Controllers on a cable stay on.

## 6. Steam Controller and Steam Deck

Out of the box, both act as keyboard and mouse ("desktop mode" or "lizard mode"). Doppelpad
turns that off while it reads them and gives it back when it stops. The Deck needs a reminder
twice a second, or it turns it back on by itself. Quit Steam for W6.1 to W6.6.

**W6.1 Before Doppelpad.** With nothing running, use the controller on the desktop.
*Record:* what moves the cursor and what clicks (on the Steam Controller the right pad and
the triggers; on the Deck the trackpads).

**W6.2 Desktop mode off.** Run `doppelpad monitor` and use every control.
*Expected:*
- The pads no longer move the cursor, and no keys or clicks reach the desktop. On the Deck
  this lasts for minutes, as long as `monitor` runs.
- `monitor` shows "Steam Controller" or "Steam Deck" with:
  - the buttons; the Steam button as `Guide`; the Deck's ··· button as `Misc`;
  - the Steam Controller's grips as `PaddleUL` and `PaddleUR`; the Deck's L4 and R4 as
    `PaddleUL` and `PaddleUR`, L5 and R5 as `PaddleLL` and `PaddleLR`;
  - the left trackpad as `touch(x,y)`, from 0 to 1 (`monitor` shows only the first touch,
    the left pad; W6.4 covers the right one), and a pad click as `Touchpad`;
  - analog triggers from 0 to 1, and the gyro.
- On the Steam Controller, the right pad also moves the right stick.

**W6.3 Desktop mode back.** Stop `monitor` with Ctrl+C. Then run it again and end it in Task
Manager instead.
*Expected:*
- After Ctrl+C, the Steam Controller's right pad moves the cursor again at once, and the Deck
  does so after about a second.
- *Record:* what each does after Task Manager ended Doppelpad. The Deck should come back by
  itself; the Steam Controller may need to be plugged in again.

**W6.4 Two trackpads.** `trackpads.toml`:

```toml
name = "Windows test: two trackpads"

[output]
virtual_pad = "none"

[touchpad]
split = true

[[mappings]]
trigger = "TrackpadLeftTap"
actions = [{ key = "Digit1" }]

[[mappings]]
trigger = "TrackpadRightTap"
actions = [{ key = "Digit2" }]

[[mappings]]
trigger = "TrackpadLeftClick"
actions = [{ key = "Digit3" }]

[[mappings]]
trigger = "TrackpadRightClick"
actions = [{ key = "Digit4" }]

[[mappings]]
trigger = "TrackpadLeftUp"
actions = [{ key = "Q" }]

[[mappings]]
trigger = "TrackpadRightDown"
actions = [{ key = "W" }]
```

Run it with Notepad in front. Tap, click and rest a finger on each pad.
*Expected:* each pad has its own tap (1, 2), click (3, 4) and regions: a finger resting at the
top of the left pad types q, at the bottom of the right pad w.

**W6.5 Rumble.** Run extras.toml and press Y and X.
*Expected:* the Deck rumbles. The Steam Controller doesn't: Doppelpad has no rumble for it yet.

**W6.6 The wireless receiver.** With the Steam Controller on its receiver, turn it off and on
while the app runs.
*Expected:*
- It goes away and comes back within a few seconds, and its desktop mode is off again when it
  is back.
- Its battery shows in the app.
- Turning it off from Doppelpad: see [K1](#known-issues).

**W6.7 With Steam running.** Start Steam and repeat W6.2 in the app with extras.toml in slot 1.
*Record:*
- whether Steam's own desktop layout comes back and fights Doppelpad (cursor moving, keys);
- the virtual Xbox controller Steam adds for these pads, and what *Settings → Controllers*
  lists (GUI.md says to ignore one of the two);
- what the controller list says about Steam.

## 7. GameCube adapter

Use the official adapter, or a clone switched to its Wii U mode. Plug in both plugs: the grey
one is needed for rumble. Windows' own driver for the adapter comes first; W7.5 tries the
WinUSB driver that Dolphin needs.

**W7.1 Detection.** Run `doppelpad devices` with controllers in some ports. Then plug a
controller into a free port, and pull one out, while `monitor` runs.
*Expected:*
- One "GameCube Controller (adapter port N)" per occupied port, with the id
  `gcadapter:<instance>:<N>` and the backend `gcadapter`. Empty ports are not listed.
- A controller plugged in shows within a few seconds; one pulled out goes away.
- The log says `GameCube adapter connected` with `hid=true`.

**W7.2 Buttons and sticks.** Plug the controllers in with the sticks and triggers untouched:
their resting position is taken then. Watch `monitor`.
*Expected:*
- Buttons go by position: A is `A`, B is `X`, X is `B`, Y is `Y`, Z is `RB`, Start is `Start`,
  and the D-pad.
- The main stick is the left stick and the C-stick the right one. Both rest near 0 and reach
  about ±1 at the edge.
- L and R go from 0 to 1 and read 1 at the click.

**W7.3 Rumble.** Run extras.toml and press Y. Then pull out the grey plug and press Y again.
*Expected:*
- With both plugs, the controller rumbles for about 300 ms (a GameCube controller has one
  motor, which runs while the stronger of the two values is 0.2 or more).
- With only the black plug, nothing rumbles and nothing breaks. A WaveBird never rumbles.

**W7.4 Hiding.** Run pad-a.toml from [section 9](#9-several-controllers) with a controller in
port 1, and look at `joy.cpl`.
*Expected:* the adapter is hidden while the config runs, and back after it stops.

**W7.5 WinUSB driver** (optional). Install the WinUSB driver for the adapter, as Dolphin's
guide describes. Repeat W7.1 to W7.4.
*Expected:* it still works, and the log says `hid=false`. Doppelpad cannot hide it with this
driver (GUI.md); *record* whether the app says so.

**W7.6 Dolphin holding it** (with the WinUSB driver). Start Dolphin with its GameCube adapter
support on, then start Doppelpad. Close Dolphin.
*Expected:* the controller list names Dolphin while it runs. After Dolphin closes, Doppelpad
takes the adapter within 30 seconds, or at once with *Look for controllers*.

**W7.7 The adapter unplugged.**
*Expected:* the log says `GameCube adapter gone`, its ports go away, and they come back when
it is plugged in again.

## 8. Virtual DualShock 4

`virtual-ds4.toml`:

```toml
name = "Windows test: virtual DualShock 4"

[output]
virtual_pad = "dual_shock4"
hide_physical = true

# Y swipes left on the virtual touchpad, X taps it with two fingers.
[[mappings]]
trigger = "Y"
actions = [{ touch = "swipe_left" }]

[[mappings]]
trigger = "X"
actions = [{ touch = "multi_tap" }]

# Up on the D-pad shakes the virtual accelerometer.
[[mappings]]
trigger = "DpadUp"
actions = [{ shake = "+y" }]
```

Watch the virtual controller in a program that reads a DualShock 4 with its touchpad and
gyro: Steam's controller settings, or a game with DualShock 4 support.

**W8.1 The virtual controller.** Run virtual-ds4.toml, then stop it.
*Expected:*
- While it runs, `joy.cpl` lists a "Wireless Controller" and no longer the physical
  controller. Both change back after it stops.
- `doppelpad devices`, run in a second window, doesn't list the virtual controller.

**W8.2 Buttons, sticks and triggers.** Use every control.
*Expected:* everything arrives, Cross, Circle, Square and Triangle by position, and the
*What Doppelpad sends* panel in the app (the pulse button at the top right of the
controller view) shows the same.

**W8.3 Touchpad.** Use a DualShock 4 or DualSense: touch with one and two fingers, and click.
Then press Y and X.
*Expected:*
- Fingers and clicks pass through to the virtual touchpad.
- Y plays a swipe to the left, X a tap with two fingers.
- *Record:* that touch does not pass through while a mapping or route of the config uses the
  touchpad (add one from gyro-touch.toml in [section 12](#12-gyro-and-touchpad)).

**W8.4 Motion.** Use a controller with motion sensors: turn and tilt it. Then press up on the
D-pad.
*Expected:*
- The virtual gyro and accelerometer follow the controller, in the right directions. A turn
  of 90° reads as about 90° where the program shows angles.
- Up on the D-pad gives a short jolt on the accelerometer.

**W8.5 Rumble from the game.** Play rumble through the virtual controller, from a game or from
Steam's controller settings.
*Expected:* the physical controller rumbles, over USB and over Bluetooth.

**W8.6 Light bar from the game.** On a DualShock 4 or DualSense, let a game or Steam set the
virtual controller's light bar colour (Steam's settings for a PlayStation controller have
one). Then add the `[lightbar]` table of extras.toml to the config and run it again.
*Expected:*
- The physical light bar takes the game's colour, over USB and over Bluetooth
  ([K3](#known-issues)).
- With a `[lightbar]` in the config, the config's light bar wins.

## 9. Several controllers

`pad-a.toml`:

```toml
name = "Windows test: pad A"

[output]
virtual_pad = "xbox360"
hide_physical = true

# A arrives as B.
[[mappings]]
trigger = "A"
actions = [{ pad = "B" }]
```

`pad-b.toml`:

```toml
name = "Windows test: pad B"

[output]
virtual_pad = "dual_shock4"
hide_physical = true

# A arrives as Y (Triangle).
[[mappings]]
trigger = "A"
actions = [{ pad = "Y" }]
```

Use two controllers that are not both Xbox controllers (for those, see W9.5).

**W9.1 Own slots.** In the app, pick the first controller in the bottom bar and apply pad-a.toml
to slot 1. Pick the second, choose **Own slots for this controller** in its cog menu, and
apply pad-b.toml to its slot 1. From the command line the same is
`doppelpad apply pad-b.toml --slot 1 --device <id>`.
*Expected:*
- `joy.cpl` shows two virtual controllers, an Xbox 360 controller and a "Wireless
  Controller", and neither physical one.
- Each physical controller moves only its own virtual controller: A gives B on the Xbox 360
  controller and Triangle on the other.
- The second controller is marked with a stack icon in the list.

**W9.2 Switching.** Turn remapping off and on with the remap button. Then turn *Autodetect* on
and bring a game with a config of its own to the front.
*Expected:* the remap button switches both controllers. Autodetect changes only the shared
slots: the second controller keeps pad-b.toml.

**W9.3 The second controller goes away.** Disconnect it, then connect it again. Then tick
*Settings → Remapping → Keep the virtual controller when a controller with slots of its own
disconnects* and repeat.
*Expected:*
- Its virtual controller goes away, and comes back with pad-b.toml when it reconnects. The
  first controller keeps running pad-a.toml.
- With the setting ticked, its virtual controller stays, idle, while it is away.

**W9.4 The first controller goes away.** Disconnect the first controller for a moment, with a
third controller connected if you have one.
*Expected:* the shared slots move to another controller without slots of their own, and come
back to the first one when it returns.

**W9.5 Two Xbox controllers** ([K4](#known-issues)). Repeat W9.1 with two Xbox controllers.
Then set `hide_physical = false` in pad-b.toml and apply it again.
*Record:* what `joy.cpl` and a game see in both cases.

**W9.6 Groups** (optional). In *Settings → Controllers*, make a group of two controllers and
run extras.toml on it.
*Expected:* the group is one controller. Rumble, the light bar and trigger effects go to both.

## 10. Nintendo controllers

Pair Switch controllers over Bluetooth in Windows' Bluetooth settings as usual. Switch 2
controllers are different: over USB Windows sets them up by itself, and over Bluetooth
Doppelpad connects them itself (W10.7).

**W10.1 Detection.** Connect each controller you have:
- Switch Pro Controller, over USB and over Bluetooth;
- Joy-Cons over Bluetooth, and in the charging grip on USB;
- Nintendo Switch Online pads (SNES, N64, Genesis) over Bluetooth;
- wired Switch pads (HORI, PDP, PowerA, ZUIKI);
- Switch 2 Pro Controller and Switch 2 GameCube controller over USB.

*Expected:*
- Each is listed once, with its connection after its name.
- For a Switch 2 controller on USB, Device Manager shows its second interface with the
  WinUSB driver, which Windows installed by itself.

**W10.2 Buttons.** Watch `monitor` (W1.3 also applies).
*Expected:*
- B (bottom) is `A`, A (right) is `B`, Y (left) is `X`, X (top) is `Y`.
- Minus is `Back`, Plus is `Start`, Home is `Guide`, Capture is `Misc`.
- On the Pro Controllers and Joy-Cons, ZL and ZR read 0 or 1.
- On the Switch 2 Pro Controller, GL and GR are `PaddleUL` and `PaddleUR`, and C is `Misc2`.
- The Switch 2 GameCube controller's buttons go by position as in W7.2, its ZL is `LB`, and
  L and R are analog.
- *Record:* how the N64 and Genesis pads' extra buttons come out.

**W10.3 Joy-Cons.** Connect a left and a right Joy-Con, then only one of them.
*Expected:*
- A left and a right Joy-Con become one controller. SL and SR become paddles: left SL
  `PaddleUL`, left SR `PaddleLL`, right SR `PaddleUR`, right SL `PaddleLR`.
- A single Joy-Con works sideways: its buttons go by their sideways position, SL is `LB` and
  SR is `RB`.
- In the charging grip both halves are one controller.

**W10.4 Rumble.** Run extras.toml and press Y and X; then play rumble from a game through the
virtual controller (W4.2).
*Expected:* each controller with rumble buzzes, the two motors feel different, and they stop
afterwards. Switch 2 controllers on USB too.

**W10.5 USB stays alive.** Leave a Switch Pro Controller on USB untouched for ten minutes with
the app running, then use it.
*Expected:* it works at once. If it went silent, the log says `Switch controller silent over
USB, starting it again` and it came back by itself.

**W10.6 Switch 2 on USB with Steam.** Start Steam with a Switch 2 controller on USB, then
start the app. Close Steam.
*Expected:* while Steam holds the controller's command interface, the controller still works,
with default stick calibration, and the app says Steam is using it. After Steam closes,
Doppelpad takes the interface over within a few seconds, without reconnecting the controller.

Motion, player lights and batteries of these controllers are in W12.1, W2.2 and W1.5.

### Switch 2 Pro Controller over Bluetooth

On Windows 10 this was tried before: connecting, every button and trigger, both sticks,
motion, battery, player light 1, rumble, and letting the controller go 3 s after it was
turned off. The controller never asks for a faster connection, so on Windows 10 its input
arrives unevenly: at times a steady 17 reports a second, at times 15 to 105 (about 45 on
average). Doppelpad asks Windows for a faster connection, which only Windows 11 can grant.
These checks are on Windows 11. Leave the controller out of Windows' Bluetooth settings.

**W10.7 Connecting on Windows 11.** Start the app with debug detail (setup step 7). Hold the
small pairing button on top of the controller until its lights run.
*Expected:*
- The log says, in this order: `Bluetooth: looking for Switch 2 controllers in pairing
  mode`, `Switch 2 Pro Controller in pairing mode, connecting`, `asked for a faster
  connection` with a `status`, `connection interval … ms`, and `Switch 2 Pro Controller
  connected`.
- The app lists "Switch 2 Pro Controller (Bluetooth)", and *Settings → Drivers* lists
  Bluetooth as available.
- *Record:* the status and the connection interval. Do the same on Windows 10 and record its
  interval too.

**W10.8 The input rate.** This needs the build tools of the [README](../README.md#build) on
the Windows 11 PC. Quit the app, put the controller in pairing mode and run:

```powershell
cargo test -p dp-platform --lib switch2_pro_over_bluetooth -- --ignored --nocapture
```

It prints one line a second with the number of reports, whether motion arrived, the battery
and the buttons pressed. It rumbles in the first second. Press buttons, move the sticks, and
let it run for a minute.
*Expected:* more reports a second than on Windows 10, and steadier. *Record:* the lowest,
typical and highest count, next to the Windows 10 numbers above.

**W10.9 Everything else on Windows 11.** In the app and with `doppelpad monitor`, repeat what
was tried on Windows 10. Then turn the controller off, and put it into pairing mode again.
*Expected:*
- Every button and trigger, both sticks, motion, the battery, player light 1 and rumble work.
- When it is turned off, the log says `Switch 2 controller disconnected` about 3 s later.
- In pairing mode again, it connects again.

**W10.10 The off switch.** With the controller connected, untick *Settings → Controllers → Look
for Switch 2 controllers over Bluetooth*. Then turn the controller off and put it into pairing
mode. Restart the app. Tick the setting again. Do this on Windows 10 and Windows 11.
*Expected:*
- When it is unticked, the log says `Bluetooth: no longer looking for Switch 2 controllers`.
  The connected controller stays connected, and the Bluetooth row in *Settings → Drivers*
  goes away.
- A controller in pairing mode is not connected while the setting is off. The setting is
  still off after the restart.
- When it is ticked, the log says `Bluetooth: looking for Switch 2 controllers in pairing
  mode`, and a controller in pairing mode connects.

## 11. Generic pads and joysticks

Use a game pad or joystick that has no driver of its own in Doppelpad, for example an older
DirectInput pad, an arcade stick or a flight stick.

**W11.1 Detection.** Connect it and run `doppelpad monitor`.
*Expected:* it is listed with an `hid:` id and its own name. With the guessed layout (the
common DirectInput order) its controls move something.

**W11.2 Identifying the buttons.** In the app, choose **Identify buttons …** in its cog menu.
Answer each question, and skip what it doesn't have. Then watch `monitor` again (app closed).
If you have a second one of the same model, connect it.
*Expected:*
- Every control now shows under the name it was given.
- The layout is kept in `%APPDATA%\doppelpad\state.toml` (under `layouts`, by vendor and
  product id), and the second controller of the model uses it too.
- The same menu entry can discard the layout, and the guess comes back.
- The welcome guide (*Settings → General → Welcome*) offers the same for an unknown
  controller.

**W11.3 Hats, sliders and many buttons.** On a joystick or a pad with a hat switch, a throttle
slider or more than 16 buttons, identify them.
*Record:* which controls could be assigned and which not.

**W11.4 Hiding and a virtual controller.** Run pad-a.toml with it.
*Expected:* `joy.cpl` shows the virtual Xbox 360 controller and no longer the pad, and A
arrives as B.

## 12. Gyro and touchpad

Controllers with motion sensors: DualShock 4, DualSense, Switch Pro Controller, Joy-Cons,
Switch 2 Pro Controller, Steam Controller, Steam Deck. With a touchpad: DualShock 4, DualSense,
and the trackpads of the Steam Controller and the Deck.

`gyro-touch.toml`:

```toml
name = "Windows test: gyro and touchpad"

[output]
virtual_pad = "none"

[gyro]
mode = "motion"
steering = "yaw"

[touchpad]
zones = "grid"

# Gyro aiming while LT is held.
[[analog]]
source = "gyro"
target = "mouse"
while_held = ["LT"]

# Gyro directions and touchpad controls type letters and digits.
[[mappings]]
trigger = "GyroLeft"
actions = [{ key = "A" }]

[[mappings]]
trigger = "GyroRight"
actions = [{ key = "D" }]

[[mappings]]
trigger = "TouchTap"
actions = [{ key = "Digit1" }]

[[mappings]]
trigger = "TouchSwipeLeft"
actions = [{ key = "Digit2" }]

[[mappings]]
trigger = "TouchSwipe2Up"
actions = [{ key = "Digit3" }]

[[mappings]]
trigger = "TouchZoomIn"
actions = [{ key = "Digit4" }]

[[mappings]]
trigger = "TouchUpLeft"
actions = [{ key = "Q" }]

[[mappings]]
trigger = "TouchCenter"
actions = [{ key = "W" }]

# RB switches the gyro on and off, Back sets the tilt centre.
[[mappings]]
trigger = "RB"
actions = [{ command = "toggle_gyro" }]

[[mappings]]
trigger = "Back"
actions = [{ command = "reset_tilt" }]
```

**W12.1 Raw motion and touch.** Watch `monitor` for every controller with motion sensors or a
touchpad, and record the results in the [Controllers](#controllers) table.
*Expected:*
- `gyro(x,y,z)` is in °/s and rests near 0.
- With the controller flat, nose up gives positive x, and turning left gives positive y.
  Rolling moves z. A brisk turn reaches a few hundred.
- `touch(x,y)` runs from 0 to 1, with the origin at the top left.
- Motion and touch belong to the same entry as the buttons. For a Joy-Con pair, *record*
  which half's motion comes through.

**W12.2 Gyro mappings and routes.** Run gyro-touch.toml with Notepad in front.
1. Turn the controller left and right, then hold LT and turn it.
2. Press RB and turn it; press RB again.
3. Change `steering` to `"roll"`, run it again, and lean the controller left and right.
4. Change `mode` to `"tilt"`, run it again, press Back with the controller held normally,
   then tilt it.
5. Add `space = "player"` under `[gyro]` (with `mode = "motion"`), run it again, and aim with
   LT held while holding the controller upright.

*Expected:*
- Turning left types a and right types d. With LT held, the cursor follows the turn (about
  10 px per degree); without LT it doesn't move.
- After RB, turning does nothing until RB is pressed again.
- With `"roll"`, leaning types a and d instead of turning.
- In tilt mode, tilting past about 10° from the pose Back took holds a or d for as long as it
  stays tilted.
- With `"player"`, turning left and right moves the cursor sideways however the controller is
  held.

**W12.3 Calibration.** In the app, apply gyro-touch.toml to a slot, put the controller down and
hold LT with a cursor in view. Open *Analog → Gyro → Calibration of this controller*:
choose *Calibrate* while it lies still, then *Reset*, then leave *Calibrate automatically
while it rests* on.
*Expected:*
- Before calibrating, the cursor may drift slowly. After *Calibrate* it stays still. If the
  controller moved during the three seconds, the old calibration stays.
- With automatic calibration on, drift goes away after the controller rests a moment.
- The calibration is kept per controller in `%APPDATA%\doppelpad\calibration.toml`.

**W12.4 Touchpad.** On a DualShock 4 or DualSense, with gyro-touch.toml running and Notepad in
front: tap, swipe left, swipe up with two fingers, spread two fingers, and rest a finger in
the top left and in the middle of the pad. Then change `zones` to `"radial"` and add
`click_required = true` under `[touchpad]`.
*Expected:*
- They type 1, 2, 3, 4, q and w.
- With `"radial"`, the regions are sectors around the centre. With `click_required`, regions
  only count while the pad is clicked; the gestures still work.
- trackpads.toml from W6.4 splits the touchpad into two halves, each with its own tap, click
  and regions.

## 13. Overlays over games

Overlays are drawn by the app, not by `doppelpad daemon`. Apply this config to slot 1.

`overlays.toml`:

```toml
name = "Windows test: overlays"

[output]
virtual_pad = "xbox360"
hide_physical = true

# A crosshair with a magnifier while LT is held.
[[crosshairs]]
name = "Dot"
shape = "circle_dot"
color = "#00ff66"
size = 30
magnifier = { zoom = 2.0, size = 0.2 }
layers = [1]

[[layers]]
id = 1
name = "Aim"

[[layers]]
id = 2
name = "Menu"

[[mappings]]
trigger = "LT"
jump = { layer = 1, mode = "hold" }
actions = [{ trigger = "left", value = 1.0 }]
description = "Aim"

# Hold LB, point with the right stick, let go.
[[mappings]]
trigger = "LB"
jump = { layer = 2, mode = "hold" }
description = "Radial menu"

[[radial_menus]]
name = "Test menu"
layer = 2
select = "right_stick"
apply = "release"

[[radial_menus.sectors]]
label = "One"
actions = [{ key = "Digit1" }]

[[radial_menus.sectors]]
label = "Two"
actions = [{ key = "Digit2" }]

[[radial_menus.sectors]]
label = "Three"
actions = [{ key = "Digit3" }]

# Back with up or down on the D-pad switches the two overlays.
[[mappings]]
trigger = ["Back", "DpadUp"]
actions = [{ command = "pad_overlay" }]
description = "Controller overlay"

[[mappings]]
trigger = ["Back", "DpadDown"]
actions = [{ command = "mappings_overlay" }]
description = "Mappings overlay"
```

Do W13.1 to W13.5 in at least two games, each in its three display modes: windowed,
borderless (full-screen window) and exclusive full screen. Games with DirectX 11, DirectX 12
and Vulkan are best. Record each game, its graphics API and the mode in the notes.

**W13.1 Notifications.** Choose *Show a sample* in *Settings → Display → On-screen
notifications*, switch to the game, then turn remapping off and on, change the slot, hold LT,
and plug a controller out and in.
*Expected:*
- In windowed and borderless mode, the messages show over the game in the chosen corner and
  monitor.
- In exclusive full screen the game draws over them, as GUI.md says. *Record* if they show
  anyway.

**W13.2 Crosshair and magnifier.** Hold LT.
*Expected:*
- The crosshair shows in the middle of the primary monitor while LT is held.
- The magnifier shows the game around it, enlarged twice, and keeps up with the game.
- *Record:* whether the magnifier shows the game or stays black or frozen, and whether the
  game's frame rate drops.

**W13.3 Radial menu.** Hold LB, point at a sector with the right stick, and let go.
*Expected:* the ring shows over the game; while it points, the right stick doesn't reach the
game; the sector's digit is typed when LB is let go.

**W13.4 Controller and mappings overlay.** Press Back + up, wait a few seconds, use the
controller, then press Back + up again. Press Back + down, hold LT and LB, and press Back +
down again.
*Expected:*
- The controller overlay shows the controller in use with its pressed buttons, sticks and
  trigger bars. It hides after a few seconds without input and comes back with the next one.
- The mappings overlay lists the active layer's mappings with their descriptions and follows
  the layer while LT or LB is held.

**W13.5 Focus.** While the overlays show, keep playing.
*Expected:* the game keeps the focus: it doesn't minimise, its mouse capture stays, and clicks
go to the game. The overlays don't appear in Alt+Tab.

**W13.6 Hotkey rumble in exclusive full screen.** Tick *Settings → Hotkeys → Confirm with a
rumble on the controller*, and use the slot hotkeys (LB + RB + X / Y / B / A) in a game in
exclusive full screen.
*Expected:* the controller rumbles as often as the slot's number.

**W13.7 Scaling and monitors** (optional). Set the display scaling to 150 % and connect a second
monitor. Repeat W13.1, W13.2 and W13.4.
*Record:* whether the overlays have the right size and place.

## 14. DSU motion server

**W14.1 Turning it on.** In *Settings → Connections → Motion server (DSU)*, tick *Turn the
motion server on*.
*Expected:* the status says `Running on 127.0.0.1:26760 · 0 emulator(s) connected`, and the log
says `DSU motion server listening`.

**W14.2 An emulator.** Point an emulator at `127.0.0.1`, port 26760. For example: Cemu's input
settings with the `DSUController` API; Dolphin's *Alternate Input Sources* (DSU client) in its
controller settings; Ryujinx's motion settings with CemuHook-compatible motion. Use a
controller with motion sensors. Record the emulator and its version.
*Expected:*
- The status counts the emulator.
- The emulator lists the first four controllers in slots 1 to 4.
- Buttons, sticks and triggers arrive as the physical controller sends them, not remapped.
- The emulator's motion preview, or a game that aims with motion, follows the controller in
  the right directions and at the right speed.
- *Record:* whether the touchpad (DualShock 4, DualSense) and the battery come through.

**W14.3 Slots.** Use two controllers. Disconnect the first, then connect it again.
*Expected:* each keeps its slot while connected. *Record* which slot the reconnected
controller gets.

**W14.4 Remapping meanwhile.** Run pad-a.toml in a slot while the emulator reads the
controller.
*Expected:* the emulator still gets the physical controller over DSU, although games no
longer see it.

**W14.5 The network** (optional). Tick *Reachable from the network too*, and point an emulator
on another computer at this PC's address. Untick it again.
*Expected:* the other computer connects only while it is ticked. *Record* whether Windows
Defender Firewall asks.

## 15. Lock screen and keeping awake

`lockscreen.toml`:

```toml
name = "Windows test: lock screen"

[output]
virtual_pad = "xbox360"
hide_physical = false

# A types k.
[[mappings]]
trigger = "A"
actions = [{ key = "K" }]

# X toggles a macro that types x twice a second.
[[mappings]]
trigger = "X"
mode = "toggle"
actions = [{ key = "X", action = "tap" }, { pause = 500 }]
```

**W15.1 The lock screen.** Apply lockscreen.toml to a slot. In Notepad, press A, then X. Lock
the PC with Win+L. On the lock screen, open the password field and press A and B. Unlock, and
press A.
*Expected:*
- The log says `the user's desktop changed locked=true` and `remapping paused` with
  `reason=desktop locked`.
- The x's stop when the PC locks, and A types nothing into the password field.
- After unlocking, the log says `locked=false` and `remapping resumed`, and A types k again.
- *Record:* whether the toggled macro is still running after unlocking.

**W15.2 Ctrl+Alt+Del and administrator prompts.** Repeat W15.1 with the Ctrl+Alt+Del screen,
and with an administrator prompt (for example *Run as administrator* on PowerShell).
*Expected:* remapping pauses while either shows, and resumes afterwards. *Record* if the
prompt shows without dimming the desktop: that is a setting of User Account Control.

**W15.3 The daemon.** Quit the app and run
`doppelpad daemon --profile lockscreen.toml`. Repeat W15.1.
*Expected:* the same, with the same log lines in the daemon's window.

**W15.4 Keeping awake.** In the app, tick *Settings → Remapping → Keep the PC awake while a
mapping plays input*. Set Windows' sleep time to one minute. Start the x macro with X and
leave it; in an administrator PowerShell, run `powercfg /requests`. Then stop it with X.
*Expected:*
- While the macro runs, `powercfg /requests` lists `doppelpad-gui.exe` under `SYSTEM`, and
  the PC doesn't go to sleep.
- After X stops it, the entry is gone and the PC sleeps after its minute.
- While the PC is locked, remapping pauses and the request goes away too.

## 16. Clean install

Do this on a fresh Windows 10 22H2 and on a fresh Windows 11. A virtual machine with a
snapshot to go back to works well. Before you start, check that neither driver is there:
*Installed apps* lists neither "ViGEm Bus Driver" nor "HidHide", and `sc query ViGEmBus` and
`sc query HidHide` say that the service does not exist.

**W16.1 Setup.** Run the setup.
*Expected:*
- SmartScreen warns, as the setup is not signed; *More info → Run anyway* goes on. *Record*
  its wording.
- The welcome page says that the setup installs the missing drivers, and their checkbox is
  ticked.
- Both drivers install without windows of their own. Their logs are
  `%TEMP%\doppelpad-ViGEmBus-setup.log` and `%TEMP%\doppelpad-HidHide-setup.log`.
- The finish page offers to restart Windows.
- *Installed apps* lists Doppelpad, "ViGEm Bus Driver" and "HidHide".

**W16.2 Before the restart.** Choose to restart later, and start Doppelpad.
*Record:* what `doppelpad doctor` and the app say about the drivers, and whether hiding
works (HidHide needs the restart).

**W16.3 After the restart.** Restart Windows. Connect an Xbox controller, a DualSense or
DualShock 4, and a Switch 2 controller on USB if you have them. Start the app.
*Expected:*
- `doppelpad doctor` says `ok` for ViGEmBus and HidHide and has no `problem:` line.
- The welcome guide shows both drivers as installed and lists the controllers. A Switch 2
  controller works without installing anything: Windows binds WinUSB by itself.
- Choosing a virtual Xbox controller in the guide creates a game with a config in slot 1.
  `joy.cpl` then shows the virtual controller and no longer the physical one.

**W16.4 Setup again.** Run the same setup once more.
*Expected:* the welcome page says "ViGEmBus and HidHide are already installed", and the
drivers are left alone.

**W16.5 Silent and without drivers** (optional, from a fresh snapshot). Run the setup with `/S`;
on another fresh snapshot, with `/NODRIVERS`.
*Expected:* `/S` installs both drivers and does not restart Windows. `/NODRIVERS` installs
neither, and the app's driver warnings offer the downloads.

**W16.6 Uninstalling while a controller is hidden.** With pad-a.toml in slot 1 hiding a
controller, uninstall Doppelpad from *Installed apps*, with *Delete the application data*
ticked. The uninstaller asks to close Doppelpad.
*Expected:*
- Afterwards `joy.cpl` shows the physical controller.
- `HidHideCLI.exe --dev-list` lists no device that Doppelpad hid, and `--app-list` no
  Doppelpad program.
- "ViGEm Bus Driver" and "HidHide" stay installed.
- `%APPDATA%\doppelpad` and `%LOCALAPPDATA%\doppelpad\logs` are gone, and so is the *Start
  with Windows* entry.
- See [K5](#known-issues).

**W16.7 Uninstalling after a crash.** Install again. With a controller hidden, end
`doppelpad-gui.exe` in Task Manager, then uninstall.
*Record:* whether the controller stays hidden ([K5](#known-issues)).

**W16.8 Uninstalling with the daemon running.** Install again, quit the app, and run
`doppelpad daemon --profile pad-a.toml`. Uninstall.
*Expected:* the uninstaller stops the daemon first (`doppelpad shutdown`), and the controller
shows again in `joy.cpl`.

## Known issues

Found in the code while writing this checklist. The checks named will show what happens;
what to do about them is for the maintainer.

- **K1, the Steam Controller on its receiver cannot be turned off** (W5.1, W6.6). Its driver
  has the off command (`steam.rs`), but the HID backend passes `power_off` only to
  controllers connected over Bluetooth (`hidhost.rs`), and the receiver is a USB device. The
  log says `this controller cannot be turned off`. The docs don't promise it either.
- **K2, the Switch 2 Pro Controller over Bluetooth on Windows cannot be turned off** (W5.1).
  It has no off command, and only Linux drops its link (`switch2gatt.rs`). The HID backend
  still reports success for a Bluetooth controller without an address, so the log says
  `controller turned off` while the controller stays on.
- **K3, the game's light bar over Bluetooth** (W8.6). GUI.md says the game's light bar colour
  reaches "DualShock 4 and DualSense over USB". PROFILES.md says light changes work over USB
  and Bluetooth, and the driver writes the light bar over both (`sony/output.rs`). The check
  settles which, and the docs follow.
- **K4, hiding one Xbox controller hides all of them** (W9.5). XInput cannot tell which device
  a pad is, so hiding one hides every physical Xbox controller (`hidhide.rs`). GUI.md says
  "Each config hides only its own controller".
- **K5, uninstalling while a controller is hidden** (W16.6, W16.7). The uninstaller stops a
  running daemon with `doppelpad shutdown`, which shows its controllers again. A running app
  is closed through Windows' Restart Manager, which ends it by force if it doesn't exit in
  time. The uninstaller undoes nothing in HidHide itself, and the journal of a killed
  Doppelpad is only replayed when Doppelpad starts again. A controller hidden at that moment
  may therefore stay hidden after the uninstall.

## Results

### Test machine

| | Windows 10 | Windows 11 |
|---|---|---|
| Machine (make and model, or virtual machine) | | |
| Windows edition, version and build (`winver`) | | |
| Bluetooth adapter (Device Manager) | | |
| ViGEmBus and HidHide versions (*Installed apps*) | | |
| Steam running | | |
| Doppelpad version, and commit for a build from source | | |
| Date | | |

### Controllers

Columns are the checks. Add a row for every controller and connection you test.

| Controller | Connection | Windows | W1.2 | W1.3 | W1.4 | W1.5 | W12.1 | Notes |
|---|---|---|---|---|---|---|---|---|
| Xbox controller | USB | | | | | | ➖ | |
| Xbox controller | wireless adapter or Bluetooth | | | | | | ➖ | |
| DualShock 4 | USB | | | | | | | |
| DualShock 4 | Bluetooth | | | | | | | |
| DualSense | USB | | | | | | | |
| DualSense | Bluetooth | | | | | | | |
| Switch Pro Controller | USB | | | | | | | |
| Switch Pro Controller | Bluetooth | | | | | | | |
| Joy-Cons | Bluetooth | | | | | | | |
| Joy-Cons in the charging grip | USB | | | | | | | |
| Switch 2 Pro Controller | USB | | | | | | | |
| Switch 2 Pro Controller | Bluetooth | | | | | | | |
| Switch 2 GameCube controller | USB | | | | | | ➖ | |
| Steam Controller | USB | | | | | | | |
| Steam Controller | wireless receiver | | | | | | | |
| Steam Deck | built in | | | | | | | |
| GameCube controller | adapter | | | | | | ➖ | |
| Generic pad: | USB | | | | | | ➖ | |

### Checks

| Check | Result | Notes |
|---|---|---|
| W1.1 Doctor | | |
| W1.2–W1.5 | | see [Controllers](#controllers) |
| W2.1 Light bar | | |
| W2.2 Player lights | | |
| W2.3 Microphone LED | | |
| W2.4 Lights after a reconnect | | |
| W3.1 Effects from the profile | | |
| W3.2 Presets in the app | | |
| W3.3 Leaving | | |
| W4.1 Rumble from a mapping | | |
| W4.2 Rumble from the game | | |
| W4.3 Vibration settings | | |
| W5.1 The command | | |
| W5.2 From the app | | |
| W5.3 Idle timeout | | |
| W6.1 Before Doppelpad | | |
| W6.2 Desktop mode off | | |
| W6.3 Desktop mode back | | |
| W6.4 Two trackpads | | |
| W6.5 Rumble | | |
| W6.6 The wireless receiver | | |
| W6.7 With Steam running | | |
| W7.1 Detection | | |
| W7.2 Buttons and sticks | | |
| W7.3 Rumble | | |
| W7.4 Hiding | | |
| W7.5 WinUSB driver | | |
| W7.6 Dolphin holding it | | |
| W7.7 The adapter unplugged | | |
| W8.1 The virtual controller | | |
| W8.2 Buttons, sticks and triggers | | |
| W8.3 Touchpad | | |
| W8.4 Motion | | |
| W8.5 Rumble from the game | | |
| W8.6 Light bar from the game | | |
| W9.1 Own slots | | |
| W9.2 Switching | | |
| W9.3 The second controller goes away | | |
| W9.4 The first controller goes away | | |
| W9.5 Two Xbox controllers | | |
| W9.6 Groups | | |
| W10.1 Detection | | |
| W10.2 Buttons | | |
| W10.3 Joy-Cons | | |
| W10.4 Rumble | | |
| W10.5 USB stays alive | | |
| W10.6 Switch 2 on USB with Steam | | |
| W10.7 Connecting on Windows 11 | | |
| W10.8 The input rate | | |
| W10.9 Everything else on Windows 11 | | |
| W10.10 The off switch, Windows 10 | | |
| W10.10 The off switch, Windows 11 | | |
| W11.1 Detection | | |
| W11.2 Identifying the buttons | | |
| W11.3 Hats, sliders and many buttons | | |
| W11.4 Hiding and a virtual controller | | |
| W12.1 Raw motion and touch | | see [Controllers](#controllers) |
| W12.2 Gyro mappings and routes | | |
| W12.3 Calibration | | |
| W12.4 Touchpad | | |
| W13.1 Notifications | | |
| W13.2 Crosshair and magnifier | | |
| W13.3 Radial menu | | |
| W13.4 Controller and mappings overlay | | |
| W13.5 Focus | | |
| W13.6 Hotkey rumble in exclusive full screen | | |
| W13.7 Scaling and monitors | | |
| W14.1 Turning it on | | |
| W14.2 An emulator | | |
| W14.3 Slots | | |
| W14.4 Remapping meanwhile | | |
| W14.5 The network | | |
| W15.1 The lock screen | | |
| W15.2 Ctrl+Alt+Del and administrator prompts | | |
| W15.3 The daemon | | |
| W15.4 Keeping awake | | |
| W16.1 Setup, Windows 10 | | |
| W16.1 Setup, Windows 11 | | |
| W16.2 Before the restart | | |
| W16.3 After the restart, Windows 10 | | |
| W16.3 After the restart, Windows 11 | | |
| W16.4 Setup again | | |
| W16.5 Silent and without drivers | | |
| W16.6 Uninstalling while a controller is hidden, Windows 10 | | |
| W16.6 Uninstalling while a controller is hidden, Windows 11 | | |
| W16.7 Uninstalling after a crash | | |
| W16.8 Uninstalling with the daemon running | | |
