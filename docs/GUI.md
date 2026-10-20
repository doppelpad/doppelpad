# Desktop app

`doppelpad-gui` is the editor and the remapping service in one program. It runs the same
engine as `doppelpad daemon`. While it runs, `doppelpad status`, `apply`, `select-slot`, `remap` and `stop` talk to
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
| **Bottom bar** | Connected controllers with their battery level where the controller reports one (DualShock 4, DualSense, Switch controllers, wireless Xbox controllers; orange below 40 %, red below 20 %) (click one to use it; its menu gives it *own slots*, see [Several controllers](#several-controllers)), driver warnings, **Remap ON/OFF**, *Apply to slot n* (saves, puts the config in that slot and activates it), *Autodetect* (switch configs by the game in the foreground; only while remapping is on), and the four slots (click to activate, which also turns remapping on; × to unload). |

## Editing a control

1. Pick the layer in the toolbar and click the control (or press it). A stick asks for its
   direction (or the click, or *any direction* for zones). The touchpad asks for its click
   or one of five regions (top, bottom, left, right, centre), or a gesture (taps, swipes with
   one or two fingers, zoom); with two trackpads, the regions, tap and click of each half.
   Controllers with motion
   sensors have a **Gyro** chip at the top left of the drawing for the gyro directions, lean
   left/right and *any direction* (zones). A trigger's binding panel offers its zones and
   *Full pull*; a gyro direction's offers its zones, which are set in Analog → Gyro.
2. Choose the **activator** from the six icons: single press, long press, double press,
   triple press, start press and release. A dot under an icon means it is already mapped.
3. **Add** actions: keyboard (from the list, or *Press a key…* and press it), mouse
   buttons, wheel and movement, controller buttons, sticks (including an *Anti-recoil* preset
   that pulls the right stick down while held), triggers, touchpad gestures and motion shakes
   (both for a virtual DualShock 4), or commands
   (turn remapping off, gyro on/off/toggle, screenshot, close the active window, shut
   down, restart, sleep, hibernate, sign out, launch a program, pause, rumble; power
   commands ask once before they are added). Several actions
   fire together, as a chord.
4. Optional:
   - **Turbo** repeats the actions while the control is held, at the given period; *Random
     extra* lets each cycle wait up to that much longer, at random (also for repeating macros).
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
   unless the layer maps it. *Give back while active* turns the layer into an unlock layer:
   the real keyboard, mouse or both work normally while it is held, and *Main*'s mappings
   for them do not apply there.
   *Config settings* also hold the **crosshairs**: shape, colours, size, line width, outline,
   offset and an optional magnifier, and the layers that show each one; the action picker's
   commands show them (*Show crosshair N*, *Crosshair on/off*).
   **Vibration** sets each motor's strength, swaps them and limits how often rumble
   commands go out.
   The **light bar** (DualShock 4, DualSense) is set there as well, for *Main* and per layer:
   an effect, its colours and the speed. *Settings → Controller lights* sets what the player
   lights and the DualSense microphone LED show.
   **Radial menus** live there too: the layer that opens each one, what points at a sector
   (a stick, the mouse or commands), when the pointed sector runs, and per sector its label,
   icon (a built-in set or an image file), colour and actions or sub-menu.
   A layer change can wait for a **Delay**; *Run the actions at the layer change* then holds
   the mapping's own actions back until the jump. *Config settings* decide whether held
   buttons are re-pressed when the layer changes.

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
row: stick amount (and *Set the axis*, which replaces the physical stick on that axis instead
of adding to it) and trigger value in %, wheel clicks, mouse speed, rumble length and the
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

**Mouse** sets how often held wheel actions repeat (0: once per press) and the speeds where
the physical mouse's zones (`MouseMove:Low`, `…Med`, `…High`) start.

**Turns** sets how the game turns with mouse motion, for flick stick routes and turn actions
(*Turn left 90°*, *Turn right 90°*, *Turn around 180°* in the mouse tab of the action picker):
*Counts per 360°* and *Turn time*. *Test a full turn* turns once around after 3 seconds, so you
can switch to the game and tune the counts until the turn is exact.

**Shake** sets the *Strength* (in g) and *Length* of motion shake actions, which rock the
accelerometer of a virtual DualShock 4 along one axis.

Its **Timing** section holds the config-wide defaults: the shortcut window (how long the
controls of a shortcut may take to be pressed together), the long-press time, the double-
and triple-press windows, the turbo period and the delay between keys. A mapping's own
timing wins over these; empty fields use the built-in defaults.

## Sticks, triggers and analog routes

**Analog** in the toolbar opens the analog settings. The controller drawing leads there
too: clicking a stick offers *Stick settings* under the direction picker, and the binding
panel of a stick direction, stick click or trigger links to the matching page. With a
controller connected, the previews follow its sticks and triggers live. The view has six
tabs.

**Left stick** and **Right stick** shape the physical stick for the whole config (every
layer, unless a layer changes them; see below):

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
light up when mappings would fire. On a DualSense, *Adaptive trigger* adds resistance from a point,
a gun-like click point or a vibration against the finger, with presets (soft, medium, hard,
half way, rifle, pulse, rumble); a layer can set its own or turn Main's off.

**Touchpad** sets how the touchpad becomes region controls, for the whole config: *Two
trackpads* makes its left and right halves separate trackpads with their own regions, tap and
click; *Regions* are sectors *Around the centre* (with the size of the *Centre region*) or a
*3 × 3 grid*; and *Regions only while the touchpad is clicked*. Gestures (taps, swipes, zoom)
are always recognised.

**Gyro** sets how the motion sensors become gyro controls, for the whole config: *Turning*
(directions and zones fire while the controller turns, by how fast) or *Tilting* (the
controller is a stick: they follow how far it is tilted from its centre, and gyro routes to a
stick follow the tilt; the *Reset tilt* command sets the centre). *Left and right* follow the
roll (like a steering wheel) or the turn; *Gyro aiming* in *Player space* turns around
gravity, however the controller is held. *Gyro on when the config starts* can be switched off
for configs that turn it on with a command. The zones (low, medium, high) are in degrees per
second when turning and in degrees when tilting.

*Calibration of this controller* removes gyro drift from the controller in use, in every
config and at once: *Calibrate* measures for three seconds while it lies still (if it moves,
the old calibration stays), *Reset* forgets it, and *Calibrate automatically while it rests*
keeps adjusting it whenever the controller lies still (on by default; very slow turns can be
taken for drift).

With a shift layer selected in the toolbar, the stick and trigger pages show the settings as
they are on that layer, and a change applies to that layer only. Everything the layer does
not change follows *Main*: a note above the settings says whether the layer has settings of
its own, *Use Main's settings* removes them, and each slider's reset button goes back to
Main's value. The zone editor in the binding panel works the same way for the zones of the
selected layer.

Below the stick settings, **Buttons and keys on this stick** sets how mappings push the
virtual stick from a button or key: *Push at once* (the default) or *Ramp up*, then *Start
at*, *Stop at*, *Speed*, *Acceleration* and a speed factor per axis. The graph shows the
deflection over the first two seconds of holding a button. Two buttons pushing in opposite
directions cancel out. The *Output curve* shapes everything the virtual stick sends, also
the passed-through stick and analog routes. On a shift layer these settings work like the
others: the layer keeps only what it changes.

*Jitter* adds a small offset that keeps moving around the virtual stick (pattern *Circle*,
*Square* or *Cross*, with the smallest and largest offset and the shortest and longest step).
It runs all the time the layer is active, also with the stick at rest, so put it on a hold
layer to limit it, e.g. to aiming.

**Routes** lists the analog routes of the current layer. *Add route* offers common ones:
right stick → mouse, gyro → mouse, gyro → right stick while `LT` is held, mouse → right
stick, touchpad → mouse, right stick → flick stick. Each route can then take any source (left or right stick, gyro,
mouse, touchpad) and target (mouse, left or right stick, or *Flick stick* for stick sources; the
mouse cannot drive the mouse). A flick stick route turns the view to face where the stick
points once it passes *Flick at*, then turns with the stick; it has no vertical settings.
A route shows only the settings it uses:

| Setting | Used by |
|---|---|
| *Sensitivity*, *Vertical ratio*, *Invert* | every route; the hint under *Sensitivity* says what 1 means for the route |
| *Curve* (exponent on the deflection; 1 = linear) | stick → mouse, mouse → stick, touchpad → stick |
| *Deadzone* | stick → mouse, stick → stick, touchpad → stick |
| *Anti-deadzone* | routes to a stick |
| *Smoothing* | stick → mouse, mouse → stick, gyro → mouse or stick |
| *Acceleration at the edge* | stick → mouse: held at 90 % or more, the cursor speeds up (up to 4 ×) |
| *Snap to centre below* | routes to a stick |
| *Noise filter*, *Spring back to centre* | mouse → stick (without the spring the stick stays where the mouse left it) |
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

## Several controllers

Normally the four slots are shared: they run on the controller in use (the one you clicked
in the bottom bar, else the first one). To play with two controllers at once, open the
second controller's menu (the arrow next to its name) and choose **Own slots for this
controller**. It then shows its own four slots: *Apply to slot* and the slot buttons work on
them while that controller is selected, and it runs its own config with its own virtual
controller while it is connected. The shared slots keep running on the other controller.
Keyboard and mouse belong to the shared slots' config. Remap ON/OFF switches all of them;
*Autodetect* changes the shared slots only. Each config hides only its own controller.

*Settings → Controllers* lists every connected device. Untick one (or choose **Ignore this
controller** in its menu) and Doppelpad leaves it alone completely: no input, no hiding, no
hotkeys. **Groups** play several controllers as one, for example a second pad for a friend
who only presses a few buttons: buttons of either count, each stick and trigger follows
whichever is pushed further, motion and touch come from the first controller that has them,
and rumble, lights and trigger effects go to all of them. A group appears as one controller
with its own name and can have slots of its own. Keyboard and mouse cannot join a group;
they always count for everyone.

The Steam Controller and the Steam Deck stop acting as keyboard and mouse while Doppelpad
reads them and do so again when it stops (the Deck after a second by itself). Their two
trackpads are the two halves of the touchpad: choose *Two trackpads* in the config's touchpad
settings to map each pad on its own. The Steam Controller's right pad also works as its right
stick. When Steam runs, it adds a virtual Xbox controller of its own for these pads; ignore
one of the two in *Settings → Controllers*.

GameCube controllers on the USB adapter (Nintendo's or a clone switched to Wii U mode) show
up one per occupied port. The adapter works with Windows' own driver and with the WinUSB
driver Dolphin needs; with the WinUSB driver Doppelpad cannot hide it from games. Rumble
needs the adapter's second (grey) plug.

Other game pads and joysticks (HID devices without a driver of their own) work with a guessed
layout at first. Choose **Identify buttons …** in their menu: Doppelpad asks for each button,
trigger and stick direction in turn and records what the controller sends (skip what it does
not have). The layout is kept for every controller of that model; the same menu entry can
discard it again.

On first start a short guide checks the drivers (with download buttons for ViGEmBus and
HidHide), lists the connected controllers (unknown ones can be identified right there), asks
what the controller should become (a virtual Xbox or DualShock 4 controller, the desktop, or
keyboard and mouse as a controller) and creates a first game with a matching config, applied to
slot 1 if you like. *Settings → Welcome* shows it again. The same templates are in the config
menu as **New from template: …**.

To share a game, choose **Export game …** in its menu in the game list: one `.dpgame` file holds
its settings, every config and its picture. **Import game** (the arrow next to *+*) unpacks such
a file into a new game; nothing existing is overwritten. **Save bindings sheet …** in a config's
menu writes an HTML page listing every layer's mappings and routes, to print from a browser. A
layer's menu can **Save as preset …** (its settings, mappings and routes); presets of all configs
appear in every layer menu as **Insert preset: …**, which adds them as a new layer (layer
switches to layers the config does not have are dropped).

*Settings → Local API (HTTP)* lets stream decks and scripts switch remapping and slots, load
configs and follow changes; see [API.md](API.md). It is off by default, answers only this PC
unless the network is allowed, and then only with its token.

*Settings → While remapping*: Doppelpad keeps the PC awake while a mapping plays input (a
macro, a toggle that holds a key); it can pause remapping while the config's game is not in
front (configs of a game folder, or with program names of their own), and a controller with
slots of its own can keep its virtual controller, idle, while it is disconnected. On the lock
screen, at Ctrl+Alt+Del and during administrator prompts remapping always pauses and lets go of
everything. While a game with an anti-cheat known to act against automated input is in front
(Counter-Strike 2, Call of Duty), the warnings bar says so, with the publisher's statement.

*Settings → Motion server (DSU)* lets emulators such as Cemu, Dolphin or Ryujinx read the
controllers' buttons, sticks and gyro over the DSU (cemuhook) protocol, on port 26760 unless
you choose another. It is off by default and answers only this PC unless *Reachable from the
network too* is on. The first four controllers take slots 1–4 and keep them while connected;
the emulator sees the physical controller, not the remapped output.

*Keyboard and mouse input* in the config's output settings says which keyboard and mouse
events its mappings react to: *all* (the default), *physical devices only* (input that other
programs inject passes unchanged), or *from scripts only*: then a tool such as AutoHotkey
drives the mappings, for example the virtual controller for a game that ignores injected
input, while the real keyboard and mouse keep working as usual. Doppelpad's own output never
counts as input.

Wireless controllers can be turned off from their menu in the bottom bar (**Turn off**), with
the `power_off` command in a mapping, or by themselves: *Turn wireless controllers off after*
switches off a controller that had no input for that many minutes (resting sticks with a
little drift do not count). Wired controllers stay on. Its home button turns it back on.

## Controller hotkeys

*Settings → Controller hotkeys* sets button combos that work in every config, with
remapping off, and without the window (`doppelpad daemon` reads them too): **Slot 1–4**
(activates that slot and turns remapping on; empty slots do nothing), **Remapping on/off**,
**Stop macros** (stops running macros, turbo and toggles and releases what they hold),
**Emergency stop** (the same, then remapping off), and the **Controller overlay** and
**Mappings overlay** switches (see [Overlays](#overlays)). Hold all buttons of a combo for
the *hold time* (500 ms by default, 50–2500 ms). Defaults: LB + RB + X / Y / B / A for slots
1–4, Back + Start + LB + RB for the emergency stop; the others are off.

To record a combo, click the record button, hold the buttons on the controller and let go;
× clears it. Hotkeys are paused while recording. As soon as all buttons of a hotkey are
down, the game no longer sees them (each stays hidden until it is released); a combo you
need in a game must therefore not be a hotkey. A combo inside a larger one (LB + RB inside
LB + RB + X) waits for the larger one.

## Overlays

Two overlays show over games, switched on and off by a controller hotkey or a mapping's
*Controller overlay on/off* / *Mappings overlay on/off* command:

- the **controller overlay**, a live picture of the controller in use with its pressed
  buttons and stick positions. It hides after a few seconds without input and comes back
  with the next one;
- the **mappings overlay**, the running config's mappings on the active layer with their
  descriptions, in columns of up to 40 rows (120 rows at most). It follows layer changes.

*Settings → Overlays* picks their corner, the controller's size, both opacities and when the
controller hides. Like the notifications they let clicks through and never take the focus;
`doppelpad daemon` shows neither.

## Tray

The tray icon is greyed out while remapping is off. Its tooltip names the active config,
warns when no controller is connected and lists the controllers' battery levels. Left-click opens the window, middle-click
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

## On-screen notifications

Short messages appear over games when remapping goes on or off, another slot or config
becomes active, the layer changes, a controller connects or disconnects, or a controller's
battery drops below 40 % or 20 % (once per drop, not while charging). They also show
while the window is closed to the tray, but not while the window has focus (it shows the same
state) or in the first seconds after start. *Settings → On-screen notifications* picks the
events, the monitor and corner, the size, opacity and display time, and whether a layer
message stays up while the layer is active; *Show a sample* previews them.

Each message is a small window that stays above other windows, lets clicks through and never
takes the focus, so it works over windowed and borderless games. Games in exclusive full-screen
mode draw over it. `doppelpad daemon` shows no notifications; they are part of the app.

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
| `%APPDATA%\doppelpad\calibration.toml` | Gyro calibration per controller (also used by `doppelpad daemon`) |
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
