//! No-op backends: used on platforms without a native implementation yet and in tests.

use std::sync::Arc;

use dp_core::keys::{Key, MouseButton, WheelDir};
use dp_core::pad::{PadState, Rumble};

use crate::*;

/// Backends that do nothing. Virtual pads are accepted but go nowhere.
pub struct NullPlatform;

impl NullPlatform {
    pub fn platform() -> Platform {
        Platform {
            probe_drivers: || DriverReport {
                virtual_pads: Err(PlatformError::Unsupported("virtual gamepads")),
                device_hiding: Err(PlatformError::Unsupported("device hiding")),
                input_access: Err(PlatformError::Unsupported("keyboard and mouse")),
                bluetooth: Err(PlatformError::Unsupported("Bluetooth")),
                hiding_problems: Vec::new(),
                system_problems: Vec::new(),
            },
            gamepads: Vec::new(),
            virtual_pads: Box::new(NullPlatform),
            kbm_out: Box::new(NullPlatform),
            kbm_capture: Box::new(NullPlatform),
            hider: Box::new(NullPlatform),
            foreground: Box::new(NullPlatform),
            system: Box::new(NullPlatform),
        }
    }
}

struct NullPad(VirtualPadKind);

impl VirtualPad for NullPad {
    fn kind(&self) -> VirtualPadKind {
        self.0
    }
    fn update(&mut self, _state: &PadState) -> Result<()> {
        Ok(())
    }
    fn take_rumble(&mut self) -> Option<Rumble> {
        None
    }
}

impl VirtualPadFactory for NullPlatform {
    fn available(&self) -> Result<()> {
        Err(PlatformError::Unsupported("virtual gamepads"))
    }
    fn create(&mut self, kind: VirtualPadKind) -> Result<Box<dyn VirtualPad>> {
        Ok(Box::new(NullPad(kind)))
    }
}

impl KbmOutput for NullPlatform {
    fn key(&mut self, _key: Key, _down: bool) {}
    fn mouse_button(&mut self, _button: MouseButton, _down: bool) {}
    fn mouse_move(&mut self, _dx: f32, _dy: f32) {}
    fn wheel(&mut self, _dir: WheelDir, _detents: f32) {}
}

impl KbmCapture for NullPlatform {
    fn start(
        &mut self,
        _filter: Arc<CaptureFilter>,
    ) -> Result<crossbeam_channel::Receiver<KbmEvent>> {
        Err(PlatformError::Unsupported("keyboard/mouse capture"))
    }
    fn stop(&mut self) {}
}

impl DeviceHider for NullPlatform {
    fn available(&self) -> Result<()> {
        Err(PlatformError::Unsupported("device hiding"))
    }
    fn hide(&mut self, _instance_paths: &[String]) -> Result<()> {
        Err(PlatformError::Unsupported("device hiding"))
    }
    fn restore(&mut self) -> Result<()> {
        Ok(())
    }
}

impl ForegroundWatcher for NullPlatform {
    fn foreground_process(&mut self) -> Option<String> {
        None
    }
}

impl SystemControl for NullPlatform {
    fn power(&mut self, _action: PowerAction) -> Result<()> {
        Err(PlatformError::Unsupported("power commands"))
    }
    fn screenshot(&mut self) -> Result<std::path::PathBuf> {
        Err(PlatformError::Unsupported("screenshots"))
    }
    fn close_foreground_window(&mut self) -> Result<()> {
        Err(PlatformError::Unsupported("closing windows"))
    }
}
