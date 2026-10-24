# Desktop app

`doppelpad-gui` is the editor and the remapping service in one program. It runs the same
engine as `doppelpad daemon`. While it runs, `doppelpad status`, `apply`, `select-slot`, `remap` and `stop` talk to
it over the usual IPC socket. Closing the window keeps remapping running in the tray (see
[Tray](#tray)), and **Quit** in the tray menu stops it and shows hidden controllers again.

The app is built with [Tauri 2](https://tauri.app) and a Svelte 5 + TypeScript frontend
(`gui/`). The Rust side (`gui/src-tauri`, crate `dp-gui`) hosts `dp-service`. The frontend
only edits profiles and sends commands.

This guide describes the app on Windows. What differs on Linux and macOS is in
[LINUX.md](LINUX.md#desktop-app) and [MACOS.md](MACOS.md#desktop-app).

## Window layout

| Area | What it does |
|---|---|
| **Games** (left) | One entry per game. A game has one or more configs and an optional list of programs (`game.exe`) that switch to it automatically. A green dot marks the game whose config is running. Configs outside a game folder appear under *Other configs*. A game's menu has **Game settings…**: its name, a description (shown under the name), a picture (copied into the game's folder and shown in the list), and its programs, picked from the running programs (those with a window), from a file picker, with *Game in front* (switch to the game; after three seconds the program in front is taken), or typed in. **Duplicate** copies a game with its configs and picture; the copy has no programs, so automatic switching keeps choosing the original. The books button at the top of the list shows the **games from your libraries**: what Steam, Epic and GOG have installed on this PC (read from their own files, nothing goes online). **Add** creates a game with its name, the program that switches to it (the likeliest is suggested; a list offers the others) and, for Steam games, the cover from Steam's cache; games already in the list are marked. |
| **Header** | *Game › Config* picker (switch, create, import a JSON profile), **Undo** and **Redo** (see [Undo](#undo)), **Save** (Ctrl+S; a dot means unsaved changes), options (rename, duplicate, show in Explorer, clear, delete), settings. Imported JSON files are read-only; duplicate them to edit. The header and the top of the games list also act as the title bar: drag them to move the window, double-click to maximize. On Windows and Linux the app draws its own minimize, maximize and close buttons at the right. On macOS the system buttons sit at the top left. |
| **Toolbar** | Views: *Controller*, *Back side*, *Keyboard*, *Mouse*. The two controller views draw the connected model as it looks from that side: the front shows the bumpers (L1 / R1, L / R) but not the triggers behind them; the back side shows the triggers, the bumpers where they show from behind (not on the DualSense), and the back buttons the model has, e.g. GL / GR on the Switch 2 Pro Controller, none on the first one. Layer chips: *Main* plus shift layers, which you can add, rename, copy, paste, clear and remove (together with their analog routes), and whose *Layer settings* say what they inherit from *Main*. Hovering a layer chip (or focusing it) shows how the layer is reached: every mapping that changes to it with its activator, kind (while held, toggle, switch) and delay, and the layer it sits on; a layer nothing changes to says so. The tooltip also names the radial menus the layer opens and whether it takes over *Main*'s mappings or gives the keyboard or mouse back. **Layer map…** in the layer menu draws all layers with an arrow for each mapping that changes layer (⇣ while held, ⇄ toggle, → switch), marks layers nothing changes to and switches nothing leaves again, and leads to a layer or a mapping when clicked. On the right: *Analog* (see [Sticks, triggers and analog routes](#sticks-triggers-and-analog-routes)), *Shortcuts*, *Radial menus* (see [Radial menus](#radial-menus)) and *Config settings*. When the window is too narrow for every layer chip, these four show only their icons (their names are in the tooltips). |
| **Centre** | The selected view. Click a control to edit it. On the controller views, pressing a button on the controller selects it, and turns the drawing to the side that shows it (a trigger to the back side, a face button to the front). On the keyboard view, pressing a key does the same. |
| **Binding panel** (right) | See below. With nothing selected, it lists the mappings of the current layer, and on a layer that nothing changes to yet it says how to reach it. |
| **Bottom bar** | On the left: the remap button (on in teal, off in grey; a pause sign while remapping pauses, see [Remapping while the window is open](#remapping-while-the-window-is-open); its tooltip names the running config), the controller in use with its battery level where the controller reports one (DualShock 4, DualSense, Switch controllers, wireless Xbox controllers; orange below 40 %, red below 20 %), and driver warnings. *+1* means another controller is connected; it turns orange or red when that one's battery runs low. The controller list picks the one to use, and the cog next to it holds that controller's options: *own slots* (see [Several controllers](#several-controllers)), identifying a generic pad's buttons, **Turn off** and **Ignore this controller**. On the right: *Apply to slot n* (saves, puts the config in that slot and activates it; the arrow picks the slot, and hovering the button marks it) and the four slots (click to activate, which also turns remapping on; × to unload). |

## Undo

**Undo** (Ctrl+Z) takes back the last change to the open config, **Redo** (Ctrl+Y or
Ctrl+Shift+Z) brings it back, up to 100 steps. A slider drag, typing into a field and a macro
recording each count as one step. Saving keeps the steps, so a change can be undone after
saving too (the config is then unsaved again); opening another config starts afresh. Undo
covers the config only: game settings, slots and the app's settings are not part of it. In a
text field, Ctrl+Z undoes the typing in that field.

Questions about changes that can be undone (adding a shut-down, restart, sleep or sign-out
command to a mapping, clearing all mappings, replacing a stick's directions with a whole-stick
choice) offer *Don't ask again*; ticked and answered with OK, they are not asked again.
*Settings → General → Questions you turned off* lists them, each with *Ask again*. Deleting
games or configs and discarding unsaved changes always ask.

## Problems in a config

A bar below the toolbar counts the problems of the open config; click it for the list. The
config is checked again a moment after each change, not only when it is saved. Besides values
out of range it finds:

- two mappings with the same trigger and activator on one layer (both run);
- a layer that no mapping changes to, so its mappings never apply (for a layer added since the
  config was last saved, this waits until it is saved: a new layer has nothing changing to it
  yet while it is being set up);
- a *switch* to a layer that no mapping there leaves again;
- controller output while the config has no virtual controller, and touchpad gestures or
  shakes for a virtual Xbox 360 controller;
- a shortcut or mapping that holds every button of a controller hotkey: the hotkey takes the
  buttons, so the mapping never fires while hotkeys are on.

A problem about a mapping, a layer or a radial menu leads there when clicked. The controls, shortcut rows and
layer chips concerned carry a small orange mark. Errors keep the config from being saved;
warnings do not.

## Copy and paste

Right-click a control (on the drawing, in the list, on the keyboard and mouse views, or a
shortcut) or press the menu key or Shift+F10 on it; the **…** menu in the binding panel's head
has the same entries:

- **Copy** takes everything the control does on the current layer, all activators with their
  modes, timing, layer switches and notes (on a shift layer, what it inherits from *Main* if it
  has nothing of its own).
- **Paste** replaces what another control or shortcut does on the current layer with it.
- **Clear** removes the control's mappings on the current layer; on a shift layer *Main*'s apply
  again.
- **Copy / Paste / Clear all directions** do the same for a whole group: a stick (with its
  zones and the routes it is the source of), the D-pad, the touchpad or one trackpad, the gyro
  or the mouse movement. Pasting moves each direction to the same direction of the target
  (left stick ↑ → right stick ↑); what the target does not have, such as diagonals on the
  D-pad, is left out, and the message says how much. The stick menu on the drawing has the
  three as buttons at its bottom; right-click a stick or the gyro chip for them too.

The copy stays while the app runs, so it can be pasted into another config. Pasting and clearing
can be undone. The macro editor has **Copy steps** and **Paste steps** for whole sequences.

**Swap or mirror controls…** in the layer menu changes the current layer or the whole config in
one step: *Swap A/B and X/Y* (for example for Nintendo button positions), *Swap the sticks*
(southpaw), *Swap bumpers and triggers*, or *Mirror left and right* (bumpers, triggers, sticks,
D-pad and face buttons, Back and Start, paddles and trackpads change sides). Each control does
what its partner did, with all activators. A button nothing maps takes its own output along
(an unmapped A becomes a mapping B → A). A stick that only passed through gets a route to the
other virtual stick, shaped by its deadzone, outer deadzone, anti-deadzone and sensitivity; its
response curve is not carried over. On a shift layer, a control that does nothing there after
the swap stops taking *Main*'s mapping over. Ctrl+Z takes a transform back.

## Find

**Find** (the magnifier in the header, or Ctrl+F) searches the open config across all layers:
by what mappings output (keys, buttons, commands, also inside macros), by their control, by
the layer they change to and by their description. Every word must match somewhere; case and
accents do not matter. Up and down move through the results, Enter or a click selects that
mapping (on its layer and activator, in the view that shows it). In the keyboard view, a
key's context menu has *Which mappings press this key?*, which lists exactly the mappings whose
actions press it.

## Keyboard shortcuts

Ctrl+/ (or F1, or *Keyboard shortcuts* in the config's options menu) lists them all:

| Keys | Does |
|---|---|
| Ctrl+S | Save the config |
| Ctrl+Z, Ctrl+Y or Ctrl+Shift+Z | Undo, redo |
| Ctrl+C, Ctrl+V | Copy the selected control, paste onto it (a copied direction group pastes onto the selected control's group) |
| Delete | Clear the selected control on the current layer |
| Escape | Deselect, leave the macro editor |
| Ctrl+1 … Ctrl+9 | Main, then the shift layers in the toolbar's order |
| F2, Ctrl+D | Rename, duplicate the config |
| Ctrl+F | Find mappings |
| Ctrl + plus, Ctrl + minus, Ctrl+0 | Make the window's contents larger, smaller, normal (80–150 %; also *Settings → Display → Size*) |

In the keyboard view, plain keys select themselves, so Delete, Escape, F1 and F2 do that there;
clearing is Ctrl+Delete. In text fields the keys act on the text, and while a dialog or menu is
open it gets them (Ctrl+S still saves). On macOS, Cmd takes the place of Ctrl.

## Editing a control

1. Pick the layer in the toolbar and click the control (or press it). A stick asks for its
   direction (or the click, or *any direction* for zones), or what the whole stick does (see
   [A whole stick](#a-whole-stick-as-mouse-wasd-or-flick-stick)). The touchpad asks for its click
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
   - **Description** is a note for yourself. The note button at the top right of the
     controller, keyboard and mouse views shows the descriptions on the drawing, the tiles and
     the keys instead of the actions (where a mapping has none, its actions stay); the app
     remembers the choice. While the actions show, a small dot marks a control whose mappings
     have a description.
   - **Timing** overrides the long-press, double- and triple-press windows or the key
     delay for this mapping only.
5. On a shift layer, controls without their own mapping inherit the one from *Main*. The
   panel says so. Editing such a control creates an override on the current layer.
   **Layer settings** (in the layer menu) turn inheriting off for the whole layer, or keep
   single controls of *Main* out of it: an unticked control does nothing on that layer
   unless the layer maps it. *Give back while active* turns the layer into an unlock layer:
   the real keyboard, mouse or both work normally while it is held, and *Main*'s mappings
   for them do not apply there.
   *Config settings → Crosshairs* holds the **crosshairs**: shape, colours, size, line width,
   outline, offset and an optional magnifier, and the layers that show each one; the action
   picker's commands show them (*Show crosshair N*, *Crosshair on/off*).
   *Config settings → Light & rumble* sets the **light bar** (DualShock 4, DualSense) for
   *Main* and per layer: an effect, its colours and the speed; and **Vibration**: each
   motor's strength, swapping them and how often rumble commands go out.
   *Settings → Controllers → Controller lights* sets what the player lights and the
   DualSense microphone LED show.
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
choose *down*, *up* or *tap* for keys and buttons. *hold for a time* turns a step into a
press, a pause of half a second (change it as you like) and the release. Steps with parameters show them in their
row: stick amount (and *Set the axis*, which replaces the physical stick on that axis instead
of adding to it) and trigger value in %, wheel clicks, mouse speed, rumble length and the
strength of each motor, a program's path and arguments. In the binding panel, click such
an action to change the same values. **Record** captures keyboard presses together with
the pauses between them; **Record controller** does the same for the buttons, stick
directions and triggers of the controller in use. A **Break** step cuts the macro into
parts: each press plays the next part, then it starts over (turbo, toggle and repeating
are not available for such macros).

**Config settings** has five pages, picked in the list on the left: *Controller*,
*Keyboard & mouse*, *Timing*, *Light & rumble* and *Crosshairs*.

*Controller* sets the output device: the virtual controller (Xbox 360, DualShock 4,
or none for keyboard and mouse only), and what shake actions do to its motion sensor. It also covers hiding the physical controller from games, passing
unmapped controls through, and blocking the original keys of remapped controls. Missing
drivers are shown here and in the bottom bar. A virtual DualShock 4 also passes the
touchpad fingers and the motion sensors through (unless a mapping or analog route uses
them), and the game's rumble and light bar colour come back to the physical controller
(light bar: DualShock 4 and DualSense over USB).

On *Keyboard & mouse*, **Held keys repeat** sets how keys that mappings hold down repeat: *Like a keyboard*
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

## A whole stick as mouse, WASD or flick stick

The menu of either stick has a row *Whole stick as*: **Mouse**, **WASD**, **Arrow keys** and
**Flick stick**, for the current layer. Mouse and Flick stick add an analog route from that
stick (see [Routes](#sticks-triggers-and-analog-routes)); WASD and Arrow keys map the four
directions to W, A, S, D or the arrow keys. A choice replaces the stick's direction mappings
(with their zones) and its routes on that layer, after asking; shortcuts with the stick and
its click stay. The stick's label on the drawing then says *Mouse* or *Flick stick*.

The side panel shows the whole stick: the same four choices, the route's settings and
*Clear this stick on this layer*. For the mouse it adds *Response* presets and a graph of the
cursor speed over the stick's deflection, with a dot that follows the stick:

| Preset | Feel |
|---|---|
| *Standard* (where a new mouse stick starts) | a gentle curve, a little smoothing, some acceleration at the edge |
| *Precise* | slow near the centre for fine aiming, quick at the edge |
| *Fast* | linear and quick, with strong acceleration at the edge |
| *Smooth* | more smoothing: calmer, with a little delay |

A preset sets the curve, deadzone, *Counts as full from*, smoothing and acceleration;
sensitivity, vertical ratio and inverting stay. Changing one of those values shows *Custom
values*.

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
| *Counts as full from* (deflection that already gives full speed or deflection) | stick → mouse, stick → stick |
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

## Radial menus

A radial menu is a ring of up to 12 sectors over the game: hold a button, point at a sector
with a stick or the mouse, let go, and the sector runs. Open *Radial menus* in the toolbar
and choose **Create radial menu**: the menu gets a layer of its own (named like the menu),
so all that is left is **Choose button…** under *Opens with*, from the list or by pressing
it. The menu shows while that button is held. A button that already does something on
*Main* asks first. The *Layer* field below picks another layer instead, a new one, or
*Only as a sub-menu*.

Click a sector in the ring to edit it in the side panel: label, icon (the built-in set or an
image file), colour, and what it does. *Run actions* lists steps that run one after the
other, as in the macro editor: each key or button can tap, press, release or be held for a
time, with pauses in between. *Open a menu* opens another menu (or a new sub-menu) instead.

*Pointing* says what points at a sector (right or left stick, the mouse, or only the radial
menu commands) and when the pointed sector runs (when the button is let go, or with the
select command). *Position* puts the ring in the middle of the screen, at an edge or in a
corner, or where the mouse pointer is when it opens, and an offset moves it from there.
*Look* sets the size, how much the game behind it darkens and a delay before it shows.
Deleting a menu also deletes its layer and its button, unless something else uses the layer.

## What Doppelpad sends

The pulse button at the top right of the controller, keyboard and mouse views opens a small
panel with what the running config sends right now: the virtual controller's pressed
buttons and stick caps, drawn on the controller in use (so they show where your buttons are;
below it, which controller games see), a bar per trigger, the keys and mouse buttons it
holds, and the layer. A config can be tried out this way without a game. Remapping pauses while the
window has the focus (see below); the panel then offers *Keep remapping active while this
window is focused*. `doppelpad monitor` shows the controller's input in a terminal.

## Remapping while the window is open

Remapping pauses while the Doppelpad window has keyboard focus. Mapped keys therefore
don't land in the editor, and pressing a button selects it instead of triggering its
mapping. The remap button in the bottom bar then shows a pause sign, and its tooltip says
*Paused while editing*. Switch to the game (or hide the window) and the active slot takes
over again. Doppelpad also checks which program is in front: if Windows misses telling the
window that it lost the focus, remapping still resumes about a second after another
program comes to the front. The log records every pause and resume with its reason.
*Settings → General → Keep remapping active while this window is focused* turns this off.

## Several controllers

Normally the four slots are shared: they run on the controller in use (the one picked in
the bottom bar's controller list, else the first one). If that controller disconnects for a
moment, they move to another one and come back to it when it returns; only the controller in
use is hidden from games. To play with two controllers at once, pick the second controller
in the list, open the cog next to it and choose **Own slots for this controller**. It then
shows its own four slots (the list marks it with a stack icon): *Apply to slot* and the slot
buttons work on them while that controller is picked, and it runs its own config with its
own virtual controller while it is connected. The shared slots keep running on the other
controller. Keyboard and mouse belong to the shared slots' config. The remap button switches
all of them; *Autodetect* (*Settings → Remapping*) changes the shared slots only. Each
config hides only its own controller.

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

### Controllers that other programs hold

Some programs take a controller for themselves, so that Doppelpad cannot even read it. The
controller list then names the likely program when one of the known ones runs (DS4Windows,
BetterJoy, Dolphin for the GameCube adapter, Steam), for example "Steam is using the
DualSense"; close that program or turn off its controller support. Doppelpad tries again
every 30 seconds; *Look for controllers* next to the warning tries at once.

Steam Input can also take Doppelpad's virtual controller and remap it a second time, so
buttons act twice or not as mapped. When a game of the library comes to the front, its config
drives a virtual controller and Steam runs, Doppelpad points this out once per game and run:
an on-screen note over the game and a hint in the window with the way to turn Steam Input
off for that game (the game's *Properties → Controller* in Steam). *Don't show again* turns
the hint off; *Settings → General* lists it with the questions turned off.

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
slot 1 if you like. *Settings → General → Welcome* shows it again. The same templates are in
the config menu as **New from template: …**.

To share a game, choose **Export game …** in its menu in the game list: one `.dpgame` file holds
its settings, every config and its picture. **Import game** (the arrow next to *+*) unpacks such
a file into a new game; nothing existing is overwritten. The installer registers `.dpgame`
files, so a double-click in Explorer opens Doppelpad (or the one already running) and asks
whether to import the game. A config can start programs (the
*launch* command), so a shared one could run something harmful: when a game or a JSON profile
you import has launch commands, the app first lists each program with its arguments and where
it sits. **Import without these** (the default) leaves them out, **Import with them** keeps
them, *Cancel* imports nothing. **Save bindings sheet …** in a config's
menu writes an HTML page listing every layer's mappings and routes, to print from a browser. A
layer's menu can **Save as preset …** (its settings, mappings and routes); presets of all configs
appear in every layer menu as **Insert preset: …**, which adds them as a new layer (layer
switches to layers the config does not have are dropped).

*Settings → Connections → Local API (HTTP)* lets stream decks and scripts switch remapping and
slots, load configs and follow changes; see [API.md](API.md). It is off by default, answers
only this PC unless the network is allowed, and then only with its token.

*Settings → Remapping*: Doppelpad keeps the PC awake while a mapping plays input (a
macro, a toggle that holds a key); it can pause remapping while the config's game is not in
front (configs of a game folder, or with program names of their own), and a controller with
slots of its own can keep its virtual controller, idle, while it is disconnected. On the lock
screen, at Ctrl+Alt+Del and during administrator prompts remapping always pauses and lets go of
everything. While a game with an anti-cheat known to act against automated input is in front
(Counter-Strike 2, Call of Duty), the warnings bar says so, with the publisher's statement.

*Settings → Connections → Motion server (DSU)* lets emulators such as Cemu, Dolphin or Ryujinx
read the controllers' buttons, sticks and gyro over the DSU (cemuhook) protocol, on port 26760
unless you choose another. It is off by default and answers only this PC unless *Reachable
from the network too* is on. The first four controllers take slots 1–4 and keep them while
connected; the emulator sees the physical controller, not the remapped output.

*Keyboard and mouse input* in the config's output settings says which keyboard and mouse
events its mappings react to: *all* (the default), *physical devices only* (input that other
programs inject passes unchanged), or *from scripts only*: then a tool such as AutoHotkey
drives the mappings, for example the virtual controller for a game that ignores injected
input, while the real keyboard and mouse keep working as usual. Doppelpad's own output never
counts as input.

Wireless controllers can be turned off from the cog next to them in the bottom bar (**Turn off**), with
the `power_off` command in a mapping, or by themselves: *Turn wireless controllers off after*
switches off a controller that had no input for that many minutes (resting sticks with a
little drift do not count). Wired controllers stay on. Its home button turns it back on.

## Controller hotkeys

*Settings → Hotkeys* sets button combos that work in every config, with
remapping off, and without the window (`doppelpad daemon` reads them too): **Slot 1–4**
(activates that slot and turns remapping on; empty slots do nothing), **Remapping on/off**,
**Stop macros** (stops running macros, turbo and toggles and releases what they hold),
**Emergency stop** (the same, then remapping off), the **Controller overlay** and
**Mappings overlay** switches (see [Overlays](#overlays)), and **Drive this app with the
controller** (see [Handheld mode](#handheld-mode); the only hotkey that also works while the
window has the focus). Hold all buttons of a combo for
the *hold time* (500 ms by default, 50–2500 ms). Defaults: LB + RB + X / Y / B / A for slots
1–4, Back + Start + LB + RB for the emergency stop; the others are off.

To record a combo, click the record button, hold the buttons on the controller and let go;
× clears it. Hotkeys are paused while recording. As soon as all buttons of a hotkey are
down, the game no longer sees them (each stays hidden until it is released); a combo you
need in a game must therefore not be a hotkey. A combo inside a larger one (LB + RB inside
LB + RB + X) waits for the larger one.

*Confirm with a rumble on the controller* makes the controller answer each hotkey: a slot
hotkey rumbles as often as the slot's number, remapping on once long and off twice long, the
emergency stop twice long, the others once. This also works in exclusive full-screen games,
where the on-screen notifications cannot draw. The game's own rumble waits for the pattern.

## Handheld mode

On a handheld PC the app can be used with the built-in controller alone. The button with the
D-pad icon in the bottom bar (or *Settings → General → Drive the app with the controller*,
or its controller hotkey) turns it on:

| Control | Does |
|---|---|
| D-pad, left stick | Move the focus to the nearest element in that direction (held: repeats) |
| Left / right on a list, slider or number field | Change its value |
| A | Press the focused button, tick the box, step the list |
| B | Back: close the menu or dialog, else drop the selection (what Escape does) |
| LB / RB | Previous / next view (front, back, keyboard, mouse, analog, shortcuts, output) |
| LT / RT | Previous / next layer |
| Start | Save the config |

The focused element gets a ring. In a dialog or menu the focus stays inside it, and the views,
layers and Start wait until it is closed. While the controller drives the app, pressing a
control no longer selects it in the editor (*Select controls by pressing them on the
controller* is off meanwhile). Text fields keep the focus for the touch keyboard.

## Overlays

Two overlays show over games, switched on and off by a controller hotkey or a mapping's
*Controller overlay on/off* / *Mappings overlay on/off* command:

- the **controller overlay**, a live picture of the controller in use with its pressed
  buttons and stick positions. It shows the front, so a bar above each bumper shows how far
  the trigger behind it is pulled. It hides after a few seconds without input and comes
  back with the next one;
- the **mappings overlay**, the running config's mappings on the active layer with their
  descriptions, in columns of up to 40 rows (120 rows at most). It follows layer changes.

*Settings → Display → Overlays* picks their corner, the controller's size, both opacities and
when the controller hides. Like the notifications they let clicks through and never take the
focus; `doppelpad daemon` shows neither.

## Tray

The tray icon is greyed out while remapping is off. Its tooltip names the active config,
warns when no controller is connected and lists the controllers' battery levels. Left-click opens the window, middle-click
turns remapping on or off. The right-click menu has:

- **Open Doppelpad**
- the active config (*Game › Config*) or *No config active*
- **Remapping on** and **Autodetect games**, the same switches as the remap button in the
  bottom bar and *Settings → Remapping → Autodetect*
- the four slots with their configs; choosing one activates it and turns remapping on
- **Quit**

The tray follows every change, whether it comes from the window, from `doppelpad`
commands, or from a mapping that turns remapping off.

*Settings → General → One more tray icon for each connected controller* adds an icon per
controller next to the main one, in a colour of its own (*Settings → Controllers* sets it
per controller; without a choice each controller keeps the same palette colour). Its tooltip
names the controller, the config it runs (or *Not remapped*) and its battery. Its menu lists
its slots: its own if it has slots of its own, the shared ones while it is the controller in
use; another controller offers **Own slots for this controller** instead. The icons come and
go with the controllers and are greyed out while remapping is off.

*Settings → General → Start with Windows, in the tray* (Windows only) starts Doppelpad when
you sign in, with only the tray icon showing; remapping runs as it was left. The setting is
the `Doppelpad` entry under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, which runs
`doppelpad-gui.exe --hidden`. Task Manager's *Startup* tab lists it too, and switching it
off there shows in the settings. If reWASD runs at sign-in and *When reWASD is running at
start-up* is set to *Ask*, the window opens to ask.

## On-screen notifications

Short messages appear over games when remapping goes on or off, another slot or config
becomes active, the layer changes, a controller connects or disconnects, or a controller's
battery drops below 40 % or 20 % (once per drop, not while charging). They also show
while the window is closed to the tray, but not while the window has focus (it shows the same
state) or in the first seconds after start. *Settings → Display → On-screen notifications*
picks the events, the monitor and corner, the size, opacity and display time, and whether a
layer message stays up while the layer is active; *Show a sample* previews them.

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

*Remember my choice* stores the answer; *Settings → General → When reWASD is running at
start-up* changes it later (*Ask*, *Quit it*, *Leave it running*) and offers *Quit reWASD*
while it runs. The setting appears only when reWASD is installed. `doppelpad doctor` also
reports a running reWASD.

## Files

| File | Content |
|---|---|
| `%APPDATA%\doppelpad\profiles\<game>\game.toml` | Game name, programs for autodetect, default config |
| `%APPDATA%\doppelpad\profiles\<game>\<config>.toml` | One profile (the normal TOML format, see [PROFILES.md](PROFILES.md)) |
| `%APPDATA%\doppelpad\profiles-location.txt` | The folder chosen for games and configs, when it is not the default `profiles` folder (see below) |
| `%APPDATA%\doppelpad\state.toml` | The four slots, active slot, remap on/off, autodetect |
| `%APPDATA%\doppelpad\hotkeys.toml` | Controller hotkeys and their hold time (also read by `doppelpad daemon`) |
| `%APPDATA%\doppelpad\calibration.toml` | Gyro calibration per controller (also used by `doppelpad daemon`) |
| `%APPDATA%\doppelpad\gui.toml` | Language, focus pause, press-to-select, what to do about a running reWASD, update checks, notifications, the controller and mappings overlays, whether the welcome guide is done, descriptions or actions in the editor, the questions turned off, zoom, controller navigation, the tray icons per controller |
| `%LOCALAPPDATA%\doppelpad\logs\doppelpad-gui.log` | Log of the current run; the logs of the five runs before are kept as `doppelpad-gui.log.1` (the newest) to `.5` |
| `%LOCALAPPDATA%\doppelpad\history\<game>\<config>\<time>.toml` | Earlier versions of each config, the last 20 |

The app saves atomically (it writes a temporary file, then renames it). Before it
overwrites a profile that it did not write itself, it keeps one `.bak` copy. Deleting
moves files to the recycle bin. Every TOML file it writes starts with the version of its
format (`format = 1`), and a settings file it cannot use is kept as `<file>.bak` before the
defaults replace it (see [Format versions](PROFILES.md#format-versions)).

Each time the app saves a config over its file, the version before is kept (the last 20 per
config). **Earlier versions…** in the config's options menu lists them by date; opening one
shows what restoring it would change (mappings added, removed or changed, and the settings
that differ). **Restore this version** makes it the unsaved draft: *Save* keeps it, Ctrl+Z
takes it back.

### Configs folder

*Settings → Files → Configs are stored in → Change…* moves the games, configs and layer
presets to another folder, for example one that a cloud drive keeps in sync. Doppelpad
copies every file, checks each copy, switches to the new folder (the slots and the open
config follow their files) and only then removes the old files. The new folder must be empty
or already hold a Doppelpad library: a folder synced from another computer is used as it is,
and the current configs then stay where they were. A folder with other files is refused.
*Reset to default* moves everything back to `%APPDATA%\doppelpad\profiles`. The app, the
service and the command line all read the choice from `profiles-location.txt`; a moved
library carries a `.doppelpad-library` file that marks it. The uninstaller never deletes a
chosen folder outside `%APPDATA%\doppelpad`.

### Portable mode

An empty file named `portable` next to `doppelpad-gui.exe` (and `doppelpad.exe`) keeps
everything beside the app in a `data` folder: the settings, slots and hotkeys, the games and
configs (`data\profiles`), the logs, the earlier versions of configs and the window's browser
data. Copy the two programs and the `portable` file to a folder or a stick to use them
there. A portable copy does not offer *Start with Windows*; *Settings → Files* says that it
runs portable, and `doppelpad doctor` names the folder. The drivers (ViGEmBus, HidHide) still
have to be installed on each PC.

### Log file

When reporting a problem, attach `doppelpad-gui.log` (and the numbered logs of earlier
runs if the problem happened before the app was last restarted). *Settings → Files → Open log
folder* shows the files in Explorer. The log starts with the app version, the Windows
version and whether ViGEmBus and HidHide are available, and it records warnings, the
controller in use and why it was picked, controllers that went silent and were started
again, and crashes. Each start begins a new log and keeps those of the five runs before;
a single run stops logging after 16 MiB. `DOPPELPAD_LOG` sets the detail level (default `info`; for example
`DOPPELPAD_LOG=debug`). On Linux the log is in `$XDG_STATE_HOME/doppelpad/logs`
(default `~/.local/state/doppelpad/logs`), on macOS in
`~/Library/Application Support/doppelpad/logs`.

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
npm run format                # Prettier (format:check only reports)
npm run icons                 # re-render src-tauri/icons/ from icons/source/*.svg
npm run drawings              # re-generate the controller drawings (src/assets/devices)
npm run licenses              # re-generate THIRD_PARTY_LICENSES.md (see below)
```

The GUI crate is not in the workspace's `default-members`, so `cargo build` and
`cargo test` at the root build only the CLI. Use `cargo test -p dp-gui` for the bridge
tests. They check that the frontend's key table, JSON shapes and default values match the
Rust model, using the fixtures in `gui/src/lib/model/fixtures`. Regenerate them with
`DOPPELPAD_UPDATE_FIXTURES=1 cargo test -p dp-gui`.

The npm packages `@tauri-apps/api` and `@tauri-apps/plugin-dialog` are pinned to the minor
versions of the `tauri` and `tauri-plugin-dialog` crates in `Cargo.lock` (`~2.11`, `~2.7`),
because `tauri build` refuses mismatched minor versions. When a `cargo update` moves those
crates to a newer minor version, move the npm pins with them.

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
npm run licenses
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

## Updates

Doppelpad updates itself from the project's
[GitHub releases](https://github.com/doppelpad/doppelpad/releases); there is no server of its
own.

- At start-up the app asks GitHub whether a newer version is out. *Settings → General →
  Check for updates automatically* (on by default) turns this off. **Check now** next to the
  version number asks right away and also says when Doppelpad is up to date.
- When there is a newer version, a dialog shows its number and release notes. **Install and
  restart** downloads the setup, checks its signature, stops remapping (hidden controllers
  show again, virtual ones are unplugged), closes the app and runs the setup with only a
  progress bar. Windows asks for administrator permission, because Doppelpad is installed
  for all users. The setup replaces the files and starts Doppelpad again, with its window
  open. **Later** leaves it until the next start or *Check now*. Unsaved changes in the
  editor are asked about first.
- An update keeps the configs, the settings and *Start with Windows*, and never installs
  or changes the drivers. A running `doppelpad daemon` is asked to stop before its file is
  replaced and is not started again: the app takes over remapping.
- If the administrator prompt is declined or the setup cannot start, the app starts again
  as it was. A download whose signature does not match is not installed; the app keeps
  running and says why.

**Privacy:** a check is one HTTPS request to github.com for
`releases/latest/download/latest.json`; an update also downloads the setup from there. No
data about you, your PC or your configs is sent, but GitHub sees your IP address and the
time, as with any download, and the updater's user agent (`tauri-plugin-updater/<version>`).
Switch off *Check for updates automatically* to stop the checks at start-up; then Doppelpad
only contacts GitHub when you click **Check now**.

### How updates work

`plugins.updater` in `gui/src-tauri/tauri.conf.json` names the endpoints and the public key.
The updater asks the endpoints in order and takes the first that answers, so an address on
a project website can go first later, with GitHub as the fallback:

```json
"endpoints": [
  "https://<project website>/latest.json",
  "https://github.com/doppelpad/doppelpad/releases/latest/download/latest.json"
]
```

`latest.json` names the newest version, its release notes, the setup's address and its
signature. The signature is made with the updater key (Ed25519, in minisign's format),
which is separate from Windows code signing. The app installs a setup only when the
signature matches the public key in its config and covers the version that `latest.json`
announces (`requireSignedVersion`), so a changed `latest.json` cannot slip in another or an
older setup.

`bundle.createUpdaterArtifacts` makes `tauri build` write the signature,
`Doppelpad_<version>_x64-setup.exe.sig`, next to the setup. That needs the private key in
`TAURI_SIGNING_PRIVATE_KEY` and its password in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`;
without them the setup is built but `tauri build` then fails. Local builds that do not need
an update can leave the signature out:

```bash
npx tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

The updater runs the setup with `/P /UPDATE /R`: progress bar only, no driver
installation (see [Drivers](#drivers)), and start the app when done. Before it starts the
setup, the app stops its remapping service, the same way **Quit** in the tray does.

### Releasing

1. Set the version in `Cargo.toml` (`[workspace.package]`) and `gui/package.json`, and commit.
2. Tag with the release notes as the message and push the tag:

   ```bash
   git tag -a v0.4.0     # the editor opens for the release notes
   git push origin v0.4.0
   ```

3. `.github/workflows/release.yml` builds and signs the setup and creates a draft release
   with the setup, its `.sig` and `latest.json`. It stops when the tag and the two versions
   differ, the tag has no message (`git tag` without `-a`), the key secret is missing, or
   the public key in `tauri.conf.json` is still the placeholder.
4. Check the draft on GitHub and publish it. Only a published release is
   `releases/latest`, so installed copies see the update from then on. A tag with a hyphen
   (`v0.4.0-beta.1`) becomes a prerelease, which the updater never offers.

The release notes in `latest.json` come from the tag message when the workflow runs;
editing the release on GitHub later does not change what the update dialog shows.

### Updater key

The maintainer makes the key once:

```bash
cd gui
npx tauri signer generate -w "$HOME/.tauri/doppelpad-updater.key"
```

It asks for a password and writes the private key (`doppelpad-updater.key`) and the public
key (`doppelpad-updater.key.pub`).

- The public key (the content of the `.pub` file) replaces the placeholder in
  `plugins.updater.pubkey` and is committed.
- The private key and the password go into the repository's secrets (*Settings → Secrets
  and variables → Actions*): `TAURI_SIGNING_PRIVATE_KEY` (the content of the key file) and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. They never go into the repository.
- Keep an offline backup of the key file and the password, for example in a password manager
  and on a USB stick. **A lost key cannot be replaced:** installed copies accept only setups
  signed with it, so they could never update again, and every user would have to download
  and install a new version by hand.

## Languages

The UI is available in English and German (*Settings → General → Language*). Translations live
in `gui/src/lib/i18n/{en,de}.json`. A test makes sure both files have the same keys and
placeholders. To add a language, copy `en.json`, translate it and register it in
`gui/src/lib/i18n/index.svelte.ts`. The tray menu takes its texts from the same files (`tray.*`).
