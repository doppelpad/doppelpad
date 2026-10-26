# Architecture

Doppelpad is split so that everything except device access is platform-independent and
deterministic.

```text
            ┌──────────────┐  JSON lines over a local socket   ┌──────────────────────┐
 CLI / GUI  │  dp-ipc      │ ◄───────────────────────────────► │  doppelpad daemon     │
            └──────────────┘                                    │  (runtime loop 1 kHz) │
                                                                └─────────┬────────────┘
                     ┌──────────────── profile (TOML / JSON) ─────────────┤
                     ▼                                                    ▼
              ┌────────────┐   compile    ┌────────────────────────────────────────────┐
              │ dp-config  │ ───────────► │ dp-core::engine                            │
              │ load/import│              │  controls → combos → recognizers → modes   │
              └────────────┘              │  → macro VM → output merge + passthrough   │
                                          └───────────────┬────────────────────────────┘
                                          Input ▲         │ Output
                                                │         ▼
              ┌───────────────────────────────────────────────────────────────────────┐
              │ dp-platform (traits)                                                  │
              │  GamepadBackend   VirtualPadFactory  KbmOutput  KbmCapture  DeviceHider│
              │  Windows: XInput, raw HID (Sony/Nintendo) · ViGEmBus · SendInput ·    │
              │           LL hooks + raw input · HidHide · foreground window          │
              │  Linux:   evdev · uinput (X360 pad, kbd/mouse, passthrough) ·         │
              │           EVIOCGRAB (hiding + capture) · X11 _NET_ACTIVE_WINDOW       │
              │  macOS:   IOKit HID (Sony/Nintendo/Steam/generic) · IOHID seize ·     │
              │           Quartz events + event tap · window list                     │
              └───────────────────────────────────────────────────────────────────────┘
```

## Crates

| Crate | Responsibility | OS code |
|---|---|---|
| `dp-core` | `PadState`, `Key`, `Control` names, `Profile` model, and the engine | none |
| `dp-config` | TOML (de)serialization, validation, JSON profile import, profile directories | none |
| `dp-platform` | Backend traits; Windows, Linux and macOS implementations; null backends elsewhere | yes |
| `dp-ipc` | `Request`/`Response` protocol and a tiny blocking client/server | local sockets |
| `dp-service` | The runtime loop, slots, auto-switching, IPC handling, live snapshots, HTTP API and DSU server; shared by the daemon and the GUI | none |
| `doppelpad` | CLI (apply, slots, remap on/off, status, devices), foreground runner, daemon | none |

## The engine (`dp-core::engine`)

`Engine::tick(Input) -> Output` is called about once per millisecond with:

- the merged physical pad state (`PadState`),
- the keyboard/mouse events since the last tick,
- a monotonic timestamp.

It returns the virtual pad state, key/mouse transitions, mouse motion, wheel, rumble and
host commands. There are no threads, clocks or I/O inside, so every behaviour is tested with
a fake clock (`engine/tests/`).

Pipeline per tick:

1. **Derive controls** (`derive.rs`). Digital controls come from analog inputs: stick directions use 45°
   sectors and a per-stick threshold, and there are trigger pull/full pull, touchpad regions,
   gyro directions, wheel pulses and mouse-motion directions.
2. **Combos** (`combo.rs`). A press of a combo member waits up to `combo_window_ms` (50 ms) for the rest of
   the combo. Larger combos win. When nothing completes, the pending presses are replayed
   with their original timestamps.
3. **Recognizers** (`recognizer.rs`). One per trigger. They turn edges into
   `Begin`/`End` for single/long/double/triple and run start/release in parallel. A single
   that has to wait for long/double is emitted as a short pulse.
4. **Activations** (`vm.rs`, `compile.rs`). Each binding is compiled into small programs
   (hold, run, turbo pulse). An activation runs its program on a tiny VM that understands
   down/up, analog contributions, waits, hold points, wheel, rumble and commands. Outputs
   are reference-counted, so overlapping mappings never release each other's keys.
5. **Layers** (`layers.rs`). Jumps are absolute ("go to layer N"). Hold and toggle shifts compile into a
   pair of jumps (a jump in, and a return jump in the target layer). The control that caused a jump is
   *jump-owned*: it gets no new press in the target layer, but its release activator fires
   there. Other held controls are re-pressed under the new layer.
6. **Output** (`compose.rs`, `routes.rs`). Unmapped physical controls are mirrored to the virtual pad (with stick and
   trigger shaping), then mapping outputs and analog routes are merged.

## Runtime (`dp-service`)

`Runtime` owns the `Platform` and one *session* per running profile: an `Engine`, its
virtual pad and its feedback state. The default session plays the shared slots on the
controller in use; a controller with slots of its own gets a session of its own. Each tick
the runtime:

1. polls the gamepad backends and binds each session to its controller,
2. runs the controller hotkeys on every controller (withholding complete combos),
3. drains captured keyboard/mouse events (they go to the default session),
4. ticks every session's engine,
5. feeds each session's output to its virtual pad (ViGEm / uinput), the shared
   keyboard/mouse output (`SendInput` / uinput) and its controller's rumble and light bar.

When the layer changes, it updates the capture filter, which decides which keys or buttons
are swallowed. It hides each session's physical pad (HidHide on Windows, `EVIOCGRAB` on
Linux) while its profile has a virtual pad. Everything is undone on `stop` and on drop.

Joy-Cons are reported one by one and combined by `dp_platform::joycon::JoyConCombiner`, which
wraps the HID backend on Windows and the evdev backend on Linux: a left and a right Joy-Con
become one controller (hidden together, rumble split between them), and a Joy-Con on its own
is turned into a sideways mini pad. Switch 2 controllers stay silent until their USB vendor
interface is told to start; the HID backend claims that interface next to the HID one.

Threads on every OS:

- **dp-service:** the runtime loop at about 1 kHz (on Windows with `timeBeginPeriod(1)`), in
  the daemon and in the GUI. The CLI's foreground runner ticks the runtime on its main
  thread instead.
- **dp-ipc:** the IPC server, in the daemon and in the GUI host. It answers one request
  after the other through the runtime thread; `dp-ipc-accept` takes connections and a
  `dp-ipc-conn` thread reads each request. A client waits for its answer on a
  `dp-ipc-call` thread (named pipes have no timeouts on Windows).
- **dp-http** and one **dp-http-conn** per connection: the HTTP API, when it is on.
- **dp-dsu:** the DSU (cemuhook) motion server, when it is on.
- **dp-drivers:** probes the drivers (ViGEmBus and HidHide, or uinput) and Bluetooth every
  5 s, so the runtime loop never waits on them.

On Windows:

- **one per HID controller:** runs its driver: start-up (for Switch pads the USB handshake,
  calibration reads and report-mode setup), overlapped report reads, rumble pacing and
  keep-alives. `set_rumble` only hands a value over, so the engine never waits on a device.
- **HID scanner:** device discovery every 2 s, so the loop never waits on SetupAPI.
- **hook thread:** low-level keyboard/mouse hooks and raw-input mouse deltas, connected to the
  loop by a lock-free filter and a channel.
- **ViGEm notification thread** per virtual pad: rumble (and the DualShock 4 light bar)
  coming back from games.
- **Bluetooth:** the advertisement watcher and the controllers' notifications call back on
  the thread pool; a `dp-ble-connect` thread sets up each Switch 2 controller that is found.

On Windows and macOS the HID controllers share one backend (`dp-platform/src/hidhost.rs`);
each OS brings the scanner that finds and opens them. On Linux it runs only the Switch 2
controllers, which no kernel driver starts: on USB through hidraw, and over Bluetooth.

On Linux:

- **one reader per evdev node:** gamepad, motion-sensor and touchpad nodes are merged per
  controller by their parent device.
- **evdev scanner:** discovery every 2 s.
- **uinput FF thread** per virtual pad: answers games' rumble uploads.
- **capture scanner plus one thread per grabbed keyboard/mouse:** these re-inject
  unsuppressed events through a passthrough device.
- **Bluetooth:** `dp-ble` follows BlueZ's D-Bus signals; a `dp-ble-connect` thread sets up
  each Switch 2 controller that is found, a `dp-ble-input` thread reads it, and its device
  thread runs in the HID backend as on Windows and macOS.

On macOS:

- **one per HID controller:** as on Windows, with the device's reports arriving on the
  thread's own run loop.
- **HID scanner:** device discovery every 2 s through an IOHIDManager.
- **event tap thread:** the keyboard/mouse event tap on a run loop of its own, connected to
  the loop by the same lock-free filter and channel as the Windows hooks.

## Windows drivers

| Need | Doppelpad on Windows |
|---|---|
| Virtual controller | ViGEmBus (Xbox 360, DualShock 4) |
| Hide the physical pad | HidHide (blacklist + our exe on the whitelist) |
| Switch 2 command interface | WinUSB through `nusb` (Windows binds it automatically; exclusive, so Steam may hold it) |
| Switch 2 over Bluetooth | WinRT (`Windows.Devices.Bluetooth`): an advertisement watcher finds a pad in pairing mode, which is connected without pairing and runs in the HID backend like the macOS one (`windows/ble.rs`, shared link in `switch2gatt.rs`); Windows 11 is asked for a faster connection, and a pad silent for 3 s is let go, as Windows notices a lost link only after about 10 s |
| Keyboard/mouse capture | low-level hooks (all devices; see limitations) |
| Keyboard/mouse output | `SendInput` with scan codes |

Known limitations of the user-mode approach:

- Hooks cannot tell keyboards apart.
- Games that read raw input still see mouse motion that the hook swallowed.
- Injected input is flagged as injected.
- Virtual Xbox One, DualSense, Switch Pro and DS3 pads need drivers that do not exist as open
  source.

The [Interception](https://github.com/oblitum/Interception) driver could lift the first two
limitations in a future backend.

## Linux interfaces

| Need | Doppelpad on Linux |
|---|---|
| Read controllers | evdev; hid-playstation / hid-nintendo motion and touchpad nodes merged in |
| Switch 2 on USB | hidraw for the input, the USB command interface through `nusb` (usbfs; exclusive, so Steam may hold it); hiding grabs the generic input node `usbhid` makes for it (`linux/hidraw.rs`, `linux/hid.rs`) |
| Switch 2 over Bluetooth | a BlueZ discovery (D-Bus, `zbus`) finds a pad in pairing mode; Doppelpad connects it on an L2CAP socket on its ATT channel and speaks ATT itself (`linux/att.rs`), as BlueZ's GATT client never finishes reading these pads; it runs in the HID backend like the Windows and macOS ones (`linux/ble.rs`, shared link in `switch2gatt.rs`) |
| Hide the physical pad | `EVIOCGRAB` through a duplicate of the reader's descriptor, so Doppelpad keeps reading |
| Virtual controller | uinput Xbox 360 pad (`045e:028e`, xpad layout) with `FF_RUMBLE` fed back |
| Keyboard/mouse capture | `EVIOCGRAB` per device + one uinput passthrough device each |
| Keyboard/mouse output | uinput keyboard + mouse |
| Foreground app | X11 / XWayland `_NET_ACTIVE_WINDOW` → `_NET_WM_PID` → `/proc` |

Every uinput device Doppelpad creates has a `phys` starting with `doppelpad/`. The input
backends skip those devices, the way the Windows backends skip ViGEm devices. Setup (udev
rule) and limitations are in [LINUX.md](LINUX.md).

## macOS interfaces

| Need | Doppelpad on macOS |
|---|---|
| Read controllers | IOKit HID: the Sony, Nintendo, Steam and generic drivers of the Windows backend, on the controllers' own reports; the stand-in Xbox 360 pads macOS makes (`GCSyntheticDevice`) are left out |
| Hide the physical pad | opening it exclusively (`kIOHIDOptionsTypeSeizeDevice`), which keeps it from the GameController framework too |
| Switch 2 over Bluetooth | CoreBluetooth on a serial dispatch queue: a pad in pairing mode is connected and runs in the HID backend, its input and command characteristics standing in for HID reports and the USB command channel (`macos/ble.rs`, `switch2gatt.rs`, `nintendo/switch2ble.rs`) |
| Virtual controller | none: a virtual HID device needs an entitlement from Apple |
| Keyboard/mouse capture | a Quartz event tap (Accessibility permission) |
| Keyboard/mouse output | `CGEventPost` with a marker in the user data, which the tap skips |
| Foreground app | the owner of the frontmost window in `CGWindowListCopyWindowInfo` → its executable, or the `.exe` under Wine |

Setup (permissions) and limitations are in [MACOS.md](MACOS.md).

## Porting

A new OS needs implementations of the six traits in `dp-platform`:

- `GamepadBackend`
- `VirtualPadFactory`
- `KbmOutput`
- `KbmCapture`
- `DeviceHider`
- `ForegroundWatcher`

Plus one `Platform::native()` arm. Nothing else changes.

- **Linux:** done (`dp-platform/src/linux`, see above). evdev provides reading and
  `EVIOCGRAB` (exclusive grab), and uinput provides the virtual X360 pad and the
  keyboard/mouse.
- **macOS:** done (`dp-platform/src/macos`, see above). A virtual gamepad needs Apple's
  entitlement for CoreHID virtual devices (`com.apple.developer.hid.virtual.device`), not a
  DriverKit extension.
