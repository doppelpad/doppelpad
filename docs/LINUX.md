# Linux

Doppelpad runs on Linux with only kernel interfaces. No extra drivers are needed:

| Need | Linux backend (`crates/dp-platform/src/linux`) |
|---|---|
| Read controllers | evdev (`/dev/input/event*`). DualShock 4 / DualSense motion sensors and touchpad come from their own event nodes and are merged into the controller. |
| Hide the physical controller | `EVIOCGRAB` on the controller's event nodes |
| Virtual controller | uinput Xbox 360 pad (`045e:028e`, laid out like the kernel's `xpad` driver, with rumble) |
| Keyboard / mouse output | uinput keyboard + mouse (with high-resolution wheel) |
| Keyboard / mouse capture | `EVIOCGRAB` on every keyboard and mouse, plus one uinput passthrough device each |
| Per-game profiles | X11 / XWayland `_NET_ACTIVE_WINDOW` → `_NET_WM_PID` → `/proc` |

> **Status:** the backend compiles and its event mapping is unit-tested, but it has **not
> been tested on real hardware yet**. Reports from Linux users are very welcome.

## Setup

Doppelpad needs read access to `/dev/input/event*` and write access to `/dev/uinput`.
Don't run it as root. Give your user access instead:

1. Create `/etc/udev/rules.d/60-doppelpad.rules`:

   ```text
   # Doppelpad: let members of the "input" group read input devices and create uinput devices.
   KERNEL=="uinput", SUBSYSTEM=="misc", MODE="0660", GROUP="input", OPTIONS+="static_node=uinput"
   KERNEL=="event*", SUBSYSTEM=="input", MODE="0660", GROUP="input"
   ```

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
DualSense Edge) become the four paddles.

Joy-Cons from hid-nintendo:

- SL and SR, which the driver reports as the shoulder codes of the other side, become
  paddles: left SL upper left, left SR lower left, right SR upper right, right SL lower right.
- A left and a right Joy-Con are combined into one controller automatically, and a Joy-Con
  on its own works sideways. If joycond runs, its "Combined Joy-Cons" device is used instead
  (SL/SR arrive as `BTN_TRIGGER_HAPPY1`–`4`) and nothing is paired twice.
- The IMU node reports in Nintendo's axes (x toward the triggers, y left, z up); Doppelpad
  converts them to its own (pitch, yaw, roll).

Switch 2 controllers are not supported on Linux yet: without a kernel driver they stay silent
until their USB vendor interface is initialised, which only the Windows backend does today.

hid-playstation and hid-nintendo create extra nodes, "… Motion Sensors" / "(IMU)" and
"… Touchpad", on the same parent device. Doppelpad attaches them to the controller:

- gyro in deg/s and accelerometer in g, scaled by each axis' resolution,
- two touch points,
- touchpad click.

Rumble uses `FF_RUMBLE`.

Controller ids look like `evdev:/dev/input/event17`, and the device path is what gets hidden.

### Hiding

While a profile with `hide_physical` drives a virtual pad, Doppelpad grabs the
controller's event nodes. It grabs them through its own open file, so Doppelpad keeps
reading them while games, Steam and the desktop no longer receive their events. Stopping
the profile, or quitting Doppelpad for any reason, releases the grab.

`EVIOCGRAB` only covers evdev. Games that open the controller through **hidraw** still see
it. SDL's HIDAPI drivers do that for PlayStation, Nintendo and some Xbox pads, provided
hidraw is accessible (Steam's udev rules make it so). If a game sees two controllers:

- start it with `SDL_JOYSTICK_HIDAPI=0`, or
- turn off Steam Input for that game.

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
- For native programs it uses the binary's file name.

On a Wayland desktop this works for XWayland windows, which includes Proton games, as long
as the compositor maintains `_NET_ACTIVE_WINDOW` (KWin and Mutter do). Native Wayland
windows are not detected, because Wayland has no portable "focused window" query.

Profiles live in `~/.config/doppelpad/profiles` (or `$XDG_CONFIG_HOME/doppelpad/profiles`).

## Commands

The power commands run `systemctl poweroff`, `reboot`, `suspend` and `hibernate`, and
`loginctl terminate-session $XDG_SESSION_ID` for signing out; polkit decides whether the
user may do that. `close_window` sends the EWMH `_NET_CLOSE_WINDOW` request for the active
X11 window. The `screenshot` command is not available on Linux yet.

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
- **Debug logging:** `RUST_LOG=dp_platform=debug doppelpad run …`.

## Sources

The backend was written from public kernel interfaces and documentation:

- `Documentation/input/gamepad.rst`
- `include/uapi/linux/input.h`, `input-event-codes.h` and `uinput.h`
- the open-source `xpad`, `hid-playstation` and `hid-nintendo` drivers, for button codes and
  axis ranges

It uses the MIT/Apache-licensed `evdev` and `x11rb` crates.
