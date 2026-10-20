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

[triggers.left]              # same for [triggers.right]
deadzone = 0.0
zones = { low = 0.03 }       # where LT / RT and the zones start (see Zones)
full_pull = 0.99             # LTFull / RTFull controls
```

## Controls (what can trigger a mapping)

| Syntax | Meaning |
|---|---|
| `A` `B` `X` `Y` | Face buttons by Xbox position. Aliases: `Cross`, `Circle`, `Square`, `Triangle` |
| `LB` `RB` `LS` `RS` | Shoulders, stick clicks. Aliases: `L1`, `R1`, `L3`, `R3` |
| `Back` `Start` `Guide` | Aliases: `View`, `Share`, `Create`, `Menu`, `Options`, `Home`, `PS` |
| `DpadUp` … `DpadRight` | D-pad |
| `Touchpad` `Misc` | Touchpad click, Share/Mute/Capture |
| `PaddleUL` `PaddleUR` `PaddleLL` `PaddleLR` | Back paddles (Elite, DualSense Edge) |
| `LeftStickUp` … `RightStickRight` | Stick pushed past its low zone threshold (diagonals press two) |
| `LeftStickUpLow` … `RightStickRightHigh` | Stick pushed in a direction and within a zone (see Zones) |
| `LeftStickLow` `LeftStickMed` `LeftStickHigh` (same for `RightStick…`) | Stick within a zone, in any direction |
| `LT` `RT` / `LTFull` `RTFull` | Trigger past its low zone threshold / past `full_pull` |
| `LTLow` `LTMed` `LTHigh` (same for `RT…`) | Trigger pulled within a zone |
| `TouchUp` `TouchDown` `TouchLeft` `TouchRight` `TouchCenter` | Finger resting in a touchpad region |
| `GyroUp` `GyroDown` `GyroLeft` `GyroRight` | Controller turned quickly (> 60 °/s) |
| `Key:Space` | Keyboard key (names below) |
| `Mouse:Left` `Mouse:Right` `Mouse:Middle` `Mouse:X1` `Mouse:X2` | Mouse buttons |
| `Wheel:Up` `Wheel:Down` `Wheel:Left` `Wheel:Right` | One wheel notch (50 ms pulse) |
| `MouseMove:Up` … | Physical mouse moving in a direction |

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
| Stick deflection | `{ stick = "left", x = 0.0, y = 1.0 }` (up is +y) |
| Trigger pull | `{ trigger = "right", value = 0.5 }` |
| Mouse movement | `{ mouse_move = "right", speed = 800 }` (pixels/s while active) |
| Pause | `{ pause = 250 }` (ms) |
| Rumble | `{ rumble = 150, low = 0.8, high = 0.8 }` |
| Command | `{ command = "remap_off" }`, `{ command = "toggle_gyro" }`, `{ command = { launch = { path = "osk.exe", args = "" } } }` |
| Break | `{ break = true }` (splits the sequence into segments, see below) |

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

When a layer is active, a trigger that has *any* mapping in that layer uses only that layer's
mappings. Otherwise the base layer's mappings apply, unless the trigger is listed in
`not_inherited`. Buttons held during a switch are re-pressed under the new layer.

## Analog routes

```toml
[[analog]]
layer = 0
source = "right_stick"   # left_stick | right_stick | mouse | gyro | touchpad
target = "mouse"         # mouse | left_stick | right_stick
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

## Importing JSON profiles

`doppelpad import file.json -o file.toml` converts JSON profiles (`schemaVersion` 3.0) from
other remapping tools. Files are recognized by their content, so the extension does not
matter. The importer reports
everything it could not represent, as diagnostics with JSON paths. These include radial
menus, crosshairs, right trackpads, adaptive triggers, flick stick, LED settings and
unsupported commands.
