//! Fake controllers and virtual pads for runtime tests.

use std::sync::{Arc, Mutex};

use dp_core::control::Control;
use dp_core::pad::{PadButton, PadState, Rgb, Rumble};
use dp_core::profile::{Action, Command, Mapping, Mode};
use dp_platform::{
    DeviceId, DeviceInfo, GamepadBackend, Leds, NullPlatform, PadFamily, PadSnapshot, Platform,
    TriggerEffects, VirtualPad, VirtualPadFactory, VirtualPadKind,
};

use super::Runtime;

/// Everything the fakes saw, shared with the test.
#[derive(Default)]
pub struct World {
    pub pads: Vec<PadSnapshot>,
    pub rumble: Vec<(DeviceId, Rumble)>,
    pub lightbar: Vec<(DeviceId, Option<Rgb>)>,
    pub leds: Vec<(DeviceId, Leds)>,
    pub triggers: Vec<(DeviceId, TriggerEffects)>,
    /// Virtual pads created so far, with the last state each was sent.
    pub virtual_pads: Vec<(VirtualPadKind, Option<PadState>)>,
    /// What the game "sends" to the next virtual pad poll.
    pub game_rumble: Option<Rumble>,
    pub game_lightbar: Option<Rgb>,
    /// Keys sent to the virtual keyboard, in order.
    pub keys: Vec<(dp_core::keys::Key, bool)>,
    /// System commands carried out, by name.
    pub system: Vec<String>,
    /// Controllers told to turn off.
    pub powered_off: Vec<DeviceId>,
    /// Instance paths hidden from games right now.
    pub hidden: Vec<String>,
}

pub type Shared = Arc<Mutex<World>>;

pub fn pad(id: &str, family: PadFamily) -> PadSnapshot {
    PadSnapshot {
        info: Arc::new(DeviceInfo {
            id: DeviceId(id.into()),
            name: id.into(),
            family,
            vendor_id: 0,
            product_id: 0,
            backend: "fake".into(),
            instance_paths: vec![format!("path-{id}")],
            joycon: None,
            has_motion: true,
            has_touchpad: true,
            has_rumble: true,
        }),
        state: PadState::default(),
        battery: None,
        raw: None,
    }
}

struct FakeBackend(Shared);

impl GamepadBackend for FakeBackend {
    fn name(&self) -> &'static str {
        "fake"
    }
    fn poll(&mut self, out: &mut Vec<PadSnapshot>) {
        out.extend(self.0.lock().unwrap().pads.iter().cloned());
    }
    fn set_rumble(&mut self, id: &DeviceId, rumble: Rumble) {
        self.0.lock().unwrap().rumble.push((id.clone(), rumble));
    }
    fn set_lightbar(&mut self, id: &DeviceId, color: Option<Rgb>) {
        self.0.lock().unwrap().lightbar.push((id.clone(), color));
    }

    fn set_leds(&mut self, id: &DeviceId, leds: Leds) {
        self.0.lock().unwrap().leds.push((id.clone(), leds));
    }

    fn set_trigger_effects(&mut self, id: &DeviceId, effects: TriggerEffects) {
        self.0.lock().unwrap().triggers.push((id.clone(), effects));
    }

    fn power_off(&mut self, id: &DeviceId) -> bool {
        self.0.lock().unwrap().powered_off.push(id.clone());
        true
    }
}

struct FakeFactory(Shared);

impl VirtualPadFactory for FakeFactory {
    fn available(&self) -> dp_platform::Result<()> {
        Ok(())
    }
    fn create(&mut self, kind: VirtualPadKind) -> dp_platform::Result<Box<dyn VirtualPad>> {
        let mut w = self.0.lock().unwrap();
        w.virtual_pads.push((kind, None));
        Ok(Box::new(FakePad {
            world: self.0.clone(),
            index: w.virtual_pads.len() - 1,
            kind,
        }))
    }
}

struct FakePad {
    world: Shared,
    index: usize,
    kind: VirtualPadKind,
}

impl VirtualPad for FakePad {
    fn kind(&self) -> VirtualPadKind {
        self.kind
    }
    fn update(&mut self, state: &PadState) -> dp_platform::Result<()> {
        self.world.lock().unwrap().virtual_pads[self.index].1 = Some(*state);
        Ok(())
    }
    fn take_rumble(&mut self) -> Option<Rumble> {
        self.world.lock().unwrap().game_rumble.take()
    }
    fn take_lightbar(&mut self) -> Option<Rgb> {
        self.world.lock().unwrap().game_lightbar.take()
    }
}

struct FakeSystem(Shared);

impl dp_platform::SystemControl for FakeSystem {
    fn power(&mut self, action: dp_platform::PowerAction) -> dp_platform::Result<()> {
        self.0.lock().unwrap().system.push(format!("{action:?}"));
        Ok(())
    }
    fn screenshot(&mut self) -> dp_platform::Result<std::path::PathBuf> {
        self.0.lock().unwrap().system.push("Screenshot".into());
        Ok("shot.png".into())
    }
    fn close_foreground_window(&mut self) -> dp_platform::Result<()> {
        self.0.lock().unwrap().system.push("CloseWindow".into());
        Ok(())
    }
    fn keep_awake(&mut self, on: bool) {
        self.0.lock().unwrap().system.push(format!("awake {on}"));
    }
}

struct FakeHider(Shared);

impl dp_platform::DeviceHider for FakeHider {
    fn available(&self) -> dp_platform::Result<()> {
        Ok(())
    }
    fn hide(&mut self, instance_paths: &[String]) -> dp_platform::Result<()> {
        self.0.lock().unwrap().hidden = instance_paths.to_vec();
        Ok(())
    }
    fn restore(&mut self) -> dp_platform::Result<()> {
        self.0.lock().unwrap().hidden.clear();
        Ok(())
    }
}

struct FakeKbm(Shared);

impl dp_platform::KbmOutput for FakeKbm {
    fn key(&mut self, key: dp_core::keys::Key, down: bool) {
        self.0.lock().unwrap().keys.push((key, down));
    }
    fn mouse_button(&mut self, _: dp_core::keys::MouseButton, _: bool) {}
    fn mouse_move(&mut self, _: f32, _: f32) {}
    fn wheel(&mut self, _: dp_core::keys::WheelDir, _: f32) {}
}

/// Null backends plus fake controller, virtual pad, hiding and keyboard backends.
pub fn platform() -> (Platform, Shared) {
    let world = Shared::default();
    let mut p = NullPlatform::platform();
    p.gamepads.push(Box::new(FakeBackend(world.clone())));
    p.virtual_pads = Box::new(FakeFactory(world.clone()));
    p.hider = Box::new(FakeHider(world.clone()));
    p.kbm_out = Box::new(FakeKbm(world.clone()));
    p.system = Box::new(FakeSystem(world.clone()));
    (p, world)
}

/// A runtime on the fake platform with these controllers connected.
pub fn runtime_with(pads: &[(&str, PadFamily)]) -> (Runtime, Shared) {
    let (platform, world) = platform();
    world
        .lock()
        .unwrap()
        .pads
        .extend(pads.iter().map(|&(id, family)| pad(id, family)));
    (Runtime::with_platform(platform), world)
}

/// A mapping that runs `commands` once per press of `button`.
pub fn command_mapping(button: PadButton, commands: &[Command]) -> Mapping {
    let actions = commands
        .iter()
        .map(|command| Action::Command {
            command: command.clone(),
        })
        .collect();
    let mut m = Mapping::new(Control::Button(button), actions);
    m.mode = Mode::Once;
    m
}
