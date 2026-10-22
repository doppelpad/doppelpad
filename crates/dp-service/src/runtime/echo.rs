//! The readable log of input and output for `doppelpad run --echo`.

use dp_core::engine::Output;
use dp_core::pad::{PadButton, PadState, Trigger};

/// Human-readable log of inputs and the actions they caused (for `doppelpad run`).
#[derive(Default)]
pub(super) struct Echo {
    pub(super) events: Vec<String>,
    pad_buttons: u32,
    triggers: [bool; 2],
    virtual_buttons: u32,
    mouse: (f32, f32),
    mouse_since: u64,
}

/// How often accumulated mouse motion is reported.
pub(super) const ECHO_MOUSE_MS: u64 = 300;

/// Trigger pull reported as "pressed" in the echo.
const ECHO_TRIGGER: f32 = 0.1;

impl Echo {
    pub(super) fn record(&mut self, pad: Option<&PadState>, out: &Output, now: u64) {
        if let Some(p) = pad {
            let changed = p.buttons ^ self.pad_buttons;
            for b in PadButton::ALL {
                if changed & b.bit() != 0 {
                    let what = if p.pressed(b) { "pressed" } else { "released" };
                    self.events
                        .push(format!("controller  {:<9} {what}", b.name()));
                }
            }
            self.pad_buttons = p.buttons;
            for (i, (t, name)) in [(Trigger::Left, "LT"), (Trigger::Right, "RT")]
                .into_iter()
                .enumerate()
            {
                let down = p.axis(t.axis()) >= ECHO_TRIGGER;
                if down != self.triggers[i] {
                    let what = if down { "pressed" } else { "released" };
                    self.events.push(format!("controller  {name:<9} {what}"));
                    self.triggers[i] = down;
                }
            }
        }
        for (k, down) in &out.keys {
            self.events.push(format!(
                "  -> key           {k} {}",
                if *down { "down" } else { "up" }
            ));
        }
        for (b, down) in &out.mouse_buttons {
            let what = if *down { "down" } else { "up" };
            self.events
                .push(format!("  -> mouse button  {} {what}", b.name()));
        }
        for (w, n) in &out.wheel {
            self.events.push(format!("  -> mouse wheel   {w:?} x{n}"));
        }
        if let Some(vp) = &out.pad {
            let changed = vp.buttons ^ self.virtual_buttons;
            for b in PadButton::ALL {
                if changed & b.bit() != 0 {
                    let what = if vp.pressed(b) { "down" } else { "up" };
                    self.events
                        .push(format!("  -> virtual pad   {} {what}", b.name()));
                }
            }
            self.virtual_buttons = vp.buttons;
        }
        for c in &out.commands {
            self.events.push(format!("  -> command       {c:?}"));
        }
        if out.layer_changed {
            self.events.push(format!("layer {}", out.layer));
        }
        self.mouse.0 += out.mouse_move.0;
        self.mouse.1 += out.mouse_move.1;
        if now.saturating_sub(self.mouse_since) >= ECHO_MOUSE_MS {
            if self.mouse.0.abs() >= 1.0 || self.mouse.1.abs() >= 1.0 {
                self.events.push(format!(
                    "  -> mouse moved   {:+.0}, {:+.0} px",
                    self.mouse.0, self.mouse.1
                ));
            }
            self.mouse = (0.0, 0.0);
            self.mouse_since = now;
        }
    }
}
