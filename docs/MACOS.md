# macOS

Doppelpad runs on macOS 13 and later, on Apple silicon and Intel Macs, with only the
interfaces macOS has. No drivers are needed:

| Need | macOS backend (`crates/dp-platform/src/macos`) |
|---|---|
| Read controllers | IOKit HID: the same drivers as on Windows read the controllers' own reports |
| Switch 2 controllers over Bluetooth | CoreBluetooth: Doppelpad connects them itself (see [below](#switch-2-controllers-over-bluetooth)) |
| Hide the physical controller | opening it exclusively (seizing it), which keeps it from every other program, the GameController framework included |
| Virtual controller | not available yet (see [Virtual controller](#virtual-controller)) |
| Keyboard / mouse output | Quartz events (`CGEventPost`) |
| Keyboard / mouse capture | a Quartz event tap |
| Per-game profiles | the frontmost window in the window server's list → its process |

> **Status:** preview. Tried on a Mac with macOS 26 with a DualSense, a Switch Pro
> Controller and a Switch 2 Pro Controller on USB, and the Switch Pro Controller over
> Bluetooth too: reading them (buttons, sticks, triggers, motion, battery), the DualSense's
> light bar, the Switch controllers' player lights, rumble on all three, hiding them from the
> GameController framework (which does not read the Switch 2 Pro Controller at all); and in
> the app with the Accessibility permission: the controller as mouse and keyboard (motion,
> clicks, wheel, keys, media keys), a key remapped on the keyboard, the program in front,
> keeping the Mac awake and noticing the lock screen, and notifications over other programs
> without taking their focus. The Switch 2 Pro Controller over Bluetooth was tried in the app:
> the permission (at once, and Bluetooth on by itself from the next start), connecting in
> pairing mode, every button, the sticks, triggers and motion, player lights, rumble without
> gaps and in three strengths, the battery level, going away when turned off, and staying with
> its console when woken. Also tried: overlays (crosshair, controller overlay) over a video in
> full screen, without taking its focus; the *screenshot*, *close window* and *sleep*
> commands. Shutting down, restarting, logging out, games in full screen and the other
> controllers are untested. Reports from Mac users are very welcome.

## Install

`Doppelpad_<version>_universal.dmg` holds the app: open it and drag Doppelpad into
Applications. One app runs on Apple silicon and on Intel Macs. It hosts the remapping
service itself, keeps running in the menu bar, and updates itself: when a new version is out
it asks, then replaces the app and starts again.

`doppelpad_<version>_macos-universal.tar.gz` holds the command-line tool `doppelpad`
(`doctor`, `daemon`, `status`, …), for checking the setup and for running without the app.

Until the app is signed with an Apple Developer ID, macOS will not open it with a double
click at first: open it once, then choose *Open Anyway* in System Settings › Privacy &
Security. That should be needed once: updates the app downloads and installs itself carry
no download quarantine (to be confirmed with the first macOS release). A tool from the
`.tar.gz` that macOS refuses to run likewise needs `xattr -d com.apple.quarantine doppelpad`
once.

## Permissions

Reading and hiding controllers needs no permission. The rest does, granted per program in
System Settings › Privacy & Security:

| Permission | Needed for |
|---|---|
| Accessibility | sending keys and mouse input, capturing keyboard and mouse, the *close window* command |
| Screen & System Audio Recording | the *screenshot* command, and the window titles in the game picker (without it, programs are listed by name) |
| Bluetooth | Switch 2 controllers over Bluetooth (nothing else: other controllers pair in macOS's own settings) |

The app shows a missing Accessibility permission in Settings › Drivers, in the welcome guide
and at the bottom of the window. *Open settings* asks for it: macOS adds Doppelpad to the
list, where it only has to be turned on. `doppelpad doctor` reports it too. macOS tells a
running program of the permission only when it starts again, so quit Doppelpad (⌘Q or *Quit*
in the menu bar) and open it again after granting it.

Bluetooth is listed there too, as optional: Doppelpad never turns Bluetooth on by itself before
the permission is given. *Allow* brings up macOS's prompt; it takes effect at once. Once
refused, *Allow* opens System Settings › Privacy & Security › Bluetooth instead.

The permission belongs to the program as it is signed. An app signed ad hoc is a new program
to macOS after every update, so the permission has to be granted again. Release builds signed
the same way every time (see [Signing releases](#signing-releases)) keep it.

For the command-line tool, macOS gives the permission to the terminal it runs in (Terminal,
iTerm, …), not to `doppelpad` itself.

## Controllers

Doppelpad reads controllers through IOKit HID, with the drivers of its Windows backend:

- DualShock 4, DualSense and DualSense Edge, with motion, touchpad, light bar, player lights,
  microphone LED, trigger effects and rumble.
- Nintendo Switch Pro Controller, Joy-Cons (paired automatically, or sideways on their own),
  Nintendo Switch Online and Switch-style wired pads, with motion and rumble.
- Switch 2 Pro Controller and GameCube controller on USB. They stay silent until they are
  started over their USB command channel, which macOS leaves free, so Doppelpad claims it. Only
  one program can: while Steam holds it, Doppelpad takes over once Steam lets go. The Pro
  Controller connects over Bluetooth too (below).
- The Steam Controller.
- Every other HID game pad or joystick, read through its report descriptor: Xbox controllers
  over Bluetooth are among them, as macOS has no XInput. Identify their buttons once with the
  wizard in the app.

Not yet: the GameCube controller adapter, and Xbox controllers over USB, which macOS drives
itself (they are not HID devices).

### Switch 2 controllers over Bluetooth

Over Bluetooth, Switch 2 controllers speak Bluetooth Low Energy with Nintendo's own protocol
rather than HID, and drop the link when a computer tries to pair them the standard way: macOS
does not list them at all. Doppelpad connects them itself:

1. Allow Bluetooth in Settings › Drivers (once).
2. Hold the small pairing button on top of the controller until its lights run. Doppelpad
   connects it within a few seconds and lights player 1.

It works without pairing, so the controller stays paired with the console, and has to be put
into pairing mode again after it was off. A controller that only looks for its console (a
button pressed while it is off) is left alone, so Doppelpad never takes it from a Switch 2
nearby.

- Doppelpad scans for controllers in pairing mode while it runs. If you don't connect one over
  Bluetooth, turn *Look for Switch 2 controllers over Bluetooth* off in Settings › Controllers:
  the scan stops, and controllers already connected stay.
- Only the Pro Controller for now; the GameCube controller and the Joy-Con 2 are not connected.
- macOS sets the connection's pace, which the controller cannot change: about 33 reports a
  second, against 250 over USB. That is enough for menus and most games, but less smooth in
  fast ones; use the cable there.
- Input, motion and player lights work as over USB, rumble too, with each report carrying
  three samples to bridge the longer gaps. No other program sees the controller, so there is
  nothing to hide.
- The battery level is worked out from the voltage the controller reports, as it reports no
  level of its own in the report Doppelpad reads; it reads high while charging.

macOS 14 and later make a stand-in Xbox 360 pad for each controller they support
(`GCSyntheticDevice`), for older games. Doppelpad leaves those out, so no controller is read
twice.

### Hiding

To hide a controller, Doppelpad opens it exclusively: macOS calls it seizing. While it does,
no other program gets the controller's input: the GameController framework (which most Mac
games use), Steam and games that read HID devices themselves see a controller that does
nothing. Doppelpad keeps reading it. Letting go of it gives it back at once.

As macOS has no virtual controllers yet (below), Doppelpad hides the controller for every config
that has *Hide the physical controller* on, not only for one with a virtual controller as on
Windows and Linux: a game then gets only the keys and mouse input the mappings make. Turn the
option off in a config that should leave the controller to the game and only add a few
mappings on top.

## Virtual controller

macOS has no driver for virtual controllers that anyone may use. A program can make a virtual
HID device with CoreHID or IOKit only with an entitlement Apple grants on request
(`com.apple.developer.hid.virtual.device`); Doppelpad does not have one yet. So on macOS every
config runs with keyboard and mouse only: one that names a virtual controller (the default)
starts all the same, without it, and keeps it for Windows and Linux. The app shows *No virtual
controller* as chosen and offers only the templates that need none.

Even with the entitlement, the GameController framework is built to ignore virtual devices, so
a virtual DualShock 4 might reach games that read HID devices themselves (SDL, emulators,
Steam) but not every Mac game.

## Keyboard and mouse

Output goes into the system's event stream as Quartz events: keys by their position on the
keyboard with the modifier flags a Mac keyboard sets, mouse buttons with click counts for
double clicks, motion with the deltas games read, the wheel in lines.

Capture is an event tap: it sees every key, button, wheel and motion and swallows the ones a
config remaps. Swallowing mouse motion (for mouse-to-stick mappings) detaches the cursor from
the mouse while it lasts.

Limitations:

- Programs that read keyboard and mouse from the HID devices themselves (the GameController
  framework's `GCKeyboard` and `GCMouse`) do not see Doppelpad's output, and still see input
  Doppelpad swallowed.
- Keyboards and mice are not told apart.
- While a password field has secure input on, macOS keeps keys from every event tap.
- Keys a Mac keyboard does not have (Print Screen, Scroll Lock, Pause, the launch and browser
  keys, media Stop) cannot be sent. Play/pause, next, previous, volume and mute go out as the
  system events a Mac keyboard's own keys make, and are captured as such.

## Per-game profiles

The program in front is the owner of the frontmost ordinary window; this needs no permission.
Its name is the file name of its executable: for an app, the one in its
`Contents/MacOS` folder, which is usually the app's name. For games under Wine (CrossOver,
Whisky, the Game Porting Toolkit) it is the Windows `.exe` name, as on Windows. Add the Mac
name to a game's programs: the game picker in the app lists the programs with a window, and
suggests the games Steam and Epic have installed, with the programs in their app bundles.

## Commands

| Command | On macOS |
|---|---|
| Shut down, restart, log out | asks `loginwindow`, so programs may still save their work |
| Sleep | `pmset sleepnow` |
| Hibernate | not available: macOS has no hibernate of its own to ask for |
| Screenshot | the display with the window in front, to `~/Pictures/Doppelpad` (needs Screen Recording) |
| Close window | presses the focused window's close button (needs Accessibility) |

Doppelpad keeps the Mac awake while mappings run, and pauses while the screen is locked.

## Desktop app

The app keeps running in the menu bar when its window is closed, and leaves the Dock until
the window opens again. *Open at login, in the menu bar* in the settings adds a launch agent
(`~/Library/LaunchAgents/dev.doppelpad.app.plist`) that starts it hidden; it shows among the
login items in System Settings › General.

Overlays (notifications, crosshairs, radial menus, the controller overlay) float above every
window, on every Space and over games in full screen, without taking the focus from the game.
They are non-activating panels: an ordinary window, even one that joins every Space, stays out
of another program's full-screen Space.
The crosshair's magnifier is not available on macOS.

Settings, configs, logs and the history of configs are in
`~/Library/Application Support/doppelpad`, for the app and the command-line tool alike (a
shell's XDG variables do not move them: the app, started from the Finder, never sees those).

## Command-line tool

`doppelpad doctor` lists what macOS allows and the controllers it finds; `doppelpad daemon`
runs remapping without the app. Keyboard and mouse need the Accessibility permission for the
terminal it runs in, Switch 2 controllers over Bluetooth its Bluetooth permission. The tool
never asks for the latter itself; it uses Bluetooth once the terminal has it. The daemon and the app talk through a socket in the user's temporary
folder.

## Building

Requirements: Rust, Node.js 20 or newer and the Xcode command-line tools
(`xcode-select --install`).

```bash
cargo build --release   # → target/release/doppelpad
cd gui
npm install
npx tauri build --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

The app is `target/release/bundle/macos/Doppelpad.app`. The `--config` leaves out the
updater's archive, which needs the release signing key. `--target universal-apple-darwin`
builds one app for both kinds of Mac, with both Rust targets installed. The release workflow
builds the `.dmg` and the updater's archive (see `.github/workflows/release.yml`).

### Signing releases

macOS ties the permissions a user grants (Accessibility, Bluetooth, Screen Recording) to how
the app is signed. A build signed ad hoc, as a local one is, gets a signature of its own, so
after an update macOS takes it for another program: the permissions have to be granted again,
and Screen Recording just fails until its old entry is removed. Release builds therefore
should always be signed with the same certificate.

Until there is an Apple Developer ID, the release workflow signs with a self-signed
certificate. Make it once, on a Mac:

```bash
sh gui/scripts/macos-signing-certificate.sh
```

It writes `doppelpad-signing.p12` and its password to `~/doppelpad-signing` and lists the three
repository secrets to set: `MACOS_CERTIFICATE`, `MACOS_CERTIFICATE_PASSWORD` and
`MACOS_SIGNING_IDENTITY`. Keep both files: a new certificate is a new program to macOS once more.
The workflow trusts the certificate on its runner only (nothing on a user's Mac does), signs
the app with it and logs the signature. Its designated requirement, which macOS ties the
permissions to, names the certificate rather than the build (`identifier "dev.doppelpad.app"
and certificate leaf = H"…"`, from the first such build), so every release signed with it
meets it and the permissions should stay across updates (to be confirmed with the first two
macOS releases). Gatekeeper still asks before the first start, as for any app without a
Developer ID.

With a Developer ID, the `APPLE_*` secrets (listed in the workflow) take over: the app is
signed with it and notarized, and Gatekeeper no longer asks. Switching from the self-signed
certificate to the Developer ID changes the signature once more, so users grant the
permissions one last time.
