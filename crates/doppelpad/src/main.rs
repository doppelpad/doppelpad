//! `doppelpad` — free, open-source controller remapping.

mod daemon;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use dp_core::pad::{Axis, PadButton};
use dp_core::profile::Profile;
use dp_ipc::{Request, Response};

use dp_platform::timer::TimerResolution;
use dp_service::{Runtime, on_off};

#[derive(Parser)]
#[command(
    name = "doppelpad",
    version,
    about = "Doppelpad — free, open-source controller remapper"
)]
struct Cli {
    /// Log filter (e.g. `debug`, `dp_core=trace`). Defaults to `info`.
    #[arg(long, global = true, env = "DOPPELPAD_LOG")]
    log: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run one profile in the foreground until Ctrl+C, printing controller input and the
    /// actions it triggers.
    Run {
        profile: PathBuf,
        /// Do not print input and actions.
        #[arg(short, long)]
        quiet: bool,
    },
    /// Run in the background: IPC control plus automatic per-game profile switching.
    Daemon {
        /// Directory with profiles (TOML or importable JSON) used for auto-switching.
        #[arg(long)]
        profiles: Option<PathBuf>,
        /// Profile to apply at start (and when no game-specific profile matches).
        #[arg(long)]
        profile: Option<PathBuf>,
        /// Disable switching profiles by foreground application.
        #[arg(long)]
        no_auto_switch: bool,
    },
    /// Ask the daemon to apply a profile.
    Apply {
        profile: PathBuf,
        /// Put it into this slot (1–4) and make the slot active, like the app's "Apply to slot".
        #[arg(long, value_parser = parse_slot)]
        slot: Option<usize>,
        /// With --slot: that controller's own slots (an id from `doppelpad devices`).
        #[arg(long, requires = "slot")]
        device: Option<String>,
    },
    /// Make a slot (1–4) active and turn remapping on.
    SelectSlot {
        #[arg(value_parser = parse_slot)]
        slot: usize,
        /// A controller's own slots (an id from `doppelpad devices`).
        #[arg(long)]
        device: Option<String>,
    },
    /// Empty a slot (1–4), or all of them.
    ClearSlot {
        /// A slot number or `all`.
        #[arg(value_parser = parse_slot_or_all)]
        slot: SlotOrAll,
        /// A controller's own slots (an id from `doppelpad devices`).
        #[arg(long)]
        device: Option<String>,
    },
    /// Turn remapping on (the active slot) or off.
    Remap { state: Switch },
    /// Ask the daemon to stop remapping.
    Stop,
    /// Show the daemon's state.
    Status,
    /// List connected controllers (via the daemon if it runs, otherwise directly).
    Devices,
    /// Turn automatic profile switching on or off.
    AutoSwitch { state: Switch },
    /// Stop the daemon (the desktop app refuses: quit it from its tray icon).
    Shutdown,
    /// Convert a JSON profile (schema 3.0) into a Doppelpad TOML profile. Launch commands,
    /// which start programs, are listed and left out unless `--keep-launches`.
    Import {
        file: PathBuf,
        /// Output file (default: print to stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Keep the profile's launch commands (only for a profile you trust).
        #[arg(long)]
        keep_launches: bool,
    },
    /// Validate a profile and print diagnostics.
    Check { profile: PathBuf },
    /// Check drivers (ViGEmBus and HidHide on Windows, uinput on Linux) and list controllers.
    Doctor,
    /// Show live controller input (useful to verify devices and button names).
    Monitor,
}

/// `on` or `off`.
#[derive(Clone, Copy, ValueEnum)]
enum Switch {
    On,
    Off,
}

impl From<Switch> for bool {
    fn from(s: Switch) -> bool {
        matches!(s, Switch::On)
    }
}

/// A slot number, 1 to the number of slots.
fn parse_slot(s: &str) -> Result<usize, String> {
    s.parse()
        .ok()
        .filter(|n| (1..=dp_service::SLOTS).contains(n))
        .ok_or_else(|| format!("{s} is not in 1..={}", dp_service::SLOTS))
}

#[derive(Clone, Copy)]
enum SlotOrAll {
    Slot(usize),
    All,
}

fn parse_slot_or_all(s: &str) -> Result<SlotOrAll, String> {
    if s == "all" {
        return Ok(SlotOrAll::All);
    }
    parse_slot(s)
        .map(SlotOrAll::Slot)
        .map_err(|_| format!("`{s}` is not a slot (1–{} or `all`)", dp_service::SLOTS))
}

fn main() {
    let cli = Cli::parse();
    let filter = cli.log.clone().unwrap_or_else(|| "info".into());
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&filter).unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Run { profile, quiet } => run_foreground(&profile, !quiet),
        Cmd::Daemon {
            profiles,
            profile,
            no_auto_switch,
        } => daemon::run(
            profiles.unwrap_or_else(dp_config::profiles_dir),
            profile,
            !no_auto_switch,
        ),
        Cmd::Apply {
            profile,
            slot,
            device,
        } => {
            let path = profile_path(&profile)?;
            send(match slot {
                Some(slot) => Request::ApplyToSlot { path, slot, device },
                None => Request::Apply { path },
            })
        }
        Cmd::SelectSlot { slot, device } => send(Request::SelectSlot { slot, device }),
        Cmd::ClearSlot { slot, device } => send(Request::ClearSlot {
            slot: match slot {
                SlotOrAll::Slot(n) => Some(n),
                SlotOrAll::All => None,
            },
            device,
        }),
        Cmd::Remap { state } => send(Request::Remap { on: state.into() }),
        Cmd::Stop => send(Request::Stop),
        Cmd::Status => send(Request::Status),
        Cmd::AutoSwitch { state } => send(Request::AutoSwitch {
            enabled: state.into(),
        }),
        Cmd::Shutdown => send(Request::Shutdown),
        Cmd::Devices => match dp_ipc::call(&Request::Devices) {
            Ok(r) => print_response(r),
            // A running daemon holds the devices; only without one scan them here.
            Err(dp_ipc::IpcError::NotRunning(_)) => {
                let rt = scan_devices();
                print_response(Response::Devices {
                    devices: dp_service::ipc_devices(&rt),
                })
            }
            Err(e) => Err(e.into()),
        },
        Cmd::Import {
            file,
            output,
            keep_launches,
        } => import(&file, output.as_deref(), keep_launches),
        Cmd::Check { profile } => check(&profile),
        Cmd::Doctor => doctor(),
        Cmd::Monitor => monitor(),
    }
}

/// The absolute path of an existing profile, as the daemon stores it in a slot. Not
/// `canonicalize`: on Windows that gives a verbatim `\\?\C:\...` path, which never equals
/// the paths the GUI and the tray compare it with.
fn profile_path(profile: &Path) -> Result<String> {
    let context = || format!("{}", profile.display());
    std::fs::metadata(profile).with_context(context)?;
    Ok(std::path::absolute(profile)
        .with_context(context)?
        .display()
        .to_string())
}

/// Send a request to the daemon and print its answer.
fn send(req: Request) -> Result<()> {
    print_response(dp_ipc::call(&req)?)
}

fn print_response(r: Response) -> Result<()> {
    match r {
        Response::Ok { message } => println!("{message}"),
        Response::Error { message } => bail!("{message}"),
        Response::Status(s) => {
            println!("profile:      {}", s.profile.as_deref().unwrap_or("(none)"));
            if let Some(p) = s.profile_path {
                println!("file:         {p}");
            }
            println!("layer:        {}", s.layer);
            println!(
                "virtual pad:  {}",
                s.virtual_pad.as_deref().unwrap_or("(none)")
            );
            println!("hidden:       {} device(s)", s.hidden_devices);
            println!("remapping:    {}", on_off(s.remap_on));
            for (i, slot) in s.slots.iter().enumerate() {
                let mark = if i + 1 == s.active_slot { "*" } else { " " };
                println!(
                    "slot {}:{mark}     {}",
                    i + 1,
                    slot.as_deref().unwrap_or("(empty)")
                );
            }
            println!("auto-switch:  {}", on_off(s.auto_switch));
            println!("foreground:   {}", s.foreground.as_deref().unwrap_or("?"));
            for w in s.warnings {
                println!("warning:      {w}");
            }
        }
        Response::Devices { devices } => {
            if devices.is_empty() {
                println!("no controllers found");
            }
            for d in devices {
                println!(
                    "{:<40} {:<26} {:04x}:{:04x}  {}",
                    d.id, d.name, d.vendor_id, d.product_id, d.backend
                );
            }
        }
    }
    Ok(())
}

fn print_diagnostics(diags: &dp_config::Diagnostics) {
    for d in diags.iter() {
        eprintln!("{d}");
    }
}

/// What `import` says about the profile's launch commands, after leaving them out unless
/// `keep`: they start programs, so a profile from someone else could run anything.
fn launch_notice(profile: &mut Profile, keep: bool) -> Vec<String> {
    let found = dp_config::launches::find(profile);
    if found.is_empty() {
        return Vec::new();
    }
    let mut lines = vec!["warning: this profile starts programs:".to_owned()];
    lines.extend(
        found
            .iter()
            .map(|l| format!("  {}: {}", l.place, l.command_line())),
    );
    if keep {
        lines.push("kept them (--keep-launches)".into());
    } else {
        dp_config::launches::strip(profile);
        lines.push(
            "left them out; --keep-launches keeps them (only for a profile you trust)".into(),
        );
    }
    lines
}

fn import(file: &Path, output: Option<&Path>, keep_launches: bool) -> Result<()> {
    let (mut profile, diags) = dp_config::load(file)?;
    print_diagnostics(&diags);
    for line in launch_notice(&mut profile, keep_launches) {
        eprintln!("{line}");
    }
    let toml = dp_config::to_toml(&profile)?;
    match output {
        Some(out) => {
            std::fs::write(out, toml).with_context(|| format!("writing {}", out.display()))?;
            eprintln!(
                "wrote {} ({} mappings, {} layers)",
                out.display(),
                profile.mappings.len(),
                profile.layers.len()
            );
        }
        None => print!("{toml}"),
    }
    Ok(())
}

fn check(path: &Path) -> Result<()> {
    let (profile, diags) = dp_config::load(path)?;
    print_diagnostics(&diags);
    let engine = dp_core::engine::Engine::new(profile);
    for w in engine.warnings() {
        eprintln!("warning: {w}");
    }
    let p = engine.profile();
    println!(
        "{}: {} mappings, {} layers, {} analog routes, virtual pad {:?}",
        p.name,
        p.mappings.len(),
        p.layers.len(),
        p.analog.len(),
        p.output.virtual_pad
    );
    if diags.has_errors() {
        bail!("profile has errors");
    }
    Ok(())
}

/// Cleared when the user presses Ctrl+C.
pub(crate) fn stop_flag() -> Result<Arc<AtomicBool>> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || r.store(false, Ordering::Relaxed))?;
    Ok(running)
}

fn run_foreground(path: &Path, echo: bool) -> Result<()> {
    let (profile, diags) = dp_config::load(path)?;
    print_diagnostics(&diags);
    if diags.has_errors() {
        bail!("{} has errors", path.display());
    }
    let running = stop_flag()?;
    let _timer = TimerResolution::high();
    let mut rt = Runtime::new();
    rt.set_echo(echo);
    rt.apply(profile, Some(path.to_path_buf()))?;
    eprintln!("running {} — press Ctrl+C to stop", path.display());
    if echo {
        eprintln!(
            "controller input and the actions it triggers are listed below (--quiet hides them)"
        );
    }
    let mut last_layer = 0;
    while running.load(Ordering::Relaxed) {
        rt.tick();
        for e in rt.take_events() {
            eprintln!("{e}");
        }
        if rt.profile().is_none() {
            eprintln!("profile stopped by command");
            break;
        }
        if !echo && rt.layer() != last_layer {
            last_layer = rt.layer();
            eprintln!("layer {last_layer}");
        }
        std::thread::sleep(dp_service::TICK);
    }
    rt.stop();
    Ok(())
}

/// Poll backends long enough for the HID scanner to find devices.
fn scan_devices() -> Runtime {
    let mut rt = Runtime::new();
    scan(&mut rt);
    rt
}

fn scan(rt: &mut Runtime) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        rt.tick();
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn doctor() -> Result<()> {
    // One runtime for all of it: on Linux each one opens a keyboard and mouse through
    // uinput, and warns again when it cannot.
    let mut rt = Runtime::new();
    let drivers = rt.driver_probe()();
    let ok = |r: dp_platform::Result<()>| match r {
        Ok(()) => "ok".to_string(),
        Err(e) => format!("NOT AVAILABLE — {e}"),
    };
    println!(
        "Doppelpad:                      {} on {}",
        env!("CARGO_PKG_VERSION"),
        dp_platform::os_version()
    );
    // What each OS uses for the two.
    let (pads, hiding) = if cfg!(windows) {
        (
            "ViGEmBus (virtual controllers):",
            "HidHide (hide physical pads):",
        )
    } else if cfg!(target_os = "macos") {
        ("virtual controllers:", "IOKit seize (hide physical pads):")
    } else {
        (
            "uinput (virtual controllers):",
            "EVIOCGRAB (hide physical pads):",
        )
    };
    println!("{pads:<32}{}", ok(drivers.virtual_pads));
    println!("{hiding:<32}{}", ok(drivers.device_hiding));
    match dp_config::portable_dir() {
        Some(dir) => println!(
            "portable mode:                  everything in {}",
            dir.display()
        ),
        None => println!(
            "settings directory:             {}",
            dp_config::config_dir().display()
        ),
    }
    println!(
        "profiles directory:             {}",
        dp_config::profiles_dir().display()
    );
    #[cfg(windows)]
    if let Some(r) = dp_platform::windows::rewasd::detect().filter(|r| r.running()) {
        let mut parts: Vec<String> = r.processes.iter().map(|p| p.name.clone()).collect();
        if r.service_running {
            parts.insert(0, "service jlserv".into());
        }
        println!(
            "reWASD is running ({}) — it keeps controllers to itself; quit it first",
            parts.join(", ")
        );
    }
    println!();
    scan(&mut rt);
    for p in rt.hiding_problems().into_iter().chain(rt.device_problems()) {
        println!("problem: {p}");
    }
    print_response(Response::Devices {
        devices: dp_service::ipc_devices(&rt),
    })
}

fn monitor() -> Result<()> {
    let running = stop_flag()?;
    let mut rt = Runtime::new();
    eprintln!("showing controller input — press Ctrl+C to stop");
    let mut last = String::new();
    while running.load(Ordering::Relaxed) {
        rt.tick();
        let mut line = String::new();
        for s in rt.devices() {
            let st = &s.state;
            let buttons: Vec<&str> = PadButton::ALL
                .iter()
                .filter(|b| st.pressed(**b))
                .map(|b| b.name())
                .collect();
            line.push_str(&format!(
                "[{}] L({:+.2},{:+.2}) R({:+.2},{:+.2}) LT {:.2} RT {:.2} {}",
                s.info.name,
                st.axis(Axis::LeftX),
                st.axis(Axis::LeftY),
                st.axis(Axis::RightX),
                st.axis(Axis::RightY),
                st.axis(Axis::LeftTrigger),
                st.axis(Axis::RightTrigger),
                buttons.join(" ")
            ));
            if let Some(m) = st.motion {
                line.push_str(&format!(
                    " gyro({:+.0},{:+.0},{:+.0})",
                    m.gyro[0], m.gyro[1], m.gyro[2]
                ));
            }
            if let Some(t) = st.touch[0] {
                line.push_str(&format!(" touch({:.2},{:.2})", t.x, t.y));
            }
            line.push_str("   ");
        }
        if line.is_empty() {
            line = "no controller connected".into();
        }
        if line != last {
            eprint!("\r{line:<160}");
            let _ = std::io::stderr().flush();
            last = line;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    eprintln!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slot paths are compared with the ones the GUI writes, so they must not be verbatim
    /// (`\\?\C:\...`) paths.
    #[test]
    fn profile_paths_are_plain_absolute_paths() {
        let file = std::env::temp_dir().join("doppelpad-cli-path-test.toml");
        std::fs::write(&file, "").unwrap();
        let path = profile_path(&file).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert!(!path.starts_with(r"\\?\"), "{path}");
        assert!(Path::new(&path).is_absolute(), "{path}");
        assert!(profile_path(&file).is_err(), "a missing file is an error");
    }

    #[test]
    fn import_lists_launches_and_drops_them_unless_kept() {
        let shared = || {
            dp_config::parse_toml(
                r#"
[[mappings]]
trigger = "A"
actions = [{ command = { launch = { path = "C:\\Program Files\\x.exe", args = "-enc AB" } } }]

[[mappings]]
trigger = "B"
actions = [{ key = "W" }]
"#,
            )
            .unwrap()
        };
        let mut p = shared();
        assert_eq!(
            launch_notice(&mut p, false),
            [
                "warning: this profile starts programs:",
                r#"  main layer, A: "C:\Program Files\x.exe" -enc AB"#,
                "left them out; --keep-launches keeps them (only for a profile you trust)",
            ]
        );
        assert_eq!(p.mappings.len(), 1, "B stays");
        let mut p = shared();
        let kept = launch_notice(&mut p, true);
        assert_eq!(kept.last().unwrap(), "kept them (--keep-launches)");
        assert_eq!(p, shared());
        assert!(launch_notice(&mut Profile::default(), false).is_empty());
    }
}
