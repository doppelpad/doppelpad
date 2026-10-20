# Profile reference

A profile is a TOML file. Every section is optional except `[[mappings]]` entries you want.
Unknown keys are rejected, so typos are caught by `doppelpad check`.

```toml
name = "My game"
description = "..."
processes = ["MyGame.exe"]   # auto-apply while one of these is in the foreground (daemon)

[output]
virtual_pad = "xbox360"      # "xbox360" | "dual_shock4" | "none"
hide_physical = true         # hide the real pad via HidHide while active
passthrough = true           # mirror unmapped physical controls to the virtual pad
mute_mapped_inputs = true    # swallow keys/mouse buttons that trigger a mapping

[timing]
long_press_ms = 300
double_tap_ms = 150          # second press must start within this time after the first release
triple_tap_ms = 300
key_delay_ms = 10            # gap between key events in macros; length of a "tap"
turbo_ms = 200               # turbo period (press + release)
combo_window_ms = 50         # time to complete a combo

[sticks.left]                # same for [sticks.right]
deadzone = 0.0               # output deadzone (radial, rescaled); 0 = raw passthrough
outer = 1.0                  # deflection treated as full
anti_deadzone = 0.0          # minimum output once outside the deadzone
sensitivity = 1.0
curve = [[0.3, 0.15], [0.7, 0.55]]   # [input, output] points; empty = linear
invert_x = false
invert_y = false
rotation = 0.0               # degrees, counter-clockwise
zones = { low = 0.24 }       # where LeftStickUp etc. and the zones start (see Zones)
diagonals = false            # separate diagonal directions (LeftStickUpLeft, ...)
direction_width = 22.5       # degrees each side of an axis that count as that direction alone

[triggers.left]              # same for [triggers.right]
deadzone = 0.0
zones = { low = 0.03 }       # where LT / RT and the zones start (see Zones)
full_pull = 0.99             # LTFull / RTFull controls
effect = { kind = "resistance", start = 3, strength = 5 }   # DualSense; see below

# DualSense adaptive triggers (positions are tenths of the travel, 0 = released):
#   { kind = "off" }
#   { kind = "resistance", start = 0-9, strength = 1-8 }
#   { kind = "weapon", start = 2-7, end = start+1 to 8, strength = 1-8 }   # gives way at end
#   { kind = "vibration", start = 0-9, amplitude = 1-8, frequency = 1-255 }  # Hz
#   { kind = "raw", bytes = [11 numbers] }   # sent as they are
# A layer can set its own (`[layers.triggers.right] effect = ...`; `off` stops Main's).

[mouse]
zones = [200.0, 1000.0, 3000.0]   # counts/s where MouseMove:Low/Med/High start (Low also starts the directions)

[touchpad]
split = false                # left and right halves are two trackpads (Trackpad* controls)
zones = "radial"             # "radial" (sectors around the centre) | "grid" (3 × 3 cells)
centre = 0.35                # radial: radius of the centre region (fraction of half the pad)
click_required = false       # regions only while the pad is clicked

[gyro]
mode = "motion"              # "motion" (turning) | "tilt" (the controller is a stick)
steering = "roll"            # what GyroLeft/GyroRight follow: "roll" | "yaw"
initially_on = true
space = "local"              # gyro routes: "local" | "player" (turning around gravity)
motion_zones = [60.0, 250.0, 550.0]   # low, med, high in °/s
tilt_zones = [10.0, 22.0, 34.0]       # low, med, high in degrees from the centre

[virtual.keyboard]
repeat = "native"            # "native" | "fast" | "slow" | "off" | interval in ms (1–1000)

[virtual.sticks.left]        # same for [virtual.sticks.right]; how mappings push the stick
initial = 1.0                # deflection a button starts at
max = 1.0                    # largest deflection a held button reaches
speed = 0.0                  # growth per second while held (0 = no ramp)
acceleration = 0.0           # growth of that speed per second
multiplier_x = 1.0           # scales speed and acceleration horizontally
multiplier_y = 1.0           # ... and vertically
curve = []                   # output curve of the whole virtual stick; empty = linear

[virtual.sticks.left.jitter]  # a small offset that keeps moving around the stick
enabled = false
pattern = "circle"           # "circle" (8 points) | "square" (corners) | "cross"
min = 0.03                   # smallest and largest offset (fraction of full deflection)
max = 0.04
delay_min_ms = 30            # the offset moves on after a random time in this range
delay_max_ms = 40

[virtual.motion]
shake_g = 2.0                # strength of a shake action, in g
shake_ms = 150               # its length
```

`[gyro]` decides how the motion sensors become `Gyro*` controls. In `motion` mode they fire
while the controller turns, by its rotation rate. In `tilt` mode the controller is a stick:
they follow how far it is tilted from the centre pose (taken when the profile starts and by
`reset_tilt`), and gyro routes to a stick follow the tilt (45° = full deflection). Up is nose up,
Left/Right follow the steering axis, Lean is the roll. `space = "player"` makes gyro routes turn
around gravity, so aiming left and right works however the controller is held.

`[virtual.keyboard] repeat` sets how keys that mappings hold repeat, the way a physical
keyboard repeats a held key. Only the most recently pressed key repeats, and only while it
is held. `native` waits 500 ms, then repeats about every 30 ms with a little jitter;
`fast` (10 ms), `slow` (50 ms) and a number of milliseconds repeat at once at that interval;
`off` sends one key-down per press.

`[virtual.sticks.*]` shapes what stick actions (`{ stick = "left", x = 0.0, y = 1.0 }`) send.
While a button holds such an action, each axis starts at `initial` and grows by `speed` per
second (plus `acceleration` per second²), up to `max` and never beyond the action's own
deflection. The defaults push at once. Two actions pushing the same axis in opposite
directions cancel out; actions on different axes add up without being normalised. `curve`
then shapes everything the virtual stick sends, including the passed-through physical stick
and analog routes. A layer can change these fields like the stick settings, under
`[layers.virtual.sticks.left]`.

With `jitter` enabled, the stick gets a small offset that steps through the pattern after
random delays, all the time the layer is active, also with the stick at rest. Put it on a
hold layer to use it only while, say, aiming. A shake action rocks the virtual controller's
accelerometer along one axis for `shake_ms`, starting in the given direction and flipping
every 25 ms; only a virtual DualShock 4 has motion sensors.

## Controls (what can trigger a mapping)

| Syntax | Meaning |
|---|---|
| `A` `B` `X` `Y` | Face buttons by Xbox position. Aliases: `Cross`, `Circle`, `Square`, `Triangle` |
| `LB` `RB` `LS` `RS` | Shoulders, stick clicks. Aliases: `L1`, `R1`, `L3`, `R3` |
| `Back` `Start` `Guide` | Aliases: `View`, `Share`, `Create`, `Menu`, `Options`, `Home`, `PS` |
| `DpadUp` … `DpadRight` | D-pad |
| `Touchpad` `Misc` | Touchpad click, Share/Mute/Capture |
| `PaddleUL` `PaddleUR` `PaddleLL` `PaddleLR` | Back paddles (Elite, DualSense Edge) |
| `LeftStickUp` … `RightStickRight` | Stick pushed past its low zone threshold (a diagonal presses two, unless the stick has `diagonals = true`) |
| `LeftStickUpLeft` `LeftStickUpRight` `LeftStickDownLeft` `LeftStickDownRight` (same for `RightStick…`) | Diagonals of a stick with `diagonals = true` |
| `LeftStickUpLow` … `RightStickDownRightHigh` | Stick pushed in a direction (diagonals too) and within a zone (see Zones) |
| `LeftStickLow` `LeftStickMed` `LeftStickHigh` (same for `RightStick…`) | Stick within a zone, in any direction |
| `LT` `RT` / `LTFull` `RTFull` | Trigger past its low zone threshold / past `full_pull` |
| `LTLow` `LTMed` `LTHigh` (same for `RT…`) | Trigger pulled within a zone |
| `TouchUp` `TouchDown` `TouchLeft` `TouchRight` `TouchCenter` | Finger resting in a touchpad region (four 90° regions around the centre) |
| `TouchUpLeft` `TouchUpRight` `TouchDownLeft` `TouchDownRight` | Corner regions; mapping any of them splits the touchpad into eight 45° regions |
| `TouchTap` `TouchTapLeft` `TouchTapCenter` `TouchTapRight` `TouchMultiTap` | Quick tap (under 200 ms, barely moving): anywhere, in the left, middle or right third, or with two fingers; a 50 ms press |
| `TouchSwipeUp` … `TouchSwipeRight`, `TouchSwipe2Up` … | Swipe with one or two fingers across a quarter of the pad within 500 ms; a 50 ms press |
| `TouchZoomIn` `TouchZoomOut` | Two fingers spreading apart or pinching together by a fifth; a 50 ms press |
| `TrackpadLeftUp` … `TrackpadRightCenter`, `TrackpadLeftTap`, `TrackpadLeftClick` … | With `[touchpad] split`: the regions, taps and clicks of each half |
| `GyroUp` `GyroDown` `GyroLeft` `GyroRight` `GyroLeanLeft` `GyroLeanRight` | Controller turned (nose up/down, the steering axis, roll) past the low zone, or in tilt mode tilted past it; see `[gyro]` |
| `GyroLow` … `GyroUpHigh`, `GyroLeanLeftMed` … | Gyro in a zone, any direction or one; see `[gyro]` |
| `Key:Space` | Keyboard key (names below) |
| `Mouse:Left` `Mouse:Right` `Mouse:Middle` `Mouse:X1` `Mouse:X2` | Mouse buttons |
| `Wheel:Up` `Wheel:Down` `Wheel:Left` `Wheel:Right` | One wheel notch (50 ms pulse) |
| `MouseMove:Up` … `MouseMove:DownRight` | Physical mouse moving in a direction; mapping a diagonal makes the mouse tell eight directions apart (otherwise a diagonal movement triggers two) |
| `MouseMove:Low` `MouseMove:Med` `MouseMove:High`, `MouseMove:UpHigh` … | Mouse moving at a zone's speed (`[mouse] zones`, counts/s), any direction or one |

`trigger` is one control (`"A"`) or a combo (`["LB", "RB"]`, up to 4). A combo may mix devices,
such as `["Key:LeftAlt", "Key:W"]`.

**Key names** are US-layout positions:

- Letters `A`–`Z` and digits `Digit0`–`Digit9` (or simply `"0"`–`"9"`).
- `F1`–`F24`, `Escape`, `Tab`, `CapsLock`, `LeftShift`, `LeftCtrl`, `LeftAlt`, `LeftMeta`,
  `RightAlt`, `RightCtrl`, `RightShift`, `Space`, `Enter`, `Backspace`.
- `Minus`, `Equal`, `LeftBracket`, `RightBracket`, `Backslash`, `Semicolon`, `Apostrophe`, `Grave`,
  `Comma`, `Period`, `Slash`, `IntlBackslash`.
- `Insert`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, `Up`, `Down`, `Left`, `Right`.
- `PrintScreen`, `ScrollLock`, `Pause`, `NumLock`, `Numpad0`–`Numpad9`, `NumpadAdd`,
  `NumpadSubtract`, `NumpadMultiply`, `NumpadDivide`, `NumpadDecimal`, `NumpadEnter`.
- Media keys: `VolumeUp`, `VolumeDown`, `VolumeMute`, `MediaPlayPause`, `MediaNext`,
  `MediaPrevious`, `MediaStop`.

Aliases: `Ctrl`, `Shift`, `Alt`, `Win`, `Esc`, `Return`.

### Zones

A stick's deflection and a trigger's pull are divided into three zones: low, medium and high.
The zone controls are active while the stick or trigger is inside that zone, so a mapping can
tell "lightly pushed" from "pushed far":

```toml
[sticks.left]
zones = { low = 0.24, med = 0.5, high = 0.8 }

[[mappings]]                 # walk
trigger = "LeftStickUpLow"
actions = [{ key = "W" }]

[[mappings]]                 # pushed further: walk faster (medium zone)
trigger = "LeftStickUpMed"
actions = [{ key = "W" }]

[[mappings]]                 # pushed far: run
trigger = "LeftStickUpHigh"
actions = [{ key = "LeftShift" }, { key = "W" }]

[[mappings]]                 # right trigger pulled all the way
trigger = "RTHigh"
actions = [{ mouse = "left" }]
```

- Each zone starts at its threshold and ends where the next one starts. The zones do not
  overlap: a stick is in at most one of them. A value exactly on a threshold is in the higher
  zone.
- `low` is also where the plain controls start: `LeftStickUp` or `RT` is active in all three
  zones. Below `low` is the deadzone.
- Thresholds are fractions of full travel. `med` and `high` are optional. Without them the
  range from `low` to the end is split into thirds; for triggers the end is `full_pull`.
  The defaults (stick 0.24 / 0.49 / 0.75, trigger 0.03 / 0.35 / 0.67) follow from this rule.
- A stick threshold can be one number (a circle) or `[x, y]` (an ellipse with separate
  horizontal and vertical radii). A stick reaches the threshold when
  `(x / tx)² + (y / ty)² ≥ 1`.
- Direction zones (`LeftStickUpHigh`) need both: the stick points that way (a diagonal counts
  for both of its directions) and is in that zone. `LeftStickHigh` ignores the direction.
- Zone controls behave like any other control. They work with all activators and modes, in
  combos (`["LB", "RTHigh"]`) and in layers.
- Mapping a stick's direction or zone replaces that stick's output on the virtual pad, like a
  button mapping does. The same holds for a trigger. Add a stick or trigger action to keep it.

## Mappings

```toml
[[mappings]]
layer = 0                 # default 0 (base layer)
trigger = "A"
activator = "single"      # single | long | double | triple | start | release
mode = "hold"             # hold | turbo | toggle | once | repeat | while_held
actions = [ ... ]
jump = { layer = 1, mode = "hold" }   # optional layer change
turbo_ms = 80             # optional overrides …
repeat = 3
repeat_pause_ms = 100
random_ms = 20            # each turbo or repeat cycle waits up to this much longer, at random
long_press_ms = 500
double_tap_ms = 200
triple_tap_ms = 350
key_delay_ms = 20
description = "free text"
```

### Activators

Several mappings can share a trigger with different activators.

| Activator | Fires |
|---|---|
| `single` | On press. If the trigger also has `long`/`double`/`triple`, it fires once those are ruled out, as a short tap. |
| `long` | When held for `long_press_ms`; ends on release. |
| `double` | On the second press. That press is held as long as the button is. |
| `triple` | On the third press. |
| `start` | Immediately on every press, in parallel with the others. |
| `release` | On every release, as a short pulse. |

### Modes

| Mode | Behaviour |
|---|---|
| `hold` | Outputs are held while the trigger is held. Multiple keys are pressed in order with `key_delay_ms` between them. |
| `turbo` | Plain outputs pulse down/up every `turbo_ms` while held. A sequence is restarted every `turbo_ms`. |
| `toggle` | The first activation holds the outputs and the next releases them. A sequence loops until toggled off. |
| `once` | The sequence runs to completion; releasing the trigger does not cancel it. |
| `repeat` | The sequence runs `repeat` times with `repeat_pause_ms` between runs. |
| `while_held` | The sequence repeats while the trigger is held. |

### Actions

| Action | Example |
|---|---|
| Key | `{ key = "Space" }`, `{ key = "W", action = "down" }` |
| Gamepad button | `{ pad = "A" }` |
| Mouse button | `{ mouse = "left" }` |
| Wheel | `{ wheel = "up", clicks = 1 }` |
| Stick deflection | `{ stick = "left", x = 0.0, y = 1.0 }` (up is +y); added to the physical stick. `absolute = true` sets the axes it names (both when centred) instead, so the physical stick and routes do not reach them. Held in a hold mapping, a small relative pull such as `{ stick = "right", x = 0.0, y = -0.15 }` on `RT` works as anti-recoil |
| Trigger pull | `{ trigger = "right", value = 0.5 }` |
| Mouse movement | `{ mouse_move = "right", speed = 800, acceleration = 1.0 }` (pixels/s while active; `acceleration` adds that much of the speed per second held, up to 20 ×) |
| Cursor position | `{ mouse_to = [0.5, 0.5] }` moves the cursor to a point of the primary screen (fractions); `{ mouse_clip = [0.25, 0.25, 0.75, 0.75] }` keeps it inside a rectangle while held. Windows only |
| Pause | `{ pause = 250 }` (ms) |
| Rumble | `{ rumble = 150, low = 0.8, high = 0.8 }` |
| Turn | `{ turn = -90.0 }`: turns in the game by that many degrees with mouse motion (positive = right), see `[virtual.mouse]` |
| Touchpad gesture | `{ touch = "swipe_left", ms = 250 }` (`tap`, `tap_left`, `tap_center`, `tap_right`, `multi_tap`, `swipe_up` … `swipe_right`, `swipe2_up` … `swipe2_right`, `zoom_in`, `zoom_out`): plays the gesture on the virtual DualShock 4's touchpad (default 250 ms, taps 60 ms) |
| Motion shake | `{ shake = "+y" }` (`+x` `-x` `+y` `-y` `+z` `-z`): shakes the virtual DualShock 4's accelerometer, see `[virtual.motion]` |
| Command | `{ command = "remap_off" }`, `{ command = "toggle_gyro" }`, `{ command = { launch = { path = "osk.exe", args = "" } } }` (all commands below) |
| Break | `{ break = true }` (splits the sequence into segments, see below) |

Commands:

| Command | Effect |
|---|---|
| `remap_off` | Turn remapping off (release everything, unload the profile) |
| `toggle_gyro`, `gyro_on`, `gyro_off` | Switch the gyro controls and routes of the profile |
| `reset_tilt` | Take the controller's pose as the tilt centre |
| `switch_gyro_axis` | Swap the gyro steering axis between roll and yaw |
| `screenshot` | Save the monitor that shows the focused window as PNG in `Pictures\Doppelpad` (Windows; exclusive-fullscreen games may come out black) |
| `close_window` | Ask the focused window to close, like its close button |
| `{ crosshair = N }` | Show crosshair N (1 = the first of `[[crosshairs]]`), or hide it if it is the one showing |
| `toggle_crosshair` | Hide the crosshair that shows, or bring back the last one |
| `radial_next`, `radial_prev` | Open radial menu: point at the next / previous sector |
| `radial_up`, `radial_down`, `radial_left`, `radial_right` | Open radial menu: point at the sector in that direction |
| `radial_apply` | Run the pointed sector, or open its sub-menu |
| `radial_back` | Back to the parent menu, or close the menu |
| `pad_overlay` | Show or hide the live controller overlay (the app's *Settings → Overlays*) |
| `mappings_overlay` | Show or hide the list of the active layer's mappings over the game |
| `power_off` | Turn this controller off (wireless DualShock 4, DualSense, Switch controllers and Xbox controllers; wired ones stay on) |
| `shutdown`, `restart`, `sleep`, `hibernate`, `log_off` | Power commands; applications may still ask to save their work (Linux: `systemctl`, `loginctl`) |
| `{ launch = { path, args } }` | Start a program |

**Break** cuts a sequence into segments. Each activation plays the next segment, and after
the last one it starts over with the first (for example "1, break, 2, break, 3" cycles
through three weapons on one button). In `hold` mode a segment is held until the trigger
is released; in `once` mode it runs once. Sequences with breaks cannot use `turbo` or
`toggle` (they are held instead) and do not repeat (`repeat` and `while_held` run each
segment once). Changing layers and stopping macros start over at the first segment.

`action` for key, pad, mouse, stick and trigger:

- `auto` (the default): held in hold/toggle/turbo modes, a tap in sequence modes.
- `tap`
- `down`
- `up`

A mapping with no actions and no jump *unmaps* its trigger: the trigger is consumed, and keys
or buttons are swallowed.

## Layers

```toml
[[layers]]
id = 1
name = "Shift"
inherit = true                    # base-layer mappings apply unless overridden
not_inherited = ["A", ["LB", "RB"]]

[[mappings]]
trigger = "Back"
jump = { layer = 1, mode = "hold" }      # hold | toggle | switch
```

- `hold`: layer 1 is active while Back is held.
- `toggle`: each press switches between the layers.
- `switch`: a permanent jump. Add another mapping to come back.

A jump can wait: `jump = { layer = 1, mode = "hold", delay_ms = 300 }` changes the layer only
once the trigger has been held for 300 ms. The mapping's own actions still start at the press;
with `postpone = true` they start at the jump instead, and nothing at all happens if the
trigger is released before the delay is over.

When a layer is active, a trigger that has *any* mapping in that layer uses only that layer's
mappings. Otherwise the base layer's mappings apply, unless the trigger is listed in
`not_inherited`. Buttons held during a switch are re-pressed under the new layer. To have them
keep what they do until they are released instead (the new layer applies from their next
press):

```toml
[layer_change]
repress = false
```

An *unlock* layer gives the physical keyboard and/or mouse back while it is active, for
example to use the real mouse for a moment in a profile that turns it into a stick:

```toml
[[layers]]
id = 2
name = "Mouse back"
unlock = "mouse"                  # all | keyboard | mouse
```

While it is active, keys or mouse input it covers reach other programs unchanged and the base
layer's keyboard or mouse mappings (including mouse routes) do not apply. The layer's own
mappings still work. Unlock layers are meant to be held.

A layer can also change stick and trigger settings. Every field it leaves out is the base
layer's (this does not depend on `inherit`), and the settings change the moment the layer
becomes active:

```toml
[[layers]]
id = 1
name = "Aim"
inherit = true

[layers.sticks.right]             # any [sticks.*] field
sensitivity = 0.5
zones = { low = 0.15 }

[layers.triggers.right]           # any [triggers.*] field
full_pull = 0.8
```

A stick direction or trigger that is held across the switch and no longer reaches the new
layer's threshold is released at once.

## Light bar

DualShock 4 and DualSense controllers show the profile's light bar, and a layer can have its own
while it is active:

```toml
[lightbar]
mode = "breath"            # off | solid | breath | rainbow | shimmer | heartbeat | strobe
colors = ["#14b8a6", "#8b5cf6"]   # 1–10
cycle = true               # one colour per period, in turn
period_ms = 2000           # 100–10000

[[layers]]
id = 1
lightbar = { mode = "solid", colors = ["#f97316"] }
```

Without a `[lightbar]` the game sets the colour through the virtual DualShock 4. The player
lights and the DualSense microphone LED are app settings (*Settings → Controller lights*): the
player lights show the active slot's number (by default only while remapping is on), and the
microphone LED can show whether remapping is on. Light changes work over USB and Bluetooth.

## Vibration

```toml
[vibration]
left = 1.0          # left (heavy) motor strength, 0–1; 0 turns it off
right = 0.6         # right (light) motor
swap = false        # each motor plays the other one's rumble
min_delay_ms = 20   # shortest gap between rumble commands, 10–200 (the latest value wins)
```

This shapes the rumble the game sends to the virtual controller on its way to the physical
one; rumble that mappings play is not changed. DualShock 4 and DualSense rumble over USB and
Bluetooth.

## Crosshairs

Up to ten crosshairs can be drawn over the game, in the middle of the primary monitor:

```toml
[[crosshairs]]
name = "Scope"
shape = "circle_dot"      # cross, cross_gap, dot, circle, circle_dot, circle_cross, x, t,
                          # chevron, square, brackets
color = "#00ff66"
opacity = 1.0
size = 30                 # logical pixels, 10–200
thickness = 50            # line width in percent of a fifth of the size, 10–100
outline = 1               # pixels, 0–10
outline_color = "#000000"
offset = [0, 0]           # pixels right and down from the centre
visible = true            # false: only the magnifier
magnifier = { zoom = 2.0, size = 0.2 }   # optional; size relative to the screen height
layers = [1]              # shown while one of these layers is active
```

A crosshair shows while one of its `layers` is active, or after a `{ crosshair = N }`
command (see [Commands](#mappings)); a command's choice wins over the layers until the
profile changes. The magnifier enlarges the screen around the crosshair with the Windows
magnification API (Windows only); the crosshair stays on top of it. Crosshairs are drawn
by the app, not by `doppelpad daemon`.

## Radial menus

A radial menu is a ring of sectors over the game. It opens while its layer is active, so a
hold jump makes "hold LB, point, let go" menus:

```toml
[[mappings]]
trigger = "LB"
jump = { layer = 3, mode = "hold" }

[[radial_menus]]
name = "Weapons"
layer = 3
select = "right_stick"      # left_stick | right_stick | mouse | commands
apply = "release"           # release: the pointed sector runs when the layer ends
                            # command: only with `radial_apply`
delay_ms = 0                # show only after this long (quick presses don't flash it)
tint = 0.0                  # darken the screen behind it, 0–1
scale = 1.0                 # 0.5–2; 1 is 400 logical pixels across
offset = [0, 0]             # pixels from the centre of the primary monitor

[[radial_menus.sectors]]
label = "Rifle"
icon = "crosshair"          # a name of the app's icon set, or an image file
color = "#14b8a6"
actions = [{ key = "Digit1" }]

[[radial_menus.sectors]]
label = "More"
submenu = 2                 # opens menu 2 instead of running actions
```

Sector 1 is at the top, the others follow clockwise. A stick points once it is pushed half
way and the choice stays when it springs back; the mouse points after a short movement and
stops moving the cursor while the menu is open. While a stick points, it does not reach the
game and its analog routes pause. The radial commands (see [Commands](#mappings)) work in any
mode, for example mapped in the menu's layer. A sector runs its actions once, like a `once`
mapping; with `apply = "command"` the menu closes afterwards unless the sector says
`close = false`. A menu whose `layer` is 0 only opens as another menu's sub-menu. Radial
menus are drawn by the app, not by `doppelpad daemon`.

## Analog routes

```toml
[[analog]]
layer = 0
source = "right_stick"   # left_stick | right_stick | mouse | gyro | touchpad
target = "mouse"         # mouse | left_stick | right_stick | flick
sensitivity = 1.0
vertical_ratio = 1.0
curve = 1.0              # exponent on the normalized magnitude
deadzone = 0.1           # source deadzone (sticks, touchpad)
anti_deadzone = 0.0      # minimum output deflection (stick targets)
smoothing_ms = 0
decay_ms = 40            # mouse → stick: recentre after this long without motion
trackball_friction = 0.0 # touchpad → mouse: glide after lift-off (1 = no glide)
while_held = ["LT"]      # only while these controls are held
consume = true           # stop the source from also reaching the virtual pad / cursor
spring = true            # mouse → stick: return to the centre when the mouse stops
noise_filter = 0.0       # mouse → stick: ignore motion of fewer counts per tick
snap = 0.0               # stick targets: smaller deflections read as centred
acceleration = 0.0       # stick → mouse: at 90 % or more, speed up by this much per second (≤ 4 ×)
invert_x = false
invert_y = false
```

What `sensitivity = 1.0` means for each route:

| Route | Calibration |
|---|---|
| stick → mouse | 1000 px/s at full deflection |
| mouse → stick | 2000 counts/s give full deflection |
| gyro → mouse | 10 px per degree |
| gyro → stick | 100 °/s give full deflection |
| touchpad → mouse | a full-width swipe moves 1000 px |
| touchpad → stick | finger position relative to the pad centre |
| stick → flick | 1 = exact: the turn follows the stick's angle one to one |

With `target = "flick"` (flick stick, stick sources only), pushing the stick past `deadzone`
turns the view to face where the stick points, spread over `turn_ms`; while it stays out,
turning the stick turns the view by the same angle (smoothed by `smoothing_ms`). Letting the
stick back in re-arms it. `invert_x` turns the other way. The turn uses the calibration
below:

```toml
[virtual.mouse]
counts_per_turn = 18000.0   # mouse counts for one full 360° turn in this game
turn_ms = 100               # how long a flick or turn action takes
wheel_repeat_ms = 0         # held wheel actions repeat this often (0 = once per press)
```

The GUI's *Test a full turn* sends `counts_per_turn` after 3 seconds; tune it until the game
turns exactly once around. A turn action `{ turn = 90.0 }` (degrees, positive = right) uses the
same calibration.

## Importing JSON profiles

`doppelpad import file.json -o file.toml` converts JSON profiles (`schemaVersion` 3.0) from
other remapping tools. Files are recognized by their content, so the extension does not
matter. The importer reports
everything it could not represent, as diagnostics with JSON paths. These include radial-menu
icons, crosshair shapes (each imported crosshair is drawn as a cross), adaptive triggers,
LED settings and unsupported commands.

In that format a button that pushes a stick ramps up gradually by default, so imported
profiles get `[virtual.sticks.*]` settings with that ramp for every stick that buttons push.
