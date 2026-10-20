//! Virtual gamepads through the ViGEmBus driver.

use std::sync::{Arc, Mutex};

use dp_core::pad::{Axis, PadButton, PadState, Rumble, axis_to_i16, axis_to_u8};
use vigem_client::{
    Client, DS4Report, DualShock4Wired, TargetId, XButtons, XGamepad, Xbox360Wired,
};

use crate::{PlatformError, Result, VirtualPad, VirtualPadFactory, VirtualPadKind};

fn vigem_err(e: vigem_client::Error) -> PlatformError {
    match e {
        vigem_client::Error::BusNotFound => PlatformError::DriverMissing(
            "ViGEmBus",
            "install it from https://github.com/nefarius/ViGEmBus/releases".into(),
        ),
        other => PlatformError::Other(format!("ViGEmBus: {other}")),
    }
}

pub struct VigemFactory;

impl VigemFactory {
    pub fn new() -> Self {
        VigemFactory
    }
}

impl VirtualPadFactory for VigemFactory {
    fn available(&self) -> Result<()> {
        Client::connect().map(drop).map_err(vigem_err)
    }

    fn create(&mut self, kind: VirtualPadKind) -> Result<Box<dyn VirtualPad>> {
        let client = Client::connect().map_err(vigem_err)?;
        match kind {
            VirtualPadKind::Xbox360 => {
                let mut target = Xbox360Wired::new(client, TargetId::XBOX360_WIRED);
                target.plugin().map_err(vigem_err)?;
                target.wait_ready().map_err(vigem_err)?;
                let slot = target.get_user_index().ok();
                let rumble = Arc::new(Mutex::new(None));
                // Rumble notifications arrive on a dedicated thread, which exits when the target is unplugged.
                match target.request_notification() {
                    Ok(req) => {
                        let sink = rumble.clone();
                        req.spawn_thread(move |_, n| {
                            *sink.lock().unwrap() = Some(Rumble {
                                low: n.large_motor as f32 / 255.0,
                                high: n.small_motor as f32 / 255.0,
                            });
                        });
                    }
                    Err(e) => tracing::warn!("no rumble feedback from virtual Xbox 360 pad: {e}"),
                }
                tracing::info!(?slot, "virtual Xbox 360 controller plugged in");
                Ok(Box::new(X360Pad {
                    target,
                    slot,
                    rumble,
                }))
            }
            VirtualPadKind::DualShock4 => {
                let mut target = DualShock4Wired::new(client, TargetId::DUALSHOCK4_WIRED);
                target.plugin().map_err(vigem_err)?;
                target.wait_ready().map_err(vigem_err)?;
                tracing::info!("virtual DualShock 4 controller plugged in");
                Ok(Box::new(Ds4Pad { target }))
            }
        }
    }
}

struct X360Pad {
    target: Xbox360Wired<Client>,
    slot: Option<u32>,
    rumble: Arc<Mutex<Option<Rumble>>>,
}

const X360_BUTTONS: [(PadButton, u16); 15] = [
    (PadButton::DpadUp, XButtons::UP),
    (PadButton::DpadDown, XButtons::DOWN),
    (PadButton::DpadLeft, XButtons::LEFT),
    (PadButton::DpadRight, XButtons::RIGHT),
    (PadButton::Start, XButtons::START),
    (PadButton::Back, XButtons::BACK),
    (PadButton::LS, XButtons::LTHUMB),
    (PadButton::RS, XButtons::RTHUMB),
    (PadButton::LB, XButtons::LB),
    (PadButton::RB, XButtons::RB),
    (PadButton::Guide, XButtons::GUIDE),
    (PadButton::A, XButtons::A),
    (PadButton::B, XButtons::B),
    (PadButton::X, XButtons::X),
    (PadButton::Y, XButtons::Y),
];

pub fn to_xgamepad(s: &PadState) -> XGamepad {
    let mut raw = 0u16;
    for (b, bit) in X360_BUTTONS {
        if s.pressed(b) {
            raw |= bit;
        }
    }
    XGamepad {
        buttons: XButtons { raw },
        left_trigger: (s.axis(Axis::LeftTrigger).clamp(0.0, 1.0) * 255.0).round() as u8,
        right_trigger: (s.axis(Axis::RightTrigger).clamp(0.0, 1.0) * 255.0).round() as u8,
        thumb_lx: axis_to_i16(s.axis(Axis::LeftX)),
        thumb_ly: axis_to_i16(s.axis(Axis::LeftY)),
        thumb_rx: axis_to_i16(s.axis(Axis::RightX)),
        thumb_ry: axis_to_i16(s.axis(Axis::RightY)),
    }
}

impl VirtualPad for X360Pad {
    fn kind(&self) -> VirtualPadKind {
        VirtualPadKind::Xbox360
    }
    fn update(&mut self, state: &PadState) -> Result<()> {
        self.target.update(&to_xgamepad(state)).map_err(vigem_err)
    }
    fn take_rumble(&mut self) -> Option<Rumble> {
        self.rumble.lock().unwrap().take()
    }
    fn xinput_slot(&self) -> Option<u32> {
        self.slot
    }
}

impl Drop for X360Pad {
    fn drop(&mut self) {
        let _ = self.target.unplug();
    }
}

struct Ds4Pad {
    target: DualShock4Wired<Client>,
}

pub fn to_ds4(s: &PadState) -> DS4Report {
    let hat = match (
        s.pressed(PadButton::DpadUp),
        s.pressed(PadButton::DpadRight),
        s.pressed(PadButton::DpadDown),
        s.pressed(PadButton::DpadLeft),
    ) {
        (true, false, _, false) => 0,
        (true, true, _, _) => 1,
        (false, true, false, _) => 2,
        (_, true, true, _) => 3,
        (false, false, true, false) => 4,
        (_, false, true, true) => 5,
        (false, false, false, true) => 6,
        (true, false, _, true) => 7,
        _ => 8,
    };
    let mut buttons = hat as u16;
    let bits = [
        (PadButton::X, 1 << 4),  // square
        (PadButton::A, 1 << 5),  // cross
        (PadButton::B, 1 << 6),  // circle
        (PadButton::Y, 1 << 7),  // triangle
        (PadButton::LB, 1 << 8), // L1
        (PadButton::RB, 1 << 9), // R1
        (PadButton::Back, 1 << 12),
        (PadButton::Start, 1 << 13),
        (PadButton::LS, 1 << 14),
        (PadButton::RS, 1 << 15),
    ];
    for (b, bit) in bits {
        if s.pressed(b) {
            buttons |= bit;
        }
    }
    let lt = s.axis(Axis::LeftTrigger).clamp(0.0, 1.0);
    let rt = s.axis(Axis::RightTrigger).clamp(0.0, 1.0);
    if lt > 0.0 {
        buttons |= 1 << 10; // L2 digital
    }
    if rt > 0.0 {
        buttons |= 1 << 11; // R2 digital
    }
    let mut special = 0u8;
    if s.pressed(PadButton::Guide) {
        special |= 0x01;
    }
    if s.pressed(PadButton::Touchpad) {
        special |= 0x02;
    }
    DS4Report {
        thumb_lx: axis_to_u8(s.axis(Axis::LeftX), false),
        thumb_ly: axis_to_u8(s.axis(Axis::LeftY), true),
        thumb_rx: axis_to_u8(s.axis(Axis::RightX), false),
        thumb_ry: axis_to_u8(s.axis(Axis::RightY), true),
        buttons,
        special,
        trigger_l: (lt * 255.0).round() as u8,
        trigger_r: (rt * 255.0).round() as u8,
    }
}

impl VirtualPad for Ds4Pad {
    fn kind(&self) -> VirtualPadKind {
        VirtualPadKind::DualShock4
    }
    fn update(&mut self, state: &PadState) -> Result<()> {
        self.target.update(&to_ds4(state)).map_err(vigem_err)
    }
    fn take_rumble(&mut self) -> Option<Rumble> {
        None
    }
}

impl Drop for Ds4Pad {
    fn drop(&mut self) {
        let _ = self.target.unplug();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ds4_hat_encoding() {
        let mut s = PadState::default();
        assert_eq!(to_ds4(&s).buttons & 0xF, 8);
        s.set(PadButton::DpadUp, true);
        s.set(PadButton::DpadRight, true);
        assert_eq!(to_ds4(&s).buttons & 0xF, 1);
        s.set(PadButton::DpadUp, false);
        s.set(PadButton::DpadDown, true);
        assert_eq!(to_ds4(&s).buttons & 0xF, 3);
    }

    #[test]
    fn x360_neutral() {
        let g = to_xgamepad(&PadState::default());
        assert_eq!(g.buttons.raw, 0);
        assert_eq!((g.thumb_lx, g.left_trigger), (0, 0));
    }
}
