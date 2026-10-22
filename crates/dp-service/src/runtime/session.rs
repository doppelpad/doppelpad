//! One running profile: its engine, its virtual pad and the feedback to its controller.

use std::path::PathBuf;

use dp_core::engine::{Engine, Output, RadialView};
use dp_core::led;
use dp_core::pad::{Rgb, Rumble};
use dp_core::profile::{Command, TriggerEffect, Vibration};
use dp_core::repeat::KeyRepeater;
use dp_platform::groups::Grouping;
use dp_platform::{DeviceId, GamepadBackend, Platform, TriggerEffects, VirtualPad};

use super::SessionInfo;
use crate::timing::Every;

/// One profile running against one controller.
pub(super) struct Session {
    /// `None` for the default session.
    pub(super) owner: Option<DeviceId>,
    pub(super) engine: Engine,
    pub(super) path: Option<PathBuf>,
    pub(super) vpad: Option<Box<dyn VirtualPad>>,
    /// The controller the session reads this tick.
    pub(super) device: Option<DeviceId>,
    pub(super) warnings: Vec<String>,
    pub(super) hide_wanted: bool,
    pub(super) needs_capture: bool,
    pub(super) game_rumble: Rumble,
    pub(super) sent_rumble: Option<(DeviceId, Rumble)>,
    /// At most one rumble command per the profile's gap.
    pub(super) rumble_gap: Every,
    /// Light bar colour the game set on a virtual DualShock 4.
    pub(super) game_lightbar: Option<Rgb>,
    /// What the controller was last told to show, and which controller.
    pub(super) sent_lightbar: Option<(DeviceId, Option<Rgb>)>,
    /// Trigger effects last sent, and to which controller.
    pub(super) sent_triggers: Option<(DeviceId, TriggerEffects)>,
    pub(super) key_repeat: KeyRepeater,
    /// The cursor clip this session last set.
    pub(super) clip: Option<[f32; 4]>,
    pub(super) crosshair: CrosshairChoice,
    /// The radial menu the engine shows.
    pub(super) radial: Option<RadialView>,
}

/// The crosshair a session's commands chose (an index), and whether they hid it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CrosshairChoice {
    chosen: Option<usize>,
    hidden: bool,
}

impl Session {
    /// Index of the crosshair to draw: the one a command chose, else the first bound to the
    /// active layer.
    pub(super) fn crosshair_index(&self) -> Option<usize> {
        if self.crosshair.hidden {
            return None;
        }
        let all = &self.engine.profile().crosshairs;
        match self.crosshair.chosen {
            Some(i) => (i < all.len()).then_some(i),
            None => all
                .iter()
                .position(|c| c.layers.contains(&self.engine.layer())),
        }
    }

    /// Carry out a crosshair command.
    pub(super) fn crosshair_command(&mut self, c: &Command) {
        let count = self.engine.profile().crosshairs.len();
        let showing = self.crosshair_index();
        match *c {
            Command::Crosshair(n) => {
                let i = (n as usize).saturating_sub(1);
                if i >= count {
                    tracing::warn!("there is no crosshair {n}");
                } else if showing == Some(i) {
                    self.crosshair.hidden = true;
                } else {
                    self.crosshair = CrosshairChoice {
                        chosen: Some(i),
                        hidden: false,
                    };
                }
            }
            Command::ToggleCrosshair if showing.is_some() => self.crosshair.hidden = true,
            Command::ToggleCrosshair => {
                self.crosshair.hidden = false;
                if self.crosshair_index().is_none() && count > 0 {
                    self.crosshair.chosen = Some(0);
                }
            }
            _ => {}
        }
    }

    pub(super) fn info(&self) -> SessionInfo {
        SessionInfo {
            owner: self.owner.as_ref().map(|d| d.0.clone()),
            device: self.device.as_ref().map(|d| d.0.clone()),
            profile: self.engine.profile().name.clone(),
            profile_path: self.path.as_ref().map(|p| p.display().to_string()),
            layer: self.engine.layer(),
            virtual_pad: self.vpad.as_ref().map(|v| v.kind()),
        }
    }
}

/// Send a session's keyboard/mouse and virtual pad output.
pub(super) fn emit(s: &mut Session, platform: &mut Platform, out: &Output, now: u64) {
    s.radial = out.radial;
    if let (Some(vpad), Some(pad)) = (&mut s.vpad, &out.pad)
        && let Err(e) = vpad.update(pad)
    {
        tracing::warn!("virtual pad update failed: {e}");
    }
    let kbm = &mut platform.kbm_out;
    for &(k, down) in &out.keys {
        kbm.key(k, down);
    }
    s.key_repeat.observe(&out.keys, now);
    if let Some(k) = s.key_repeat.due(now) {
        kbm.key(k, true);
    }
    for &(b, down) in &out.mouse_buttons {
        kbm.mouse_button(b, down);
    }
    if out.mouse_clip != s.clip {
        kbm.clip(out.mouse_clip);
        s.clip = out.mouse_clip;
    }
    if let Some((x, y)) = out.mouse_to {
        kbm.mouse_to(x, y);
    }
    if out.mouse_move != (0.0, 0.0) {
        kbm.mouse_move(out.mouse_move.0, out.mouse_move.1);
    }
    for &(w, n) in &out.wheel {
        kbm.wheel(w, n);
    }
}

/// The light bar the profile asks for now: the active layer's, else the base layer's.
fn profile_light(s: &Session, now: u64) -> Option<Rgb> {
    let p = s.engine.profile();
    let layer = s.engine.layer();
    let light = p
        .layer(layer)
        .and_then(|l| l.lightbar.as_ref())
        .or(p.lightbar.as_ref())?;
    Some(led::color_at(light, now - now % led::STEP_MS))
}

/// The shortest and longest gap between two rumble commands a profile may ask for.
const RUMBLE_GAP_MIN_MS: u32 = 10;

const RUMBLE_GAP_MAX_MS: u32 = 200;

/// The gap between two rumble commands the profile asks for, within the limits.
pub(super) fn rumble_gap_ms(vibration: &Vibration) -> u64 {
    u64::from(
        vibration
            .min_delay_ms
            .clamp(RUMBLE_GAP_MIN_MS, RUMBLE_GAP_MAX_MS),
    )
}

/// Rumble for the session's controller: mapping rumble overrides what the game asks the
/// virtual pad for. Also sets the light bar: the profile's, else the colour the game set.
pub(super) fn feedback(
    s: &mut Session,
    pads: &mut Grouping,
    mapping_rumble: Option<Rumble>,
    now: u64,
) {
    if let Some(r) = s.vpad.as_mut().and_then(|v| v.take_rumble()) {
        s.game_rumble = r;
    }
    if let Some(c) = s.vpad.as_mut().and_then(|v| v.take_lightbar()) {
        s.game_lightbar = Some(c);
    }
    let Some(id) = s.device.as_ref() else { return };
    let game = s.engine.profile().vibration.apply(s.game_rumble);
    let rumble = mapping_rumble.unwrap_or(game);
    // At most one command per gap; a change held back goes out with a later tick.
    let sent = |sent: &Option<(DeviceId, Rumble)>| {
        sent.as_ref().is_some_and(|(d, r)| d == id && *r == rumble)
    };
    if !sent(&s.sent_rumble) && s.rumble_gap.due(now) {
        pads.set_rumble(id, rumble);
        s.sent_rumble = Some((id.clone(), rumble));
    }
    // A trigger whose effect ends is turned off rather than left playing.
    let want = s.engine.trigger_effects();
    let before = s
        .sent_triggers
        .as_ref()
        .filter(|(d, _)| d == id)
        .map_or([None; 2], |(_, t)| *t);
    let effects = [0, 1].map(|i| want[i].or(before[i].map(|_| TriggerEffect::Off)));
    if !s
        .sent_triggers
        .as_ref()
        .is_some_and(|(d, t)| d == id && *t == effects)
    {
        pads.set_trigger_effects(id, effects);
        s.sent_triggers = Some((id.clone(), effects));
    }
    let light = profile_light(s, now).or(s.game_lightbar);
    if s.sent_lightbar
        .as_ref()
        .is_some_and(|(d, l)| d == id && *l == light)
    {
        return;
    }
    if let Some((old, Some(_))) = &s.sent_lightbar
        && old != id
    {
        pads.set_lightbar(old, None);
    }
    pads.set_lightbar(id, light);
    s.sent_lightbar = Some((id.clone(), light));
}

/// Stop a session's feedback on its controller.
pub(super) fn quiet(s: &mut Session, pads: &mut Grouping) {
    if let Some((id, r)) = s.sent_rumble.take()
        && r != Rumble::default()
    {
        pads.set_rumble(&id, Rumble::default());
    }
    if let Some((id, Some(_))) = s.sent_lightbar.take() {
        pads.set_lightbar(&id, None);
    }
    if let Some((id, t)) = s.sent_triggers.take()
        && t.iter().any(Option::is_some)
    {
        pads.set_trigger_effects(&id, [Some(TriggerEffect::Off); 2]);
    }
}
