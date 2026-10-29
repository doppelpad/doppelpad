//! Mapping modes: turbo, once, toggle, repeat, break steps, and stopping them.

use super::*;

#[test]
fn turbo_pulses_while_held() {
    let mut p = Profile::default();
    let mut m = map(
        Control::TriggerPress(Trigger::Right),
        vec![Action::Mouse {
            mouse: MouseButton::Left,
            action: Press::Auto,
        }],
    );
    m.mode = Mode::Turbo;
    m.turbo_ms = Some(100);
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.pad.set_axis(Axis::RightTrigger, 1.0);
    h.until(1); // press at 1
    h.until(260);
    h.pad.set_axis(Axis::RightTrigger, 0.0);
    h.until(300);
    let ev: Vec<(u64, bool)> = h.mouse.iter().map(|(t, _, d)| (*t, *d)).collect();
    assert_eq!(
        ev,
        vec![
            (1, true),
            (51, false),
            (101, true),
            (151, false),
            (201, true),
            (251, false)
        ]
    );
    assert_eq!(
        h.virtual_pad().axis(Axis::RightTrigger),
        0.0,
        "mapped trigger is not mirrored"
    );
}

#[test]
fn once_macro_runs_to_completion() {
    let mut p = Profile::default();
    let mut m = map(
        b(PadButton::Y),
        vec![
            Action::Key {
                key: Key::LeftCtrl,
                action: Press::Down,
            },
            key(Key::C),
            Action::Pause { pause: 100 },
            Action::Key {
                key: Key::LeftCtrl,
                action: Press::Up,
            },
        ],
    );
    m.mode = Mode::Once;
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::Y, true); // 1
    h.btn(PadButton::Y, false); // 2: release does not cancel
    h.until(300);
    // Ctrl down @1, key delay 10, C tap 10 ms, pause 100, key delay is not added next to pauses.
    assert_eq!(h.key_events(Key::LeftCtrl), vec![(1, true), (121, false)]);
    assert_eq!(h.key_events(Key::C), vec![(11, true), (21, false)]);
}

#[test]
fn toggle_mode_latches() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::X), vec![key(Key::LeftShift)]);
    m.mode = Mode::Toggle;
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.tap(PadButton::X);
    h.until(50);
    assert!(h.e.is_output_down(Key::LeftShift));
    h.tap(PadButton::X);
    assert!(!h.e.is_output_down(Key::LeftShift));
    assert_eq!(h.key_events(Key::LeftShift), vec![(1, true), (51, false)]);
}

#[test]
fn repeat_mode() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::A), vec![key(Key::E)]);
    m.mode = Mode::Repeat;
    m.repeat = Some(3);
    m.repeat_pause_ms = Some(40);
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.tap(PadButton::A);
    h.until(500);
    let downs: Vec<u64> = h
        .key_events(Key::E)
        .into_iter()
        .filter(|e| e.1)
        .map(|e| e.0)
        .collect();
    assert_eq!(downs, vec![1, 51, 101]);
}

#[test]
fn a_held_sequence_without_waits_repeats_once_per_tick() {
    let mut p = Profile::default();
    let mut m = map(
        b(PadButton::A),
        vec![Action::Wheel {
            wheel: WheelDir::Up,
            clicks: 1.0,
        }],
    );
    m.mode = Mode::WhileHeld;
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true);
    let mut notches = vec![h.last.wheel.len()];
    for _ in 0..9 {
        h.step();
        notches.push(h.last.wheel.len());
    }
    assert_eq!(notches, vec![1; 10]);
    h.btn(PadButton::A, false);
    h.step();
    assert!(h.last.wheel.is_empty(), "stops on release");
}

#[test]
fn a_toggled_sequence_without_waits_repeats_once_per_tick() {
    let mut p = Profile::default();
    let mut m = map(
        b(PadButton::X),
        vec![
            Action::Key {
                key: Key::A,
                action: Press::Down,
            },
            Action::Rumble {
                rumble: 100,
                low: 1.0,
                high: 1.0,
            },
        ],
    );
    m.mode = Mode::Toggle;
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.tap(PadButton::X);
    h.until(20);
    // Each run presses the key and releases it at its end: one run per tick.
    let presses: Vec<u64> = h
        .key_events(Key::A)
        .into_iter()
        .filter(|e| e.1)
        .map(|e| e.0)
        .collect();
    let mut once = presses.clone();
    once.dedup();
    assert_eq!(presses, once, "at most one run per tick");
    assert!(presses.len() >= 15, "{presses:?}");
    h.tap(PadButton::X);
    h.until(h.t + 5);
    assert!(!h.e.is_output_down(Key::A), "toggled off");
    let n = h.key_events(Key::A).len();
    h.until(h.t + 5);
    assert_eq!(h.key_events(Key::A).len(), n, "and stays off");
}

#[test]
fn a_looping_sequence_does_not_replay_missed_runs_after_a_stall() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::A), vec![key(Key::E)]);
    m.mode = Mode::WhileHeld;
    m.repeat_pause_ms = Some(30);
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true);
    h.until(100);
    let before = h.key_events(Key::E).len();
    // A one-second stall: the loop carries on from now instead of catching up.
    h.t += 1000;
    h.tick();
    // At most the run in progress, one on time and one now (instead of ~25 runs).
    let burst = h.key_events(Key::E).len() - before;
    assert!(burst <= 6, "{burst} key events in one tick");
}

#[test]
fn releasing_between_two_runs_starts_no_further_run() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::A), vec![key(Key::E)]);
    m.mode = Mode::WhileHeld;
    m.repeat_pause_ms = Some(100);
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true); // 1: run, tap ends at 11
    h.until(50); // in the pause
    h.btn(PadButton::A, false);
    h.until(300);
    assert_eq!(h.key_events(Key::E), vec![(1, true), (11, false)]);
}

#[test]
fn random_ms_spreads_turbo_cycles() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::A), vec![key(Key::F)]);
    m.mode = Mode::Turbo;
    m.turbo_ms = Some(50);
    m.random_ms = Some(20);
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true);
    h.until(1000);
    let downs: Vec<u64> = h
        .key_events(Key::F)
        .into_iter()
        .filter(|(_, d)| *d)
        .map(|(t, _)| t)
        .collect();
    let gaps: Vec<u64> = downs.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps.len() > 10, "{gaps:?}");
    assert!(gaps.iter().all(|g| (50..=70).contains(g)), "{gaps:?}");
    assert!(
        gaps.iter().any(|g| *g != gaps[0]),
        "not all the same: {gaps:?}"
    );
}

#[test]
fn break_steps_cycle_through_segments() {
    let mut p = Profile::default();
    let mut m = map(
        b(PadButton::A),
        vec![key(Key::Q), Action::BREAK, key(Key::E), key(Key::R)],
    );
    m.mode = Mode::Turbo; // not possible with segments: held instead
    p.mappings.push(m);
    let mut h = Harness::new(p);
    let press = |h: &mut Harness| {
        h.btn(PadButton::A, true);
        h.until(h.t + 30);
        let down: Vec<Key> = [Key::Q, Key::E, Key::R]
            .into_iter()
            .filter(|k| h.e.is_output_down(*k))
            .collect();
        h.btn(PadButton::A, false);
        h.until(h.t + 30);
        down
    };
    assert_eq!(press(&mut h), vec![Key::Q]);
    assert!(!h.e.is_output_down(Key::Q), "held until release");
    assert_eq!(press(&mut h), vec![Key::E, Key::R]);
    assert_eq!(press(&mut h), vec![Key::Q], "starts over");
}

#[test]
fn stop_all_releases_latched_outputs() {
    let mut p = Profile::default();
    let mut m = map(b(PadButton::X), vec![key(Key::LeftShift)]);
    m.mode = Mode::Toggle;
    p.mappings.push(m);
    let mut h = Harness::new(p);
    h.tap(PadButton::X);
    assert!(h.e.is_output_down(Key::LeftShift));
    h.e.stop_all();
    h.step();
    assert!(!h.e.is_output_down(Key::LeftShift));
    assert_eq!(h.key_events(Key::LeftShift).last(), Some(&(h.t, false)));
}

#[test]
fn release_all_clears_outputs() {
    let mut p = Profile::default();
    p.mappings.push(map(b(PadButton::A), vec![key(Key::Space)]));
    let mut h = Harness::new(p);
    h.btn(PadButton::A, true);
    let out = h.e.release_all();
    assert_eq!(out.keys, vec![(Key::Space, false)]);
}
