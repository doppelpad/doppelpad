//! `doppelpad` — free, open-source controller remapping.

mod daemon;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use dp_config::Severity;
use dp_core::control::pad_button_name;
use dp_core::pad::{Axis, PadButton};
use dp_ipc::{Request, Response};
use dp_platform::Platform;

use dp_service::{Runtime, TimerResolution};

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
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=4))]
        slot: Option<u8>,
    },
    /// Make a slot (1–4) active and turn remapping on.
    SelectSlot {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=4))]
        slot: u8,
    },
    /// Empty a slot (1–4), or all of them.
    ClearSlot {
        /// A slot number or `all`.
        slot: String,
    },
    /// Turn remapping on (the active slot) or off.
    Remap {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Ask the daemon to stop remapping.
    Stop,
    /// Show the daemon's state.
    Status,
    /// List connected controllers (via the daemon if it runs, otherwise directly).
    Devices,
    /// Turn automatic profile switching on or off.
    AutoSwitch {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Stop the daemon.
    Shutdown,
    /// Convert a JSON profile (schema 3.0) into a Doppelpad TOML profile.
    Import {
        file: PathBuf,
        /// Output file (default: print to stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Validate a profile and print diagnostics.
    Check { profile: PathBuf },
    /// Check drivers (ViGEmBus, HidHide) and list controllers.
    Doctor,
    /// Show live controller input (useful to verify devices and button names).
    Monitor,
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
        Cmd::Apply { profile, slot } => {
            let path = std::fs::canonicalize(&profile)
                .with_context(|| format!("{}", profile.display()))?
                .display()
                .to_string();
            let req = match slot {
                Some(slot) => Request::ApplyToSlot {
                    path,
                    slot: slot as usize,
                },
                None => Request::Apply { path },
            };
            print_response(dp_ipc::call(&req)?)
        }
        Cmd::SelectSlot { slot } => print_response(dp_ipc::call(&Request::SelectSlot {
            slot: slot as usize,
        })?),
        Cmd::ClearSlot { slot } => {
            let slot = match slot.as_str() {
                "all" => None,
                n => Some(
                    n.parse::<usize>()
                        .ok()
                        .filter(|n| (1..=4).contains(n))
                        .with_context(|| format!("`{n}` is not a slot (1–4 or `all`)"))?,
                ),
            };
            print_response(dp_ipc::call(&Request::ClearSlot { slot })?)
        }
        Cmd::Remap { state } => {
            print_response(dp_ipc::call(&Request::Remap { on: state == "on" })?)
        }
        Cmd::Stop => print_response(dp_ipc::call(&Request::Stop)?),
        Cmd::Status => print_response(dp_ipc::call(&Request::Status)?),
        Cmd::AutoSwitch { state } => print_response(dp_ipc::call(&Request::AutoSwitch {
            enabled: state == "on",
        })?),
        Cmd::Shutdown => print_response(dp_ipc::call(&Request::Shutdown)?),
        Cmd::Devices => match dp_ipc::call(&Request::Devices) {
            Ok(r) => print_response(r),
            Err(_) => {
                let rt = scan_devices();
                print_response(Response::Devices {
                    devices: dp_service::ipc_devices(&rt),
                })
            }
        },
        Cmd::Import { file, output } => import(&file, output.as_deref()),
        Cmd::Check { profile } => check(&profile),
        Cmd::Doctor => doctor(),
        Cmd::Monitor => monitor(),
    }
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
            println!("remapping:    {}", if s.remap_on { "on" } else { "off" });
            for (i, slot) in s.slots.iter().enumerate() {
                let mark = if i + 1 == s.active_slot { "*" } else { " " };
                println!(
                    "slot {}:{mark}     {}",
                    i + 1,
                    slot.as_deref().unwrap_or("(empty)")
                );
            }
            println!("auto-switch:  {}", if s.auto_switch { "on" } else { "off" });
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

fn import(file: &Path, output: Option<&Path>) -> Result<()> {
    let (profile, diags) = dp_config::load(file)?;
    print_diagnostics(&diags);
    let toml = dp_config::to_toml(&profile);
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
    if diags.iter().any(|d| d.severity == Severity::Error) {
        bail!("profile has errors");
    }
    Ok(())
}

fn stop_flag() -> Result<Arc<AtomicBool>> {
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
        std::thread::sleep(Duration::from_millis(1));
    }
    rt.stop();
    Ok(())
}

/// Poll backends long enough for the HID scanner to find devices.
fn scan_devices() -> Runtime {
    let mut rt = Runtime::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        rt.tick();
        std::thread::sleep(Duration::from_millis(50));
    }
    rt
}

fn doctor() -> Result<()> {
    let platform = Platform::native();
    let ok = |r: dp_platform::Result<()>| match r {
        Ok(()) => "ok".to_string(),
        Err(e) => format!("NOT AVAILABLE — {e}"),
    };
    println!(
        "Doppelpad:                      {} on {}",
        env!("CARGO_PKG_VERSION"),
        dp_platform::os_version()
    );
    println!(
        "ViGEmBus (virtual controllers): {}",
        ok(platform.virtual_pads.available())
    );
    println!(
        "HidHide (hide physical pads):   {}",
        ok(platform.hider.available())
    );
    drop(platform);
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
    let rt = scan_devices();
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
                .map(|b| pad_button_name(*b))
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
