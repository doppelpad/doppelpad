# Linux

Doppelpad runs on Linux with only kernel interfaces. No extra drivers are needed:

| Need | Linux backend (`crates/dp-platform/src/linux`) |
|---|---|
| Read controllers | evdev (`/dev/input/event*`). DualShock 4 / DualSense motion sensors and touchpad come from their own event nodes and are merged into the controller. |
| Switch 2 controllers over Bluetooth | BlueZ finds them, Doppelpad connects them itself on an L2CAP socket (see [below](#switch-2-controllers-over-bluetooth)) |
| Hide the physical controller | `EVIOCGRAB` on the controller's event nodes |
| Virtual controller | uinput Xbox 360 pad (`045e:028e`, laid out like the kernel's `xpad` driver, with rumble) |
| Keyboard / mouse output | uinput keyboard + mouse (with high-resolution wheel) |
| Keyboard / mouse capture | `EVIOCGRAB` on every keyboard and mouse, plus one uinput passthrough device each |
| Per-game profiles | X11 / XWayland `_NET_ACTIVE_WINDOW` → `_NET_WM_PID` → `/proc` |

> **Status:** first tested on real hardware on a Steam Deck (SteamOS 3.8, desktop mode) with
> a DualSense and a Switch Pro Controller over USB, a USB keyboard and mouse, and the desktop
> app. A second run on SteamOS 3.9 covered batteries, lights, turning off over Bluetooth,
> keeping awake and the lock screen with a DualSense. A fourth run tried a Switch 2 Pro
> Controller over Bluetooth with the command-line tool: connecting from pairing mode, every
> button, the sticks, triggers and motion, the battery level, player light 1, rumble, turning
> it off, and going away when turned off. The [results](LINUX-TESTING.md#results) list what works and
> what is still open. DualShock 4, Joy-Cons, Switch controllers over Bluetooth, the Switch 2
> Pro Controller in the app, other distributions and Game Mode are untested. Reports from
> Linux users are very welcome.

## Install

Each release has a build for 64-bit Linux, made on Ubuntu 22.04 (it needs glibc 2.35 or
newer):

- `Doppelpad_<version>_amd64.AppImage`: the desktop app, with the libraries it needs
  (WebKitGTK included). Make it executable and start it:
  `chmod +x Doppelpad_*.AppImage && ./Doppelpad_*.AppImage`. It hosts the remapping service
  itself, keeps running in the tray, and updates itself: when a new version is out it asks,
  then replaces its own file and starts again.
- `doppelpad_<version>_linux-x86_64.tar.gz`: the command-line tool `doppelpad` (`doctor`,
  `daemon`, `status`, …), for checking the setup and for running without the app.

Then give your user access as described under [Setup](#setup); on SteamOS nothing needs
setting up. To build from source instead, see [Desktop app](#desktop-app). A build from
source doesn't look for updates.

## Setup

Doppelpad needs read access to `/dev/input/event*` and write access to `/dev/uinput`.
Don't run it as root. Give your user access instead:

1. Create `/etc/udev/rules.d/60-doppelpad.rules`:

   ```text
   # Doppelpad: let members of the "input" group read input devices and create uinput devices.
   KERNEL=="uinput", SUBSYSTEM=="misc", MODE="0660", GROUP="input", OPTIONS+="static_node=uinput"
   KERNEL=="event*", SUBSYSTEM=="input", MODE="0660", GROUP="input"
   # Controllers' light bars and player lights (hid-playstation, hid-sony, hid-nintendo).
   SUBSYSTEM=="leds", ACTION=="add", DRIVERS=="playstation|sony|nintendo", RUN+="/bin/sh -c 'chmod a+w /sys%p/brightness /sys%p/multi_intensity 2>/dev/null'"
   ```

   The last rule, the LED rule, lets every user set these controllers' lights, and nothing
   else.

2. Load uinput now and at every boot, reload the rules, and join the group:

   ```bash
   sudo modprobe uinput
   echo uinput | sudo tee /etc/modules-load.d/uinput.conf
   sudo udevadm control --reload-rules
   sudo udevadm trigger
   sudo usermod -aG input "$USER"
   ```

3. Log out and back in so the new group applies, then run `doppelpad doctor`. Its
   virtual-controller line checks `/dev/uinput`. Device hiding on Linux needs no driver.

Be aware that members of the `input` group can read every keyboard on the system. Every
evdev-based remapper needs this access.

On **SteamOS** you can skip these steps. Steam's udev rules (`60-steam-input.rules`,
`70-steam-jupiter-input.rules`) and systemd's `uaccess` already give the logged-in user
`/dev/uinput` and every input device on USB, which on a Steam Deck includes Bluetooth ones
(its Bluetooth adapter is a USB device). `doppelpad doctor` confirms it. Without the rule,
the user can't read virtual keyboards and mice that other programs create through uinput, so
capture leaves them alone. The nodes that stay unreadable, such as the touchscreen and the
audio jacks, are not needed. SteamOS 3.9 also ships `70-steam-jupiter-leds.rules`, which
gives the `deck` user the brightness and colour of every LED, so controllers' lights need no
rule either. Where that file is missing, put the LED rule in a file of its own under
`/etc/udev/rules.d` and run the two `udevadm` commands.

## Controllers

Every event node with gamepad buttons (`BTN_SOUTH`) and sticks is read as a controller.
This includes xpad, xone, xpadneo, hid-playstation, hid-sony, hid-nintendo and generic HID
pads. Buttons are mapped by position, following `Documentation/input/gamepad.rst`:

- Sony and Nintendo drivers report the face buttons by position. Triangle / Nintendo X is
  `BTN_NORTH`, and Square / Nintendo Y is `BTN_WEST`.
- Xbox-style drivers (xpad and most others) report by label. The left "X" button is
  `BTN_X`, which is the same code as `BTN_NORTH`.

Doppelpad handles both. The Xbox Series Share button (`KEY_RECORD`) and the Switch Capture
button become `Misc`. Back paddles (`BTN_GRIP*` on Elite pads, Fn and paddles on the
DualSense Edge) become the four paddles. The DualSense Create button is `Back`. Its Mute
button never arrives: hid-playstation handles it itself and toggles the microphone LED.

Joy-Cons from hid-nintendo:

- SL and SR, which the driver reports as the shoulder codes of the other side, become
  paddles: left SL upper left, left SR lower left, right SR upper right, right SL lower right.
- A left and a right Joy-Con are combined into one controller automatically, and a Joy-Con
  on its own works sideways. If joycond runs, its "Combined Joy-Cons" device is used instead
  (SL/SR arrive as `BTN_TRIGGER_HAPPY1`–`4`) and nothing is paired twice.
- The IMU node reports in Nintendo's axes (x toward the triggers, y left, z up); Doppelpad
  converts them to its own (pitch, yaw, roll).

Over USB, Switch 2 controllers are not supported on Linux yet: without a kernel driver they
stay silent until their USB vendor interface is initialised, which only the Windows and macOS
backends do today. On SteamOS 3.8, Steam doesn't initialise a Switch 2 Pro Controller either.
Over Bluetooth, Doppelpad connects them itself (see
[below](#switch-2-controllers-over-bluetooth)).

hid-playstation and hid-nintendo create extra nodes, "… Motion Sensors" / "(IMU)" and
"… Touchpad", on the same parent device. Doppelpad attaches them to the controller:

- gyro in deg/s and accelerometer in g, scaled by each axis' resolution,
- two touch points,
- touchpad click.

Rumble uses `FF_RUMBLE`.

evdev has no interface for batteries, lights or turning a controller off. For those Doppelpad
goes to the controller's HID device in sysfs, and to BlueZ:

- **Battery:** the level and charging state the driver reports under `power_supply/`.
  hid-playstation reports percent, hid-nintendo steps (shown as 0, 10, 30, 60 and 100 %, as
  on Windows). Wired pads have none.
- **Light bar and player lights:** through the LED devices the driver registers:
  `…:rgb:indicator` and `…:white:player-1` to `player-5` on a DualSense, `…:red`, `…:green`
  and `…:blue` on a DualShock 4, `…:green:player-1` to `player-4` on Switch controllers.
  Writing them needs the LED rule from [Setup](#setup). Without it the app shows a warning.
  Steam sets the lights of a controller it takes over just after it connects, through
  hidraw and past the driver, so Doppelpad writes them again for a few seconds after a
  controller connects.
- **Turning off** (the `power_off` command, and controllers turned off after the idle time):
  Doppelpad ends the Bluetooth connection through BlueZ (`busctl`), and DualShock 4,
  DualSense and Switch controllers switch off. A controller on its cable stays on. The
  adapter is taken from where the HID device sits in sysfs. With BlueZ's `UserspaceHID`, as
  on SteamOS 3.9, it sits under `uhid` instead, and the first adapter (`hci0`) is assumed.
- **Not available:** DualSense trigger effects, and its microphone LED, which hid-playstation
  drives from the mute button itself.

Controller ids look like `evdev:054c:0ce6:a0:b1:c2:d3:e4:f5`: vendor and product id, then the
Bluetooth address, the USB port or the device node, so a controller keeps its id when it
reconnects. Its event node is what gets hidden.

### Switch 2 controllers over Bluetooth

Over Bluetooth, Switch 2 controllers speak Bluetooth Low Energy with Nintendo's own protocol
rather than HID, and drop the link when a computer tries to pair them the standard way. Don't
pair them in the desktop's Bluetooth settings (remove one that is listed there); Doppelpad
connects them itself:

1. Bluetooth must be on. `doppelpad doctor` and Settings › Drivers in the app show whether it
   is.
2. Hold the small pairing button on top of the controller until its lights run. Doppelpad
   connects it within a few seconds and lights player 1.

It works without pairing, so the controller stays paired with its console, and has to be put
into pairing mode again after it was off. A controller that only looks for its console (a
button pressed while it is off) is left alone, so Doppelpad never takes it from a Switch 2
nearby. Nothing needs setting up: no udev rule, no root.

How it works: while Doppelpad runs, it keeps a Bluetooth LE discovery of its own running in
BlueZ, which reports the controllers' advertisements. For a controller in pairing mode,
Doppelpad opens an L2CAP socket on its Attribute Protocol channel (which needs no privileges),
finds its characteristics and switches its input on, the way Windows and macOS do it.
BlueZ's own `Connect` cannot be used: its GATT client never finishes reading these
controllers, whose Generic Attribute service is empty, and stops after 30 s without a
single characteristic. Closing the socket ends the link, so a controller is let go when
Doppelpad quits; in the test it then turned off.

- Only the Pro Controller for now; the GameCube controller and the Joy-Con 2 are not
  connected.
- BlueZ's default connection interval sets the pace, which the controller never asks to
  change: about 20 reports a second, 17 to 19 after the first 20 seconds or so (measured on a
  Steam Deck), against 250 over USB on other systems. That is enough for menus and slower
  games, but less smooth in fast ones.
- Input, motion, the battery level, player lights and rumble work as on Windows and macOS.
  No other program sees the controller, so there is nothing to hide.
- A controller that is turned off is let go within half a second.
- Turning it off (the `power_off` command, and the idle time) ends its link, as quitting
  Doppelpad does, and the controller switches off (tried).
- Steam keeps a discovery of its own running on SteamOS. While one runs, the adapter may hold
  back the advertisement the connection waits for, so Doppelpad starts the scan over when the
  controller has not connected after 2 seconds; connecting took 0.4 s in the tests.

### Hiding

While a profile with `hide_physical` drives a virtual pad, Doppelpad grabs the
controller's event nodes. It grabs them through its own open file, so Doppelpad keeps
reading them while games, Steam and the desktop no longer receive their events. Stopping
the profile, or quitting Doppelpad for any reason, releases the grab.

`EVIOCGRAB` only covers evdev. Programs that open the controller through **hidraw** still see
it:

- **Steam** reads every controller Steam Input drives through hidraw and gives games its
  own virtual pad for it. With Steam Input on, a game therefore gets the controller twice:
  once from Steam, not remapped, and once from Doppelpad's virtual pad, which Steam wraps
  as well. Turn Steam Input off for the game (game *Properties → Controller → Disable Steam
  Input*). The game then uses Doppelpad's pad and nothing else of that controller. This was
  tested with a native game on a Steam Deck.
- SDL's HIDAPI drivers open PlayStation, Nintendo and some Xbox pads through hidraw,
  provided hidraw is accessible (Steam's udev rules make it so). If a game still sees two
  controllers, start it with `SDL_JOYSTICK_HIDAPI=0`.

## Steam

Steam makes a virtual pad (`28de:11ff`, named "Microsoft X-Box 360 pad N") for every
controller it drives, Doppelpad's own virtual pad included. Doppelpad lists them, but never
picks one on its own, and the app doesn't announce them or give them a tray icon of their
own. To use one, pick it in the app. While Steam runs, that is the only way to the Steam
Deck's own controls (below). Steam's pads have no serial number or port, so their id is the
event node (`evdev:28de:11ff:/dev/input/event10`): after Steam or the computer restarts, a
pick can point to another of Steam's pads, or to none.

While Steam drives a controller it also changes what its evdev nodes deliver:

- Steam turns the Switch Pro Controller's IMU off, so its gyro stays at zero.
- SteamOS's `steamos-manager` inhibits the touchpad node of a PlayStation controller that
  Steam uses, so the touchpad sends nothing. To get it back while Steam runs:
  `busctl set-property com.steampowered.SteamOSManager1 /com/steampowered/SteamOSManager1 com.steampowered.SteamOSManager1.RootManager InhibitDs b false`
  (and `b true` to restore it).

**Steam Deck controls.** While Steam runs, they exist only as Steam's virtual pad, with the
Xbox 360 layout: no back buttons, trackpads or gyro. Pick that pad in the app to remap them.
In desktop mode Steam's desktop configuration turns the controls into mouse and keyboard and
its pad stays silent: hold the Menu button (≡) to switch Steam to gamepad mode, and again to
switch back. With Steam closed, hid-steam offers a "Steam Deck" controller and a motion node, but starts in lizard mode, in which the controls
act as keyboard and mouse and the gamepad node stays silent. Hold the Menu button (≡) to
switch to gamepad mode. Doppelpad then reads the buttons, L4/R4/L5/R5 as the four paddles,
the sticks and the gyro. The trackpads, the ··· button and the analog triggers are not read
yet (L2/R2 arrive as buttons), and the left trackpad wrongly moves the d-pad.

## Virtual controller

The virtual pad is named "Microsoft X-Box 360 pad" with USB id `045e:028e`. It has the same
buttons, axis ranges and dpad hat as a wired 360 pad on xpad, so SDL, Wine/Proton and
Steam treat it as an Xbox 360 controller. Rumble that games upload is passed back to the
physical controller.

A virtual **DualShock 4 is not available on Linux yet** (`virtual_pad = "dual_shock4"`
fails with "not supported"). It would need a `uhid` device that speaks the DS4 HID
protocol.

## Keyboard / mouse capture

Profiles that map keyboard or mouse inputs grab every keyboard and relative mouse. Each one
gets a uinput passthrough device named `Doppelpad passthrough: <device>`, which has the
same capabilities. Doppelpad re-injects every event the active profile does not remap
through that device. Remapped keys, buttons, wheel directions or mouse motion are
swallowed.

This works below the display server, so it behaves the same on X11 and Wayland, and games
reading the devices directly don't see swallowed input either.

Before grabbing a device, Doppelpad waits until no key on it is held (up to 5 s), so no key
can get stuck.

Limitations:

- Caps / Num / Scroll Lock LEDs on grabbed keyboards don't update while capture runs. The
  lock state itself still works.
- Touchpads, tablets and touchscreens (absolute pointers) are not captured. Their buttons
  and motion can't be remapped.
- ACPI power / sleep buttons are left alone.

## Per-game profiles

Automatic switching asks X11 for the active window (`_NET_ACTIVE_WINDOW`) and its process
id (`_NET_WM_PID`). It then names the process from `/proc`:

- For Wine / Proton games it uses the Windows executable name (`Game.exe`, read from the
  command line), so `processes = ["Game.exe"]` works for Proton games too.
- For native programs it uses the binary's file name. Names are compared with or without
  `.exe`, so a game listed as `Game.exe` (in an imported Windows profile, say) is found when
  its native build `Game` runs.

On a Wayland desktop this works for XWayland windows, which includes Proton games, as long
as the compositor maintains `_NET_ACTIVE_WINDOW` (KWin and Mutter do). Native Wayland
windows are not detected, because Wayland has no portable "focused window" query.

Profiles live in `~/.config/doppelpad/profiles` (or `$XDG_CONFIG_HOME/doppelpad/profiles`),
unless another folder is named in `profiles-location.txt` next to it (the app's *Settings →
Files* writes it).

## Commands

The power commands run `systemctl poweroff`, `reboot`, `suspend` and `hibernate`, and
`loginctl terminate-session $XDG_SESSION_ID` for signing out; polkit decides whether the
user may do that. `close_window` sends the EWMH `_NET_CLOSE_WINDOW` request for the active
X11 window. The `screenshot` command is not available on Linux yet.

While macros or toggles play input back, Doppelpad takes logind's idle lock
(`systemd-inhibit --what=idle`), so the computer doesn't count as idle and doesn't go to sleep
on its own. Whether the desktop's own power settings follow that lock is up to the desktop.

Remapping pauses while the session is locked, so nothing a controller does reaches the lock
screen. Doppelpad asks logind once a second for the session's `LockedHint` (`busctl`), which
desktops such as KDE Plasma set while their lock screen shows.

## Desktop app

The [AppImage](#install) brings WebKitGTK along, so it runs on SteamOS without a container.
Built from source, the app needs WebKitGTK on the system; on SteamOS that means a container
(see [LINUX-TESTING.md § 9](LINUX-TESTING.md#9-desktop-app)). There, `npx tauri build` in
`gui/` makes the AppImage too (`tauri.linux.conf.json`), and `--no-bundle` only the program.

On a Wayland desktop the app is a native Wayland
window, and the compositor decides where windows go: the OSD and the controller overlay
appear in the middle of the screen instead of the chosen corner. Over a fullscreen game an
overlay takes the focus and then ends up behind the game.

Tray icons go through libappindicator, which cannot remove an icon. The icon of a
controller that went away is hidden, and desktops such as KDE Plasma list it among the
tray's hidden icons until the controller is back or the app quits.

Keeping awake, the lock-screen pause and turning controllers off talk to the system bus. A
container may not share it: the Arch Distrobox container on SteamOS has no `/run/dbus`.
There, start the app with
`DBUS_SYSTEM_BUS_ADDRESS=unix:path=/run/host/run/dbus/system_bus_socket`; otherwise they
fail without a warning.

## Troubleshooting

- **"uinput is not installed or not reachable"**:
  - Check `ls -l /dev/uinput`. It should be `crw-rw---- root input`.
  - Check that `id` lists `input`.
  - Check that `lsmod | grep uinput` shows the module.
- **No controllers listed:**
  - Check `ls -l /dev/input/event*`.
  - `evtest` (as your user) shows whether the controller produces events.
- **Another program holds the device** ("EVIOCGRAB … Device or resource busy"). Only one
  program can grab a device. Stop other remappers (input-remapper, keyd, Steam Input for
  that controller) first.
- **A Switch 2 controller doesn't connect over Bluetooth:**
  - `doppelpad doctor` should say `ok` for Bluetooth: BlueZ (`bluetoothd`) running, with an
    adapter that is on.
  - Remove the controller from the desktop's Bluetooth settings if it was paired there.
  - `DOPPELPAD_LOG=info,dp_platform::linux::ble=debug doppelpad monitor` shows each step.
- **Debug logging:** `doppelpad --log dp_platform=debug run …`, or set
  `DOPPELPAD_LOG=dp_platform=debug`.

## Sources

The backend was written from public kernel interfaces and documentation:

- `Documentation/input/gamepad.rst`
- `include/uapi/linux/input.h`, `input-event-codes.h` and `uinput.h`
- the open-source `xpad`, `hid-playstation` and `hid-nintendo` drivers, for button codes and
  axis ranges, and the names of their batteries and LEDs
- `Documentation/ABI/testing/sysfs-class-power` and `sysfs-class-led-multicolor`
- the D-Bus interfaces of systemd-logind (`org.freedesktop.login1`) and BlueZ
- for Switch 2 controllers over Bluetooth: ndeadly's `switch2_controller_research`
  (`bluetooth_interface.md`), the Attribute Protocol in the Bluetooth Core Specification
  (Vol 3, Part F), and `struct sockaddr_l2` in BlueZ's `lib/l2cap.h`
  (`org.bluez.Device1`)

It uses the MIT/Apache-licensed `evdev` and `x11rb` crates.
