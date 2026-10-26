# Linux hardware test checklist

The Linux backend ([LINUX.md](LINUX.md)) compiles and its event mapping is unit-tested, but
it has not run on real hardware yet. This checklist covers everything that needs a real
machine, real controllers, a keyboard and a mouse. Results go in the
[tables at the end](#results).

A person with the hardware can work through it alone, or together with a coding agent (such
as Claude Code) running on the test machine. In that case the agent runs the commands,
reads logs and fixes bugs, and the person presses buttons and watches the screen. The first
test machine is a Steam Deck in desktop mode, so SteamOS has its own notes. Every check also
works on other distributions.

## How to work through it

- Work on a branch (for example `linux-hardware-test`) and push only that branch. Fixes are
  reviewed and merged from there.
- Go in order. [Setup](#0-setup) and [Basics](#1-basics) must pass before the later sections
  mean anything.
- For each check, run it and have the person do the physical part. Then compare with
  **Expected** and fill in its row under [Results](#results): ✅ works, ⚠️ partly, ❌ fails,
  ➖ skipped (say why). Name the controller and how it was connected (USB, Bluetooth).
- When a check fails, keep the evidence: the output of `doppelpad --log dp_platform=debug …`
  and what `evtest` shows for the device node.
- Fix clear, local bugs right away, one commit per fix. If the bug is in the translation
  code (`padmap.rs`, `kbmmap.rs`, `x360.rs`, `codes.rs`, `procname.rs`), add a unit test
  there. Those modules are tested on every OS, so CI keeps the fix.
- Don't settle design questions alone, such as how to treat Steam's virtual controllers
  ([section 6](#6-steam)). Write down what you saw and the options in the check's notes, and
  leave the decision to the maintainer.
- Before every commit, run what CI runs on Linux, plus formatting and the platform tests,
  and fix new warnings:

  ```bash
  cargo fmt --all --check
  cargo clippy -p dp-platform -p dp-service --all-targets
  cargo test -p dp-core -p dp-config -p dp-ipc -p dp-service -p dp-platform
  ```

- Base fixes only on public sources: kernel documentation and headers, and open-source
  drivers, as listed under [Sources](LINUX.md#sources). See [CLEAN_ROOM.md](CLEAN_ROOM.md).
- House style:
  - Commit subjects are short imperative sentences, for example "Skip Steam's virtual
    controllers".
  - Comments and docs are plain English.
  - Doppelpad is unreleased, so don't add compatibility code for data from older
    development builds.
- When you are done, update whatever the results confirm: the status note in
  [LINUX.md](LINUX.md), the 🧪 marks in the README's platform table, and
  [ROADMAP.md](ROADMAP.md).

> **Getting out of a grab.** Hiding and keyboard/mouse capture grab devices, so a bug can
> leave you without working input. Ctrl+C in Doppelpad's terminal stops it. Grabs and
> virtual devices end when the process exits, even with `kill -9`. For sections 4 to 6, keep
> a second way in: an SSH session from another computer, where `pkill doppelpad` works. On a
> Steam Deck, holding the power button forces it off as a last resort.

## 0. Setup

### Any distribution

1. Install:
   - Rust stable, 1.88 or newer, through rustup
   - a C toolchain and git
   - `evtest`
   - `fftest` (package `linuxconsole` on Arch, `joystick` on Debian and Ubuntu)
2. Build with `cargo build --release -p doppelpad`. The binary is `target/release/doppelpad`.
   Put that folder on `PATH`; the checks call it `doppelpad`.
3. **Before** setting up access, run `doppelpad doctor` once and keep its output for
   [L1.2](#1-basics).
4. Set up access as described in [LINUX.md § Setup](LINUX.md#setup).
5. Create a folder for test profiles outside the repository, for example `~/dp-test/`. The
   sections below give each profile's contents.

### Steam Deck (SteamOS)

Work in desktop mode.

- `evtest` and `fftest` are already installed.
- The SteamOS system partition is read-only, has no compiler, and is replaced by every
  update. Don't unlock it to install packages. Build in Distrobox containers instead
  (SteamOS 3.5 and newer ship Distrobox). A container shares your home folder, so a checkout
  in `~` is visible on both sides, and rustup's toolchains in `~/.rustup` serve every
  container.
  - Build `doppelpad` in Debian 13 (trixie). Its glibc is the one SteamOS 3.8 has (2.41), so
    the binary runs on the host:

    ```bash
    distrobox create --name dph --image docker.io/library/debian:trixie
    distrobox enter dph
    sudo apt-get install build-essential pkg-config rustup
    rustup default stable
    ```

  - Build the desktop app ([section 9](#9-desktop-app)) in Arch, which has a current
    WebKitGTK. Arch's glibc is newer than SteamOS's, so what it builds only runs inside that
    container. Give it its own target folder with `export CARGO_TARGET_DIR=target/arch`:
    build scripts compiled there don't run in the Debian container.

    ```bash
    distrobox create --name dp --image docker.io/library/archlinux:latest
    distrobox enter dp
    sudo pacman -Syu --needed base-devel git rustup
    rustup default stable
    ```

- Run `doppelpad` on the host, where device access has the fewest surprises.
  - If the host refuses the binary with `GLIBC_… not found`, it was built in a container
    with a newer glibc than SteamOS's.
  - If something works on one side but not the other, suspect the container before
    Doppelpad.
  - The Arch container has no `/run/dbus`, so the app can't reach logind or BlueZ from it
    (L8.4, L8.6, L8.7). Start it with
    `DBUS_SYSTEM_BUS_ADDRESS=unix:path=/run/host/run/dbus/system_bus_socket`.
- Access needs no setup on SteamOS: Steam's udev rules already give the logged-in user
  everything Doppelpad needs ([LINUX.md § Setup](LINUX.md#setup)). So step 4 above is only
  a check with `doppelpad doctor`.
- To see the messages for missing access anyway (L1.2, L8.4), hide a device file in a
  namespace of its own, without root. A bind mount of a file only root can read makes it
  unreadable for Doppelpad, for example a controller's node:
  `unshare -r -m sh -c 'mount --bind /etc/shadow /dev/input/event10 && doppelpad doctor'`.
  The same works for `/dev/uinput` and for the `brightness` and `multi_intensity` files of a
  controller's LEDs.
- `sudo` needs a password, which a new Deck doesn't have yet. Set one with `passwd` when you
  need it, for example to turn on SSH with `sudo systemctl enable --now sshd`. A second
  computer can then run commands, and it can stop Doppelpad if input gets stuck.
- On a new Deck, git has no `user.name` and `user.email` yet, and no GitHub login.
- Commands over SSH have no display. For the X11 checks ([section 7](#7-per-game-profiles)
  and `close_window`), take `DISPLAY` and `XAUTHORITY` from the desktop session:
  `tr '\0' '\n' < /proc/$(pgrep -x plasmashell)/environ | grep -E '^(DISPLAY|XAUTHORITY)='`.

Before you start, fill in [Test machine](#test-machine).

## 1. Basics

**L1.1 Unit tests run natively.** Run `cargo test -p dp-platform`. CI only compiles this
crate on Linux, so this is its first native run.
*Expected:* all tests pass.

**L1.2 Messages without access.** This uses the `doctor` output from setup step 3.
- *Expected:* the virtual-controller line names `/dev/uinput`. If a controller's event node
  can't be read, a `problem:` line names the controller and points to the udev rule in
  docs/LINUX.md, and the app shows the same warning. Nothing crashes.
- On SteamOS, Steam's own udev rules may already give the logged-in user some of this
  access. Note which lines were already fine.

**L1.3 Doctor.** Run `doppelpad doctor` with access set up.
*Expected:*
- The first line names the Doppelpad version, the distribution and the kernel.
- The virtual-controller line says `ok`.
- The settings and profiles folders are under `~/.config/doppelpad`.
- There are no `problem:` lines, and the connected controllers are listed.

The labels still name the Windows drivers ([K1](#known-issues)).

**L1.4 Daemon and IPC.**
1. In one terminal, run `doppelpad daemon --no-auto-switch`.
2. In a second terminal, run these in order:
   - `doppelpad status`
   - `doppelpad devices`
   - `doppelpad apply ~/dp-test/virtual.toml` (the profile from
     [section 3](#3-virtual-controller-and-rumble))
   - `doppelpad status`
   - `doppelpad remap off`
   - `doppelpad remap on`
   - `doppelpad stop`
   - `doppelpad shutdown`
3. Start the daemon again, end it with `kill -9`, and start it once more.

*Expected:*
- Every command answers.
- `status` shows the profile and the virtual pad while one is applied.
- `shutdown` ends the daemon.
- The daemon starts again without complaining about a leftover socket, also after
  `kill -9`.

## 2. Controllers

Do L2.1 to L2.5 for every controller you have and record them in the
[Controllers](#controllers) table. To find a node's kernel driver, run
`basename $(readlink -f /sys/class/input/eventN/device/device/driver)`.

**L2.1 Detection.** Run `doppelpad devices` (no daemon running).
*Expected:*
- The controller appears with its name and an id `evdev:<vendor>:<product>:<key>`. The key is
  the Bluetooth address, otherwise the USB port, otherwise the event node.
- Each controller appears once, also when it has separate motion and touchpad nodes
  (DualShock 4, DualSense, Switch controllers).

**L2.2 Buttons and axes.** Run `doppelpad monitor` and press everything.
*Expected:*
- Face buttons go by position, whatever is printed on them: bottom `A`, right `B`, left `X`,
  top `Y`. On a Nintendo controller, the bottom "B" shows as `A`.
- Shoulders, stick clicks, `Back`, `Start`, `Guide` and the d-pad each show under their own
  name.
- The Xbox Share button and the Switch Capture button show as `Misc`; the DualSense Create
  button is `Back`. hid-playstation keeps the DualSense Mute button to itself (it toggles
  the microphone LED), so it never arrives. Back paddles show as `PaddleUL` … `PaddleLR`.
- Sticks go from −1 to +1, with up and right positive, and rest near 0.
- Triggers go from 0 to 1. On Switch controllers they are digital: 0 or 1.
- No control is stuck, missing or doubled.

**L2.3 Motion and touchpad.** For DualShock 4, DualSense, Switch Pro and Joy-Con, watch
`monitor`.
*Expected:*
- `gyro(x,y,z)` is in °/s and rests near 0.
- Nose up gives positive x, and turning left while flat gives positive y. Rolling moves z.
  A brisk turn reaches a few hundred.
- `touch(x,y)` runs from 0 to 1, with the origin at the top left. A touchpad click shows as
  `Touchpad`.
- Motion and touch belong to the same entry as the buttons, not to a second controller.

**L2.4 Joy-Cons.**
*Expected:*
- A single Joy-Con works sideways.
- A left and a right Joy-Con become one controller. SL and SR become paddles as described in
  [LINUX.md](LINUX.md#controllers).
- If joycond runs (`systemctl status joycond`), its combined device is used and nothing
  appears twice.

**L2.5 Reconnect.** Turn the controller off and on (Bluetooth), or unplug it and plug it back
in (USB). Do this while `monitor` runs, and again during `doppelpad run`.
*Expected:*
- The log shows `evdev controller disconnected` and `connected`.
- The controller comes back within a few seconds with the same id, and no duplicate entry is
  left behind.
- A running profile picks it up again.

**L2.6 The Steam Deck's own controls, with Steam closed.** While Steam runs, it reads the
Deck's controls itself, and the kernel's gamepad device for them can be missing.
1. Exit Steam completely (Steam menu → Exit).
2. Run `doppelpad devices`, then `doppelpad monitor`. Without Steam, the trackpads may stop
   acting as a mouse; use the touchscreen or an external mouse.

*Expected:* a "Steam Deck" controller (`28de:1205`).

Record which of these work:
- the buttons
- L4, R4, L5, R5 (they should become `PaddleUL`, `PaddleUR`, `PaddleLL`, `PaddleLR`, as in
  [PROFILES.md](PROFILES.md))
- the trackpads and the gyro

Native Steam Deck support on Linux is still open in the roadmap, so this is groundwork. For
each control that doesn't arrive, note what `evtest` shows. Starting Steam again, or going
back to Game Mode, restores the Deck's normal controls.

**L2.7 Battery.** Look at each wireless controller's battery in the app and the tray. Then
plug it in.
*Expected:*
- DualSense and DualShock 4 show a percentage, Switch controllers one of 0, 10, 30, 60 and
  100 %, close to `cat /sys/class/power_supply/*/capacity*`.
- On the cable the battery shows as charging. Wired pads show none.

## 3. Virtual controller and rumble

`~/dp-test/virtual.toml`:

```toml
name = "Linux test: virtual pad"

[output]
virtual_pad = "xbox360"
hide_physical = false

# A arrives as B, so remapping is visible on the virtual pad.
[[mappings]]
trigger = "A"
actions = [{ pad = "B" }]

# LB rumbles the physical controller for 300 ms.
[[mappings]]
trigger = "LB"
actions = [{ rumble = 300, low = 1.0, high = 1.0 }]
```

**L3.1 The virtual pad exists.** Run `doppelpad run ~/dp-test/virtual.toml`.
*Expected:* `/proc/bus/input/devices` lists "Microsoft X-Box 360 pad" with `Vendor=045e
Product=028e` and a `Phys` that starts with `doppelpad/x360-`. Note its `eventN` from the
`Handlers=` line.

**L3.2 Its input.** Run `evtest` on the virtual pad's node.
*Expected:*
- The physical controls are mirrored, and A arrives as B (`BTN_EAST`).
- Sticks go from −32768 to 32767, and up is negative on `ABS_Y`, as with xpad.
- Triggers are `ABS_Z` and `ABS_RZ`, from 0 to 255.
- The d-pad is `ABS_HAT0X` and `ABS_HAT0Y`.
- Nothing is sent while the controller rests.

**L3.3 Doppelpad ignores its own pad.**
*Expected:* `doppelpad devices`, run in a second terminal, doesn't list the virtual pad. One
press gives exactly one press on the virtual pad, with no echo.

**L3.4 Games see an Xbox 360 controller.**
*Expected:* Steam (Settings → Controller) lists it as an Xbox 360 controller, and a Proton or
native SDL game reacts to it. How Steam then behaves is [section 6](#6-steam).

**L3.5 Game rumble reaches the controller.** Run `fftest` on the virtual pad's node and play
the strong and weak rumble effects. `fftest` reports errors for its other effect types, which
is expected because the pad only has `FF_RUMBLE`. Then try a game with rumble.
*Expected:*
- The physical controller rumbles for the length of each effect: strong is the
  low-frequency motor, weak the high-frequency one.
- The motors stop afterwards.

**L3.6 Rumble from a mapping.**
*Expected:* pressing LB rumbles the controller for 300 ms.

**L3.7 Clean stop.** Stop the profile with Ctrl+C, then run it again and end it with
`kill -9` from another terminal.
*Expected:* in both cases the virtual pad disappears from `/proc/bus/input/devices` and the
motors stop.

## 4. Hiding the physical controller

`~/dp-test/hidden.toml` is `virtual.toml` with `hide_physical = true`.

**L4.1 Grab.** Run `doppelpad run ~/dp-test/hidden.toml`, then run `evtest` on the physical
controller's node.
*Expected:*
- `evtest` reports that another process grabbed the device. The same goes for the
  controller's motion and touchpad nodes.
- The virtual pad still gets all input.

**L4.2 Release.** Stop with Ctrl+C, and again with `kill -9`.
*Expected:* in both cases, `evtest` on the physical node gets events again.

**L4.3 Reconnect while hidden.**
*Expected:* a controller that reconnects while `hidden.toml` runs is grabbed again within a
few seconds.

**L4.4 hidraw.** Turn Steam Input off for the game (section 6 covers Steam itself). Use a
PlayStation or Nintendo controller.
- Check whether the game sees one controller or two.
- Then add `SDL_JOYSTICK_HIDAPI=0 %command%` to the game's launch options and check again.

*Expected,* as described in [LINUX.md](LINUX.md#hiding): two controllers without the
variable, one with it. Record which games do this.

## 5. Keyboard and mouse

This section needs a USB or Bluetooth keyboard and mouse. On a Deck, the built-in controls
reach the desktop only through Steam (L5.8). Keep the second way in from the
[box above](#how-to-work-through-it) ready.

**L5.1 Output.** Run `doppelpad run profiles/examples/controller-desktop.toml` with a
controller.
*Expected:*
- A "Doppelpad keyboard and mouse" device appears.
- The right stick moves the cursor smoothly, and RT and LT click.
- A is Enter, B is Escape, and the d-pad sends arrow keys.
- The left stick scrolls, faster when pushed all the way. Scrolling is smooth in a browser,
  which needs the high-resolution wheel.
- RB opens the window switcher (Alt+Tab), and Start opens the application launcher.
- A long press on Back tries to start `osk.exe`. That is Windows-only, so expect an error in
  the log and no crash.

**L5.2 Capture.** Run `doppelpad run profiles/examples/keyboard-mouse-to-xbox.toml`. The
profile maps Left Ctrl, so stop it with **Right** Ctrl+C or from another terminal.
*Expected:*
- Every keyboard and mouse gets a "Doppelpad passthrough: <name>" twin in
  `/proc/bus/input/devices`.
- On the virtual pad (`evtest`):
  - WASD moves the left stick, and the mouse moves the right stick.
  - Space is A, and the mouse buttons are the triggers.
  - Wheel up taps Y.
- In a text editor, mapped keys type nothing. Unmapped keys (K, L, digits) type normally.

**L5.3 A key held at start.** Run `sleep 2; doppelpad run …` and hold a letter key until the
profile has started.
*Expected:* capture starts after the key is released (within 5 s), and no key gets stuck.

**L5.4 Release on stop.** Stop with Right Ctrl+C, and once with `pkill -9 doppelpad` (over
SSH).
*Expected:* the keyboard and mouse work directly again, and the passthrough devices are
gone. No key or modifier is stuck.

**L5.5 A keyboard plugged in during capture.**
*Expected:* it is captured within a few seconds and gets its own passthrough device.

**L5.6 What isn't captured.**
*Expected:* the touchscreen, touchpads and the power button keep working during capture.

**L5.7 Lock keys.**
*Expected:* Caps Lock works during capture, but its LED doesn't change, which is a known
limitation. Record what the LED shows after stopping.

**L5.8 The Deck's controls during capture (Steam running).** Steam turns the trackpads and
buttons into its own uinput mouse and keyboard, and capture grabs those too.
*Expected:* the trackpad cursor and the Deck's buttons keep working through the passthrough
devices. Record anything odd.

## 6. Steam

Steam reads controllers through hidraw and creates its own virtual controllers through uinput
(Valve's vendor id is `28de`). Doppelpad's backend skips only the devices it created itself
(`Phys` starting with `doppelpad/`, see `is_own` in
[dev.rs](../crates/dp-platform/src/linux/dev.rs)). Two things follow:
- Steam's virtual controllers look like real ones to Doppelpad.
- `EVIOCGRAB` doesn't hide a controller from Steam, because Steam reads it through hidraw.

SteamOS runs Steam in the background in desktop mode too, so there this is the normal case.
These checks find out what actually happens. Record the results and the options; don't change
the behavior yet.

**L6.1 Inventory.** With Steam running, a controller connected and no profile running, run
`doppelpad devices` and `grep -A5 'Vendor=28de' /proc/bus/input/devices`. Then turn Steam
Input off for that controller type (Steam → Settings → Controller) and compare.
*Record:* every Valve device and which of them Doppelpad lists as controllers.

**L6.2 Feedback loop.** Run `virtual.toml` with a physical controller while Steam runs, and
watch `/proc/bus/input/devices` and the `run` log.
*Record:*
- Does Steam create a virtual controller for Doppelpad's pad?
- Does Doppelpad then pick that one up as a controller?
- With `evtest` on Doppelpad's pad, does one press of A give one B, or does it repeat,
  latch or drift?

*Wanted:* no loop.

**L6.3 What a game sees.** Run a Proton game in desktop mode with a physical controller and
`hidden.toml`. Try three setups:
- (a) Steam Input on (the default)
- (b) Steam Input off for this game (game Properties → Controller)
- (c) Steam Input off for that controller type

For each, record how many controllers the game sees and whether it gets the remapped input.
*Wanted:* one controller, remapped. The setup that works goes into LINUX.md.

**L6.4 The Deck's controls with Steam running.** With Steam running, the Deck's own controls
exist only as Steam's virtual controller, if at all.
*Record:*
- its name and ids
- what `doppelpad monitor` shows, including paddles and gyro
- whether remapping them through Steam's controller is workable, or whether it needs native
  support ([L2.6](#2-controllers))

## 7. Per-game profiles

**L7.1 Session.**
*Record:*
- `echo $XDG_SESSION_TYPE`
- `echo $DISPLAY`
- the desktop and its version (`plasmashell --version`)

**L7.2 Foreground names.** Start `doppelpad daemon`. Then run `sleep 3; doppelpad status` and
switch to another window before it answers. Do this for a terminal, a file manager, a
browser, a Proton game and a native game.
*Expected:* `foreground:` shows the program's name, such as `konsole`, `dolphin` or `firefox`.
For Proton games it shows the Windows executable (`Game.exe`).

**L7.3 Automatic switching.**
1. Put a profile with `processes = ["<the name from L7.2>"]` into
   `~/.config/doppelpad/profiles/`.
2. Run `doppelpad daemon --profile ~/dp-test/virtual.toml`.

*Expected:*
- Switching to the game applies its profile within about a second (log and `status`).
- Switching away goes back to `virtual.toml`.

**L7.4 Native Wayland windows** (Wayland sessions only).
*Expected:* they aren't detected (`foreground: ?`). That is a known limitation. It should stay
quiet, with no repeated errors in the log.

**L7.5 Game Mode** (optional, over SSH). Switch the Deck to Game Mode, start
`doppelpad daemon` over SSH, and start a game.
*Record:*
- what `status` shows as the foreground
- whether a virtual pad reaches the game at all

This is exploratory: nothing here is expected to work yet.

## 8. Commands

`~/dp-test/commands.toml`:

```toml
name = "Linux test: commands"

[output]
virtual_pad = "none"

[[mappings]]
trigger = "Y"
actions = [{ command = "close_window" }]

[[mappings]]
trigger = "X"
actions = [{ command = "screenshot" }]

[[mappings]]
trigger = "A"
actions = [{ command = { launch = { path = "konsole" } } }]

[[mappings]]
trigger = "B"
actions = [{ command = "power_off" }]

[[mappings]]
trigger = ["LB", "RB", "Start"]
actions = [{ command = "sleep" }]
```

**L8.1 `close_window`.** Focus a text editor with unsaved text and press Y.
*Expected:* the editor asks whether to save, as if its close button had been clicked.

**L8.2 `screenshot`.** Press X.
*Expected:* the log says screenshots aren't available on Linux, and nothing crashes.

**L8.3 `launch`.** Press A.
*Expected:* a terminal opens.

**L8.4 Turning off and lights** ([K2](#known-issues)).
1. Press B on a controller connected over Bluetooth, then on one on its cable.
2. Run a profile with
   `lightbar = { mode = "breath", colors = ["#14b8a6", "#8b5cf6"], cycle = true }` on a
   DualSense or DualShock 4: first without the LED rule from
   [LINUX.md § Setup](LINUX.md#setup), then with it (reconnect the controller after adding
   the rule). SteamOS 3.9 makes the LEDs writable without the rule; simulate its absence as
   described under [Steam Deck](#steam-deck-steamos).
3. In the app, turn on the player lights (*Settings → Controllers → Controller lights*) and
   switch slots, on a DualSense and on a Switch controller.
4. With Steam running, unplug the controller and plug it back in.

*Expected:*
- Over Bluetooth the controller switches off. On its cable the log says that it cannot be
  turned off.
- Without the LED rule the app warns that the lights cannot be set. With it the light bar
  breathes smoothly, one colour per breath, and the player lights show the slot's number
  as on Windows: on a DualSense from the centre outwards, on a Switch controller filling up
  from the first light.
- After reconnecting, the player lights may flash and go dark as Steam takes the controller
  over, but they are back within a few seconds.

**L8.5 `sleep`.** Ask the person first. Then press LB+RB+Start.
*Expected:*
- The machine suspends.
- After waking, the controllers come back (a Bluetooth controller may need a button press),
  and the profile keeps working.

**L8.6 Keeping awake.** Turn on *Keep the PC awake while a mapping plays input* in the app's
settings. Start a mapping that keeps playing input, such as a toggle or a repeating macro,
and run `systemd-inhibit --list`. Stop it, and once more end Doppelpad with `kill -9` while it
plays.
*Expected:* a Doppelpad `idle` lock is listed while the input plays, and is gone after it
stops and after `kill -9`.

**L8.7 The lock screen.** With a profile running, lock the session (Meta+L on KDE).
*Expected:* the log says `the user's desktop changed locked=true`, and remapping pauses: a
mapped key doesn't type into the lock screen. After unlocking, remapping resumes.

**L8.8 Other power commands** (`shutdown`, `restart`, `log_off`, `hibernate`). These are
optional. Do them last, and only with the person's OK.
*Expected:* each one runs, or polkit refuses with a message in the log.
- On a Deck, `log_off` leaves desktop mode for Game Mode.
- Hibernation is usually not set up.

## 9. Desktop app

A smoke test of the app built from source (L9.1–L9.7), and of the AppImage the release
workflow builds (L9.8).

1. Install Tauri's Linux dependencies. On Arch (in the container on SteamOS):
   `sudo pacman -S --needed webkit2gtk-4.1 gtk3 gtk-layer-shell libappindicator-gtk3 librsvg nodejs npm`.
2. Build:

   ```bash
   cd gui
   npm ci
   npx tauri build --no-bundle
   ```

   `--no-bundle` leaves out the AppImage (`tauri.linux.conf.json`), which these checks don't
   need.
3. The binary is `target/release/doppelpad-gui`.
   - On SteamOS, run it inside the container, because it needs the container's WebKitGTK.
   - Don't run it while `doppelpad daemon` runs: the app hosts the service itself.

**L9.1 Build.**
*Expected:* it builds without errors.

**L9.2 Window.**
*Expected:*
- The window opens with the app's own minimize, maximize and close buttons.
- Dragging the header moves the window, and double-clicking it maximizes the window.

**L9.3 Controller view.**
*Expected:* connected controllers are listed, and live input moves the controller drawing.

**L9.4 Apply to a slot.** Apply a profile to slot 1.
*Expected:*
- The virtual pad appears as in L3.1, and remapping works.
- `doppelpad status` in a terminal reaches the app's service.

**L9.5 Tray.**
*Expected:*
- Closing the window leaves remapping running, with an icon in the system tray.
- The tray menu works.
- Quitting from the tray releases every grab and virtual device.

**L9.6 Log file.**
*Expected:* the log is in `~/.local/state/doppelpad/logs`.

**L9.7 Overlays.** Try a crosshair and the pad overlay (`pad_overlay`).
*Record:* whether they show above other windows and above a game.

**L9.8 AppImage.** Before a release: on GitHub, *Actions → Release → Run workflow* on the
branch to release, then download the run's artifact `doppelpad-linux`. After a tag: the
AppImage and the `.tar.gz` of the draft release. Quit every other Doppelpad, then on the
host, outside any container: `chmod +x Doppelpad_*.AppImage && ./Doppelpad_*.AppImage`.
*Expected:*
- It starts without a container and without `DBUS_SYSTEM_BUS_ADDRESS`: the window,
  controllers in the device bar, the tray icon.
- L9.3–L9.5 hold as in the container. Keeping awake and the lock-screen pause work (L8.6,
  L8.7).
- Update check (*Settings → General → Updates → Check now*): as long as the newest
  published release has no AppImage, it says that the release has no `linux-x86_64` build.
  Once one has, it says the app is up to date, or offers the newer version, which replaces
  the AppImage file and starts again.
- `doppelpad doctor` from the `.tar.gz` runs on the host.
- `Doppelpad_*_libraries.txt` lists the libraries in the AppImage, each with a package,
  followed by each package's copyright file and the license texts they name. A library
  under `-` (no package) or a package whose licenses name only the GPL, not the LGPL, needs
  a look before the release. `libtiff5` and `libjbig0` are not in it (K13).
- `Doppelpad_*_gpl_sources.tar` holds Ubuntu's source package of libayatana-indicator, of
  the version in the list.

## Known issues

Found while writing this checklist, and fixed in the first run on a Steam Deck:

- **K1:** `doppelpad doctor` labelled its driver lines "ViGEmBus" and "HidHide" on Linux too.
  It now names uinput and `EVIOCGRAB`.
- **K2:** the evdev backend has no controller power-off, light bar, player LEDs or DualSense
  trigger effects. These use the do-nothing defaults of `GamepadBackend` in
  `crates/dp-platform/src/lib.rs`. LINUX.md says so now.
  *After the run:* batteries, the light bar and player lights now go through sysfs, and
  turning off over Bluetooth through BlueZ (L2.7, L8.4). Trigger effects and the DualSense's
  microphone LED stay unavailable. *Second run:* tested with a DualSense; Switch controllers
  not yet.

Found in the first run and left for the maintainer, with what was seen:

- **K3, Steam's virtual controllers.** Steam makes a "Microsoft X-Box 360 pad N" (`28de:11ff`,
  no `phys`, no `uniq`) for every controller it drives, Doppelpad's virtual pad included.
  Doppelpad lists all of them as controllers, and the app's notices announce each
  controller twice when it connects or goes away. "First free controller" picks by node
  path, so it can be one of Steam's mirrors. Options: skip `28de:11ff` (but Steam's pad is
  the only way to the Deck's controls while Steam runs, L6.4), or only skip the mirror of
  our own pad (nothing in the device tells which pad Steam mirrors).
  *Decided after the run:* Steam's pads are still listed and can be picked in the app, but
  are never picked on their own, are not announced, and get a tray icon only while in use.
  *Second run:* checked (L6.1, L6.2, L6.4); their tray icons needed fixes (`8a0d69f`,
  `3acb566`). Still open: their ids come from the event node, so a pick in the app can point
  to another of Steam's pads after Steam restarts. The app drew them as a generic
  controller, because the family came from Valve's vendor id; they are drawn as the Xbox 360
  pad they imitate now.
- **K4, Steam and hidraw.** While Steam Input drives a controller, Steam reads it through
  hidraw, so hiding doesn't reach Steam: a game with Steam Input on gets the controller
  twice (L6.3). Steam also turns the Switch Pro Controller's IMU off, and SteamOS's
  `steamos-manager` inhibits the DualSense touchpad node (`InhibitDs`), so neither reaches
  Doppelpad through evdev while Steam drives the controller.
- **K5, Steam Deck controls with Steam closed** (L2.6). The left trackpad (`ABS_HAT0X/Y`) is
  read as the d-pad, the right trackpad (`ABS_HAT1X/Y`), the pad touches (`BTN_THUMB`,
  `BTN_THUMB2`), the ··· button (`BTN_BASE`) and the analog triggers (`ABS_HAT2X/Y`) are not
  read; L2/R2 arrive as digital `BTN_TL2`/`BTN_TR2`.
- **K6, overlays on Wayland** (L9.7). The app runs as a native Wayland window, so the
  compositor ignored overlay positions: the OSD and the controller overlay appeared in the
  middle of the screen, the OSD's window 200 px high instead of 84. Over a fullscreen game
  the overlay took the focus, and then ended up behind the game. Fixed in `81c0d0f`: where
  the compositor supports wlr-layer-shell (KWin does), the overlays are layer surfaces,
  anchored to their corner and never focused; on GNOME, which has none, they stay as they
  were. The controller overlay also shows the default session's controller, not the one
  whose profile toggled it.
- **K7, stick jitter at rest.** DualSense sticks are 8-bit; one step of jitter is 257 units
  on the virtual pad, above its `fuzz` of 16, so it sends 60–100 events a second while the
  sticks rest (L3.2).
- **K8, Switch 2 controllers over USB** stayed unsupported, as documented: neither Doppelpad
  nor Steam initialised a Switch 2 Pro Controller on SteamOS 3.8, and not even its hidraw node
  sent anything. *Fifth run:* the HID backend, which runs on Linux since the fourth run, now
  opens their hidraw nodes and starts them over their command interface through `nusb`, with
  the driver of Windows and macOS.

Found in the second run and left for the maintainer:

- **K9, tray icons can't be removed on Linux** (L9.5). libappindicator 0.9 never frees an
  icon, and tray-icon only makes it passive. Since `3acb566` a controller that goes away
  keeps its hidden icon, which KDE Plasma lists among its hidden icons until the controller
  is back or the app quits. Removing it would need a change in libappindicator or tray-icon.
- **K10, no system bus, no warning.** When `systemd-inhibit` and `busctl` can't reach the
  system bus, as in the Arch container without `DBUS_SYSTEM_BUS_ADDRESS`, keeping awake, the
  lock-screen pause and turning off fail silently: the inhibitor exits at once, and the lock
  watch gives up with a debug line. A warning in the app would help.
  *Fixed after the fifth run:* the driver check, every 5 s, also pings logind on the system
  bus; when the bus or logind does not answer, the app shows a warning and `doppelpad doctor`
  a problem saying what does not work. Checked with the command-line tool in the Arch
  container, with and without `DBUS_SYSTEM_BUS_ADDRESS`.
- **K11, Steam sets controllers' lights.** Steam writes a controller's lights through hidraw
  when it takes the controller over, past the driver. Doppelpad now writes its own again for
  6 s after a controller connects (`a2cc63a`). Whether Steam sets them again later, for
  example when a game starts, was not checked.
- **K12, update check.** The app warns at every start that no `linux-x86_64` build is in the
  update response. Without a Linux release it could stay quiet.
  *Decided after the run:* releases get an AppImage and a `linux-x86_64` entry in
  `latest.json`, and only the AppImage looks for updates; a build from source doesn't (L9.8).

Found in the third run and left for the maintainer:

- **K13, GPL libraries in the AppImage** (L9.8). Two of the libraries the AppImage carries
  are under the GPL alone, not the LGPL: `libayatana-indicator3-7` (GPL-3), which
  `libayatana-appindicator3` loads for the tray icon, and `libjbig0` (GPL-2+), which only
  gdk-pixbuf's TIFF loader needs (`libpixbufloader-tiff.so` → `libtiff5` → `libjbig0`).
  Leaving the TIFF loader out of the AppImage would drop `libtiff5` and `libjbig0`. The
  other GPL licenses in the list cover Debian's packaging files, documentation or are one
  choice of several.
  *Decided after the run:* the release workflow leaves the TIFF loader out. The AppImage as
  a whole is distributed under the GPL version 3 (THIRD_PARTY_NOTICES.md), with Ubuntu's
  source package of libayatana-indicator beside it. Checked in the AppImage built from
  `e7acb0a`: the source package matches the version in the library list and its checksums.

## Results

### Test machine

| | First run | Second run | Third run | Fourth run | Fifth run |
|---|---|---|---|---|---|
| Machine | Steam Deck LCD ("Jupiter"), desktop mode | the same | the same | the same (Bluetooth: Realtek `13d3:3553`, BlueZ 5.87) | the same |
| Distribution (`VERSION_ID` and `BUILD_ID` in `/etc/os-release`) | SteamOS 3.8.28 (build 20260922.1) | SteamOS 3.9.5 (build 20261023.100) | the same | the same | the same |
| Kernel (`uname -r`) | 6.18.50-valve2-1-neptune-618-gc7289a96b14d | 7.2.7-valve2-2-neptune-72-g31504582a073 | the same | the same | the same |
| Desktop and session type | KDE Plasma 6.4.3, Wayland (`DISPLAY=:0` for XWayland) | KDE Plasma 6.7.3, Wayland | the same | the same | the same |
| Steam running | yes, except for L2.6 and the Switch Pro's L2.3 | yes | yes | yes | yes |
| Doppelpad commit (`git rev-parse --short HEAD`) | `ca8c455` plus the fixes on `linux-hardware-test` | `989add8` plus the fixes on `linux-device-test` | the release workflow's AppImage from `4b7548d`, then from `72f0671` on `linux-appimage-test` | `linux-switch2-ble`: the command-line tool, the hardware tests in `linux/ble.rs`, and the app built in the Arch container | `linux-switch2-usb`: the command-line tool, the hardware test in `linux/hid.rs`, and the app built in the Arch container |
| Date | 2026-10-23 | 2026-10-24 | 2026-10-24 | 2026-10-26 | 2026-10-26 |

The CLI was built in a Debian 13 container and run on the host; the app was built and run in
an Arch container ([setup](#steam-deck-steamos)). Checks marked *fake pad* were driven by a
uinput test gamepad (`045e:028e`, with `FF_RUMBLE`) instead of hands; *fixed* names the
commit on `linux-hardware-test`, `linux-device-test` or `linux-appimage-test` without which
the check failed.

The second run tested what changed after the first: batteries, lights, turning off, keeping
awake, the lock screen and Steam's virtual pads (K3), with a DualSense over USB and
Bluetooth. After the SteamOS update it also ran L1.1–L1.4, L3.1–L3.3, L4.1, L4.2, L5.2 and
L5.4 again, all ✅.

The third run tested what the release workflow builds for Linux (L9.8): the artifact
`doppelpad-linux` of a *Run workflow* on `main`, with a DualSense over Bluetooth. On the way
it found that a stick's whole-stick choices in the editor (mouse, WASD, arrow keys, flick
stick) did nothing, on every OS; fixed in `6678de4`.

The fourth run tried the Switch 2 Pro Controller over Bluetooth with `doppelpad monitor` and
the hardware test `linux::ble::tests::switch2_pro_over_bluetooth`. On the way it found that
BlueZ's `Connect` cannot be used: the link came up, but BlueZ's GATT client waited 30 s for an
answer and then listed no characteristic, while the controller ran its lights until it gave
up. Doppelpad now speaks ATT itself on an L2CAP socket, which found everything in 2.5 s. It
also found that connecting while Steam's discovery runs could time out, twice in a row, until
Doppelpad started the scan over after the connection attempt (connected in 0.4 s).

The fifth run tried the Switch 2 Pro Controller on USB with `doppelpad monitor`, `doppelpad
run` and the hardware test `linux::hid::tests::switch2_on_usb`. Its hidraw node and its USB
device node were open to the `deck` user, and nothing held its command interface, Steam
included.

### Controllers

Driver = kernel driver of the gamepad node. Columns are the checks.

| Controller | Connection | Driver | L2.1 | L2.2 | L2.3 | L2.5 | L2.7 | L3.5 | L4.1 | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| DualSense `054c:0ce6` | Bluetooth | playstation | ✅ | ➖ | ➖ | ✅ | ✅ | ➖ | ➖ | Second run. Its id is its Bluetooth address, which hid-playstation also gives it on USB, so switching between USB and Bluetooth keeps the id and the profile. The HID device sits under uhid (BlueZ `UserspaceHID`). |
| DualSense `054c:0ce6` | USB | playstation | ✅ | ⚠️ | ⚠️ | ✅ | ✅ | ✅ | ✅ | Mute never arrives (see L2.2). Gyro signs right. Touchpad: phantom touch at (0,0) fixed in `020954d`; with Steam running, `steamos-manager` inhibits the touchpad node (K4), with `InhibitDs` off touch and click work. L3.5: rumble kept running after a stop, fixed in `fcbeaf3` and measured with the controller's accelerometer. |
| Switch Pro Controller `057e:2009` | USB | nintendo | ✅ | ✅ | ⚠️ | ➖ | ➖ | ➖ | ✅ | Capture is `Misc`, ZL/ZR digital. IMU silent while Steam drives the controller (K4); with Steam closed, gravity confirms the axes Doppelpad assumes. |
| Switch 2 Pro Controller `057e:2069` | USB | usbhid (generic), read through hidraw | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ✅ | Skipped with "not supported on Linux yet" in the first run (K8). Fifth run: started in 0.4 s over its command interface, id `hid:057e:2069:<USB port>`; every button, sticks to ±1, ZL/ZR to 1, motion, 250 reports a second, battery 100 % while charging, player light 1 and rumble from the driver, let go 0.3 s after it was unplugged. With `hide_physical` and a virtual pad its generic input node was grabbed (another grab got `EBUSY`), and free again after the profile stopped; A came out as B on the virtual pad and LB rumbled. Steam made one of its Xbox 360 pads for it once Doppelpad had started it (K3, K4). In the app: listed as "Switch 2 Pro Controller (USB)" with its picture and its battery, charging on the cable (L2.7), its buttons and sticks lighting up, and the disconnected and connected notifications at the bottom right when it was unplugged and plugged in again. |
| Switch 2 Pro Controller `057e:2069` | Bluetooth LE | none (Doppelpad's ATT socket) | ✅ | ✅ | ✅ | ✅ | ✅ | ➖ | ➖ | Fourth run. Id `ble:057e:2069:<address>`, as on Windows. Every button (GL/GR as `PaddleUL`/`PaddleUR`, C as `Misc2`, Capture as `Misc`), sticks to ±1, ZL/ZR to 1, motion. Player light 1 and rumble from the driver; about 20 reports a second, 17–19 after the first 20 s. Turned off, it was let go 0.4 s after its last report; Doppelpad quitting ended the link, and so did the backend's `power_off`, after which the controller switched off and stayed away. In the app: Settings › Drivers lists Bluetooth as ok with the pairing hint, the controller shows as "Switch 2 Pro Controller (Bluetooth)" with its picture, buttons and sticks light up, and the battery shows (L2.7). Not in the tray: on KDE its icons have no tooltip at all (the StatusNotifierItem that libayatana-appindicator exports has no `ToolTip`). Killing the app ended the link too. |
| Steam Deck controls `28de:1205` | built in | hid-steam | ⚠️ | ⚠️ | ⚠️ | ➖ | ➖ | ➖ | ➖ | See L2.6 and L6.4. |
| Fake pad `045e:028e` | uinput | — | ✅ | ✅ | ➖ | ✅ | ➖ | ✅ | ✅ | Stable id from `phys`. |

### Checks

| Check | Result | Notes |
|---|---|---|
| L1.1 Unit tests run natively | ✅ | 144 passed (147 with the tests the fixes added); 152 in the second run |
| L1.2 Messages without access | ✅ | Simulated in containers: a missing and an unwritable `/dev/uinput` and an unreadable controller each get their message. On SteamOS nothing needed setting up. The warning used to name the first unreadable node, an HDMI audio jack; fixed in `b2cb3c0`. Second run, simulated with `unshare` and bind mounts: a missing or unwritable `/dev/uinput` warns once, an unreadable controller gets a `problem:` line, and LEDs without write access warn once in the log and in the warnings. |
| L1.3 Doctor | ✅ | Fixed in `098cd6b`: evdev 0.13.2 sends `UI_SET_PHYS` with the wrong size, so no uinput device could be created at all (EINVAL). Labels fixed in `e673a14` (K1). `doctor` printed the uinput warning twice when uinput was missing, because it set up the backends twice; fixed after the run (`06ffaeb`), once in the second run. |
| L1.4 Daemon and IPC | ✅ | Also after `kill -9`. `devices` right after the daemon started could say "no controllers found" until the first scan; the backend now waits for that scan when it starts (`4653bbc`); in the second run `devices` listed every controller at once. |
| L2.1–L2.3, L2.5 | ⚠️ | see [Controllers](#controllers) |
| L2.4 Joy-Cons | ➖ | none at hand |
| L2.6 Steam Deck controls, Steam closed | ⚠️ | A "Steam Deck" controller (`28de:1205`, id from its serial) with its motion node. hid-steam starts in lizard mode (keyboard and mouse) and its gamepad node stays silent until the Menu button (≡) is held. Then buttons, L4/R4/L5/R5 (`PaddleUL`/`UR`/`LL`/`LR`), sticks, d-pad and gyro arrive; trackpads, ··· and analog triggers don't (K5). |
| L2.7 Battery | ✅ | Second run, DualSense: 75 % as in sysfs, charging on its cable and discharging over Bluetooth, in the app and in the tray's tooltip. No Switch controller at hand. |
| L3.1 Virtual pad exists | ✅ | after `098cd6b` |
| L3.2 Virtual pad input | ⚠️ | All controls, ranges and A→B as expected (DualSense, fake pad). At rest the DualSense keeps sending stick events (K7). |
| L3.3 Own pad ignored | ✅ | Steam's mirror of it is listed (K3). |
| L3.4 Games see an Xbox 360 controller | ✅ | Steam lists it as an Xbox 360 Controller; Hollow Knight: Silksong (native) uses it (L6.3). |
| L3.5 Game rumble | ✅ | `fftest` strong and weak, with gain and delay, reach the controller (fake pad); see [Controllers](#controllers). |
| L3.6 Rumble from a mapping | ✅ | Fixed in `fcbeaf3`: a 300 ms rumble lasted until something else wrote to the DualSense (1–9 s). |
| L3.7 Clean stop | ✅ | Ctrl+C and `kill -9`; the kernel stops a playing rumble when the file closes. |
| L4.1 Grab | ✅ | see [Controllers](#controllers); motion and touchpad nodes are grabbed too |
| L4.2 Release | ✅ | profile change, Ctrl+C, `kill -9` |
| L4.3 Reconnect while hidden | ✅ | DualSense and fake pad grabbed again within seconds |
| L4.4 hidraw | ➖ | Silksong opens no hidraw node, so it sees the controller once without `SDL_JOYSTICK_HIDAPI=0` (L6.3 b). No game using SDL's HIDAPI was at hand. |
| L5.1 Keyboard/mouse output | ✅ | fake pad, output read with `evtest --grab`: keys, clicks, smooth cursor, wheel with `REL_WHEEL_HI_RES`, Alt+Tab, Meta; `osk.exe` logs an error |
| L5.2 Capture | ✅ | USB keyboard and mouse: every keyboard and mouse got a passthrough twin, including the Deck's lizard nodes and the HDMI-CEC keyboards; mapped keys type nothing, the others type normally |
| L5.3 Key held at start | ✅ | Fixed in `d27f196`: a key held longer than the 5 s wait stayed pressed for the desktop, which repeated it. |
| L5.4 Release on stop | ✅ | SIGINT and `kill -9`; nothing stuck |
| L5.5 Keyboard plugged in during capture | ✅ | |
| L5.6 What isn't captured | ✅ | touchscreen, power buttons and a Wacom tablet stay untouched |
| L5.7 Lock keys | ➖ | the keyboard has no Caps Lock LED |
| L5.8 Deck controls during capture | ✅ | trackpad cursor and buttons keep working through the passthrough devices |
| L6.1 Inventory | ⚠️ | Steam opens the hidraw nodes of the controllers it drives and makes a `28de:11ff` pad for each, Doppelpad's pad included (K3). SteamOS 3.8's Steam has no switch to turn Steam Input off per controller type. Second run: Steam's pads are listed but never picked on their own, and the app's notices name each controller once. |
| L6.2 Feedback loop | ✅ | No loop: Steam mirrors Doppelpad's pad and Doppelpad lists the mirror, but a session keeps the controller it has; one press gives one press. Second run: the session picks the fake pad, not Steam's pad 0. When the fake pad goes away no controller is in use, instead of Steam's mirror of Doppelpad's pad, and the fake pad is picked again when it is back. |
| L6.3 What a game sees | ⚠️ | Silksong with `hidden.toml` on a Switch Pro Controller. (a) Steam Input on: three pads, the controller twice, once through Steam unremapped (K4); Steam also takes Capture for its screenshots. (b) Steam Input off for the game: it opens Doppelpad's pad and the grabbed (silent) physical node, so one controller, remapped. (c) not available (L6.1). |
| L6.4 Deck controls with Steam running | ✅ | Only as Steam's "Microsoft X-Box 360 pad 0": the Xbox 360 layout, without paddles, trackpads or gyro. Remapping those needs native support, for example through hidraw like the Windows `steam` driver. Second run: picked in the app, it gets a tray icon and moves the controller view once Steam is in gamepad mode (hold ≡); in desktop mode Steam keeps it silent. |
| L7.1 Session | ✅ | Wayland, `DISPLAY=:0`, Plasma 6.4.3 |
| L7.2 Foreground names | ✅ | XWayland windows by name (`kdialog`, `Hollow Knight Silksong`); native Wayland ones `?`. From the containerised app the name was cut to 15 bytes; fixed in `4e651fa`. No Proton game tested. Second run: argv[0] is used only when comm has 15 bytes (`c49a0bf`); from the app, `averyveryverylongname` and `kdialog` came out whole. |
| L7.3 Automatic switching | ✅ | in and out within 0.5 s |
| L7.4 Native Wayland windows | ✅ | `?`, nothing in the log |
| L7.5 Game Mode | ➖ | skipped (optional) |
| L8.1 `close_window` | ✅ | Fixed in `d12b70f`: the window almost never closed, because the connection closed before the X server handled the request. Tested on a dialog, not on an editor with unsaved text. |
| L8.2 `screenshot` | ✅ | |
| L8.3 `launch` | ✅ | |
| L8.4 Turning off and lights | ✅ | Second run, DualSense: over Bluetooth B turns it off (BlueZ; `hci0` is assumed because the HID device sits under uhid); on its cable the log says it cannot be turned off. The light bar breathes smoothly, one colour per breath with `cycle = true`. Player lights 2 and 4 for slot 2; after a reconnect Steam turned them off past the driver, fixed in `a2cc63a` (K11). SteamOS 3.9.5's `70-steam-jupiter-leds.rules` makes the LEDs writable without the LED rule; a simulated missing rule warns once. Switch controllers not tried. |
| L8.5 `sleep` | ✅ | suspends; after waking, the controllers and the profile keep working |
| L8.6 Keeping awake | ✅ | Second run: a `Doppelpad` `idle` lock while a toggle holds output, gone after toggling off, after `stop` and after `kill -9`; from the daemon and from the app (with the system bus, K10). |
| L8.7 The lock screen | ✅ | Second run, locked and unlocked with `loginctl lock-session` and `unlock-session`: the log says `locked=true` and `remapping paused`; while locked, mapped buttons give nothing on the virtual pad and keyboard, and they work again after unlocking. Daemon and app. |
| L8.8 Other power commands | ➖ | skipped |
| L9.1 App builds | ✅ | `npm ci` and `tauri build --no-bundle` in the Arch container; two Linux-only clippy warnings fixed in `3e5b87f` |
| L9.2 Window | ✅ | Fixed in `601b203`: the 1320 × 820 window was larger than the Deck's screen, which cut off the close button and the bottom bar. |
| L9.3 Controller view | ✅ | |
| L9.4 Apply to a slot | ✅ | `doppelpad status` reaches the app's service |
| L9.5 Tray | ✅ | quitting from the tray releases everything. Second run: per-controller icons failed for ids with a slash, fixed in `8a0d69f`, and a reconnected controller's icon stayed hidden, fixed in `3acb566` (K9). |
| L9.6 Log file | ✅ | `~/.local/state/doppelpad/logs` |
| L9.7 Overlays | ✅ | Fixed in `374edfa`: the first overlay crashed the app (tao panicked). They then showed above other windows, but in the middle of the screen, and over a fullscreen game they took the focus (K6). Since `81c0d0f` they are layer-shell surfaces on KWin (2026-10-26, KDE Plasma 6.7.3): the OSD at the bottom right above the panel at its size, the crosshair in the middle, the controller overlay in its corner, the mappings overlay and a radial menu, driven by a controller through a test profile; the crosshair over a video in Firefox in fullscreen, which kept its focus and its fullscreen. |
| L9.8 AppImage | ✅ | Third run, started from Dolphin on the host: window, controllers and tray icon, without a container or `DBUS_SYSTEM_BUS_ADDRESS`. L9.3–L9.5 as in the container; quitting from the tray removes the virtual devices and releases the grab. Keeping awake and the lock-screen pause work (L8.6, L8.7). *Check now* says that no `linux-x86_64` build was found, as v0.5.0 has none. `doctor` from the `.tar.gz` runs on the host; the tool needs glibc 2.35. The library list had every library under `-` and no copyright files, because `dpkg -S` takes `/libfoo.so.1` for a whole path; fixed in `cfa4e13`. Two libraries are GPL only (K13). A `launch` of Konsole failed (`version XZ_5.4 not found`), because programs Doppelpad started inherited the AppImage's `LD_LIBRARY_PATH` and more; fixed in `2ae9ff1`. They also stayed behind as zombie processes; fixed in `2d08949`. The AppImage built from `72f0671` has the full library list and starts Konsole without the AppImage's environment. Programs still started in the AppImage's `usr` folder, which goes away when Doppelpad ends; fixed in `d86007c`. The AppImage built from `e7acb0a` starts them in the folder it was started from, carries neither `libtiff5` nor `libjbig0`, and has the GPL source beside it (K13). |
