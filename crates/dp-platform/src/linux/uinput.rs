//! uinput output: a virtual Xbox 360 pad (laid out like xpad's, with `FF_RUMBLE` so games'
//! rumble comes back to us) and a virtual keyboard + mouse.

use std::io;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dp_core::keys::{Key, MouseButton, WheelDir};
use dp_core::pad::{PadState, Rumble};
use evdev::uinput::VirtualDevice;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AttributeSet, BusType, EventSummary, FFEffectCode, FFEffectKind,
    InputEvent, InputId, KeyCode, RelativeAxisCode, UInputCode, UinputAbsSetup,
};

use super::codes::*;
use super::dev::{check_uinput, own_phys, uinput_error, wait_readable};
use super::kbmmap::{WheelOut, mouse_button_code};
use super::padmap::{
    FfMixer, X360_AXES, X360_EVENTS, X360_KEYS, X360_NAME, X360_PID, X360_VERSION, X360_VID,
    x360_events,
};
use crate::{KbmOutput, PlatformError, Result, VirtualPad, VirtualPadFactory, VirtualPadKind};

const FF_EFFECTS_MAX: u32 = 16;
const KBM_RETRY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct UinputFactory {
    created: u32,
}

impl VirtualPadFactory for UinputFactory {
    fn available(&self) -> Result<()> {
        check_uinput()
    }

    fn create(&mut self, kind: VirtualPadKind) -> Result<Box<dyn VirtualPad>> {
        match kind {
            VirtualPadKind::Xbox360 => {
                let pad = X360Pad::new(self.created).map_err(uinput_error)?;
                self.created += 1;
                tracing::info!("virtual Xbox 360 controller created (uinput)");
                Ok(Box::new(pad))
            }
            VirtualPadKind::DualShock4 => Err(PlatformError::Unsupported(
                "virtual DualShock 4 on Linux (use the Xbox 360 virtual pad)",
            )),
        }
    }
}

fn build_x360(index: u32) -> io::Result<VirtualDevice> {
    let keys: AttributeSet<KeyCode> = X360_KEYS.iter().map(|&(_, code)| KeyCode(code)).collect();
    let ff: AttributeSet<FFEffectCode> = [FFEffectCode::FF_RUMBLE, FFEffectCode::FF_GAIN]
        .into_iter()
        .collect();
    let phys = own_phys(&format!("x360-{index}"));
    let mut builder = VirtualDevice::builder()?
        .name(X360_NAME)
        .input_id(InputId::new(
            BusType::BUS_USB,
            X360_VID,
            X360_PID,
            X360_VERSION,
        ))
        .with_phys(&phys)?
        .with_keys(&keys)?;
    for &(code, min, max, fuzz, flat) in &X360_AXES {
        let info = AbsInfo::new(0, min, max, fuzz, flat, 0);
        builder = builder.with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisCode(code), info))?;
    }
    builder
        .with_ff(&ff)?
        .with_ff_effects_max(FF_EFFECTS_MAX)
        .build()
}

struct X360Pad {
    dev: Arc<Mutex<VirtualDevice>>,
    ff: Arc<Mutex<FfMixer>>,
    epoch: Instant,
    last: Option<[(u16, u16, i32); X360_EVENTS]>,
    last_rumble: Rumble,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl X360Pad {
    fn new(index: u32) -> io::Result<Self> {
        let dev = Arc::new(Mutex::new(build_x360(index)?));
        let ff = Arc::new(Mutex::new(FfMixer::default()));
        let epoch = Instant::now();
        let stop = Arc::new(AtomicBool::new(false));
        let (dev2, ff2, stop2) = (dev.clone(), ff.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("dp-uinput-ff".into())
            .spawn(move || ff_loop(&dev2, &ff2, epoch, &stop2))?;
        Ok(X360Pad {
            dev,
            ff,
            epoch,
            last: None,
            last_rumble: Rumble::default(),
            stop,
            thread: Some(thread),
        })
    }
}

fn now_ms(epoch: Instant) -> u64 {
    epoch.elapsed().as_millis() as u64
}

/// Answers the game's force-feedback uploads (games block in `EVIOCSFF` until we do) and
/// tracks which effects play.
fn ff_loop(dev: &Mutex<VirtualDevice>, ff: &Mutex<FfMixer>, epoch: Instant, stop: &AtomicBool) {
    let fd = dev.lock().unwrap().as_raw_fd();
    while !stop.load(Ordering::Relaxed) {
        match wait_readable(fd, 100) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(_) => break,
        }
        let mut dev = dev.lock().unwrap();
        let events: Vec<InputEvent> = match dev.fetch_events() {
            Ok(events) => events.collect(),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
            Err(e) => {
                tracing::warn!("virtual pad force feedback stopped: {e}");
                break;
            }
        };
        for ev in events {
            match ev.destructure() {
                EventSummary::UInput(ev, code, _) if code == UInputCode::UI_FF_UPLOAD => {
                    let Ok(mut upload) = dev.process_ff_upload(ev) else {
                        continue;
                    };
                    let effect = upload.effect();
                    match effect.kind {
                        FFEffectKind::Rumble {
                            strong_magnitude,
                            weak_magnitude,
                        } => ff.lock().unwrap().upload(
                            upload.effect_id(),
                            strong_magnitude,
                            weak_magnitude,
                            effect.replay.length,
                            effect.replay.delay,
                        ),
                        _ => upload.set_retval(-libc::EINVAL),
                    }
                }
                EventSummary::UInput(ev, code, _) if code == UInputCode::UI_FF_ERASE => {
                    if let Ok(erase) = dev.process_ff_erase(ev) {
                        ff.lock().unwrap().erase(erase.effect_id() as i16);
                    }
                }
                EventSummary::ForceFeedback(_, code, value) if code == FFEffectCode::FF_GAIN => {
                    ff.lock().unwrap().set_gain(value.clamp(0, 0xFFFF) as u16);
                }
                EventSummary::ForceFeedback(_, code, value) => {
                    ff.lock().unwrap().play(code.0 as i16, value, now_ms(epoch));
                }
                _ => {}
            }
        }
    }
}

impl VirtualPad for X360Pad {
    fn kind(&self) -> VirtualPadKind {
        VirtualPadKind::Xbox360
    }

    fn update(&mut self, state: &PadState) -> Result<()> {
        let now = x360_events(state);
        let changed: Vec<InputEvent> = now
            .iter()
            .enumerate()
            .filter(|&(i, e)| self.last.is_none_or(|last| last[i] != *e))
            .map(|(_, &(ty, code, value))| InputEvent::new(ty, code, value))
            .collect();
        if changed.is_empty() {
            return Ok(());
        }
        self.dev
            .lock()
            .unwrap()
            .emit(&changed)
            .map_err(|source| PlatformError::Os {
                context: "uinput write",
                source,
            })?;
        self.last = Some(now);
        Ok(())
    }

    fn take_rumble(&mut self) -> Option<Rumble> {
        let r = self.ff.lock().unwrap().rumble(now_ms(self.epoch));
        (r != self.last_rumble).then(|| {
            self.last_rumble = r;
            r
        })
    }
}

impl Drop for X360Pad {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        tracing::info!("virtual Xbox 360 controller removed");
    }
}

fn build_kbm() -> io::Result<VirtualDevice> {
    let mut keys: AttributeSet<KeyCode> = Key::ALL.iter().map(|k| KeyCode(k.evdev())).collect();
    for b in MouseButton::ALL {
        keys.insert(KeyCode(mouse_button_code(b)));
    }
    let rel: AttributeSet<RelativeAxisCode> = [
        REL_X,
        REL_Y,
        REL_WHEEL,
        REL_HWHEEL,
        REL_WHEEL_HI_RES,
        REL_HWHEEL_HI_RES,
    ]
    .into_iter()
    .map(RelativeAxisCode)
    .collect();
    VirtualDevice::builder()?
        .name("Doppelpad keyboard and mouse")
        .input_id(InputId::new(BusType::BUS_VIRTUAL, 0, 0, 1))
        .with_phys(&own_phys("kbm"))?
        .with_keys(&keys)?
        .with_relative_axes(&rel)?
        .build()
}

/// Keyboard and relative mouse through one uinput device. Created up front, because the
/// desktop needs a moment to pick up a new device and the first events would be lost
/// otherwise; if `/dev/uinput` is unavailable, creation is retried every few seconds.
pub struct UinputKbm {
    dev: Option<VirtualDevice>,
    retry_at: Option<Instant>,
    warned: bool,
    move_rem: (f32, f32),
    wheel: WheelOut,
}

impl UinputKbm {
    pub fn new() -> Self {
        let mut kbm = UinputKbm {
            dev: None,
            retry_at: None,
            warned: false,
            move_rem: (0.0, 0.0),
            wheel: WheelOut::default(),
        };
        kbm.device();
        kbm
    }

    fn device(&mut self) -> Option<&mut VirtualDevice> {
        if self.dev.is_none() && self.retry_at.is_none_or(|t| Instant::now() >= t) {
            match build_kbm() {
                Ok(dev) => self.dev = Some(dev),
                Err(e) => {
                    let e = uinput_error(e);
                    if self.warned {
                        tracing::debug!("keyboard/mouse output unavailable: {e}");
                    } else {
                        tracing::warn!("keyboard/mouse output unavailable: {e}");
                        self.warned = true;
                    }
                    self.retry_at = Some(Instant::now() + KBM_RETRY);
                }
            }
        }
        self.dev.as_mut()
    }

    fn emit(&mut self, events: &[(u16, u16, i32)]) {
        let events: Vec<InputEvent> = events
            .iter()
            .map(|&(ty, code, value)| InputEvent::new(ty, code, value))
            .collect();
        if let Some(dev) = self.device()
            && let Err(e) = dev.emit(&events)
        {
            tracing::trace!("uinput write failed: {e}");
        }
    }
}

impl KbmOutput for UinputKbm {
    fn key(&mut self, key: Key, down: bool) {
        self.emit(&[(EV_KEY, key.evdev(), i32::from(down))]);
    }

    fn mouse_button(&mut self, button: MouseButton, down: bool) {
        self.emit(&[(EV_KEY, mouse_button_code(button), i32::from(down))]);
    }

    fn mouse_move(&mut self, dx: f32, dy: f32) {
        self.move_rem.0 += dx;
        self.move_rem.1 += dy;
        let (ix, iy) = (self.move_rem.0.trunc(), self.move_rem.1.trunc());
        if ix == 0.0 && iy == 0.0 {
            return;
        }
        self.move_rem.0 -= ix;
        self.move_rem.1 -= iy;
        let mut events = Vec::with_capacity(2);
        if ix != 0.0 {
            events.push((EV_REL, REL_X, ix as i32));
        }
        if iy != 0.0 {
            events.push((EV_REL, REL_Y, iy as i32));
        }
        self.emit(&events);
    }

    fn wheel(&mut self, dir: WheelDir, detents: f32) {
        let events: Vec<(u16, u16, i32)> = self
            .wheel
            .wheel(dir, detents)
            .into_iter()
            .map(|(code, value)| (EV_REL, code, value))
            .collect();
        if !events.is_empty() {
            self.emit(&events);
        }
    }
}
