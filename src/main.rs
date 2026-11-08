use std::process::ExitCode;

use clap::{Parser, Subcommand};

use odyssey_ctl::actions::{self, open};
use odyssey_ctl::caps::Capabilities;
use odyssey_ctl::device::{Error, Vcp};
use odyssey_ctl::features::{self, FEATURES, Feature, parse_number, parse_relative};
use odyssey_ctl::presets::{self, Preset, Presets};
use odyssey_ctl::{SDM_DISABLE, SDM_ENABLE, aim, pip};

#[derive(Parser)]
#[command(version, about = "Control Samsung Odyssey monitors over DDC/CI")]
struct Cli {
    /// I2C bus number (/dev/i2c-N); detected automatically when omitted.
    #[arg(long, global = true)]
    bus: Option<u32>,
    /// Allow writes that have not been verified on hardware (see `features`).
    #[arg(long, global = true)]
    unverified: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List detected monitors.
    List,
    /// Show the capability string and firmware profile.
    Info,
    /// Read features (all known features when none is given).
    Get { features: Vec<String> },
    /// Write a feature. The value is a number, one of the names shown by `features`, or a
    /// relative step such as +5 or -5.
    Set {
        feature: String,
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
    /// Presets saved in presets.toml, shared with the desktop app and its tray menu.
    #[command(subcommand)]
    Preset(PresetCmd),
    /// List known features and actions with their verification status.
    Features,
    /// Virtual Aim Point placement (choose an icon with `set aim-point` first).
    #[command(subcommand)]
    Aim(Aim),
    /// Picture-in-Picture / Picture-by-Picture.
    #[command(subcommand)]
    Pip(Pip),
    /// CoreSync: the lighting follows the screen content.
    Coresync {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Reset the white balance (red, green, blue gains) of the current picture mode.
    ResetWhiteBalance,
    /// Switch Samsung's extended (SDM) profile on or off via VCP EB.
    Sdm {
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Low-level access by VCP code.
    #[command(subcommand)]
    Raw(Raw),
}

#[derive(Subcommand)]
enum PresetCmd {
    /// List the saved presets.
    List,
    /// Save the monitor's current settings under this name (replaces a preset with the same name).
    Save { name: String },
    /// Apply a preset, writing only the settings that differ from the monitor.
    Apply { name: String },
}

#[derive(Subcommand)]
enum Aim {
    /// Where the crosshair is and the area it can be placed in.
    Status,
    /// Move the crosshair one step (or `--steps` steps, 500 ms apart: faster steps hide it).
    Move {
        #[arg(value_parser = ["up", "down", "left", "right"])]
        direction: String,
        #[arg(long, default_value_t = 1)]
        steps: u16,
    },
    /// Put the crosshair at X, Y (inside the border shown by `status`), or at the centre.
    Position {
        x: Option<u16>,
        y: Option<u16>,
        #[arg(long, conflicts_with_all = ["x", "y"])]
        center: bool,
    },
}

#[derive(Subcommand)]
enum Pip {
    /// Decoded PIP/PBP status and sources.
    Status,
    On,
    Off,
    /// Window layout: pip, or a PBP layout (pbp11, pbp21, pbp12) on monitors that support PBP.
    Mode {
        #[arg(value_parser = ["pip", "pbp11", "pbp21", "pbp12"])]
        mode: String,
    },
    /// PIP window size, 1 (small) to 3 (large).
    Size {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=3))]
        size: u8,
    },
    /// PIP window corner: 1 top-left, 2 top-right, 3 bottom-left, 4 bottom-right.
    Position {
        #[arg(value_parser = clap::value_parser!(u8).range(1..=4))]
        corner: u8,
    },
    /// Input of a window: `main` or `sub`, then hdmi1, hdmi2, dp1, ...
    Source {
        #[arg(value_parser = ["main", "sub"])]
        window: String,
        input: String,
    },
}

#[derive(Subcommand)]
enum Raw {
    Get { code: String },
    Set { code: String, value: String },
    Caps,
}

fn main() -> ExitCode {
    // Exit quietly when stdout is closed early (e.g. `odyssey-ctl features | head`).
    #[cfg(target_os = "linux")]
    // SAFETY: restoring the default SIGPIPE disposition before any other thread exists.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    let allow = cli.unverified;
    match cli.command {
        Command::List => list(),
        Command::Features => {
            print_features();
            Ok(())
        }
        Command::Info => info(&mut *open(cli.bus)?.monitor),
        Command::Get { features } => get(&mut *open(cli.bus)?.monitor, &features),
        Command::Set { feature, value } => {
            set(&mut *open(cli.bus)?.monitor, &feature, &value, allow)
        }
        Command::Preset(PresetCmd::List) => {
            preset_list();
            Ok(())
        }
        Command::Preset(PresetCmd::Save { name }) => {
            preset_save(&mut *open(cli.bus)?.monitor, &name, allow)
        }
        Command::Preset(PresetCmd::Apply { name }) => {
            preset_apply(&mut *open(cli.bus)?.monitor, &name, allow)
        }
        Command::Aim(cmd) => aim_cmd(&mut *open(cli.bus)?.monitor, cmd, allow),
        Command::Pip(cmd) => pip_cmd(&mut *open(cli.bus)?.monitor, cmd, allow),
        Command::Coresync { state } => {
            let after = actions::set_coresync(&mut *open(cli.bus)?.monitor, state == "on", allow)?;
            println!(
                "coresync: {} (lighting {})",
                on_off(after & 2 != 0),
                on_off(after & 1 != 0)
            );
            Ok(())
        }
        Command::ResetWhiteBalance => {
            let [r, g, b] = actions::reset_white_balance(&mut *open(cli.bus)?.monitor, allow)?;
            println!("white balance reset: red {r}, green {g}, blue {b}");
            Ok(())
        }
        Command::Sdm { state } => sdm(&mut *open(cli.bus)?.monitor, state == "on"),
        Command::Raw(raw) => raw_cmd(&mut *open(cli.bus)?.monitor, raw),
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

fn aim_cmd(m: &mut dyn Vcp, cmd: Aim, allow: bool) -> Result<(), Error> {
    let p = match cmd {
        Aim::Status => {
            let (p, b) = actions::aim_state(m)?;
            println!(
                "border:   x {} y {} width {} height {}",
                b.x, b.y, b.width, b.height
            );
            p
        }
        Aim::Move { direction, steps } => {
            let dir = aim::Move::parse(&direction).expect("validated by clap");
            actions::aim_move(m, dir, steps, allow)?
        }
        Aim::Position { x, y, center } => {
            let target = match (x, y, center) {
                (Some(x), Some(y), _) => aim::Position { x, y },
                (None, None, true) => aim::read_border(m)?.center(),
                (None, None, false) => {
                    println!("position: {}", describe_position(aim::read_position(m)?));
                    return Ok(());
                }
                _ => return Err(Error::Other("give both X and Y, or --center".into())),
            };
            actions::aim_position(m, target, allow)?
        }
    };
    println!("position: {}", describe_position(p));
    Ok(())
}

fn describe_position(p: aim::Position) -> String {
    format!("x {} y {}", p.x, p.y)
}

fn pip_cmd(m: &mut dyn Vcp, cmd: Pip, allow: bool) -> Result<(), Error> {
    let status = match cmd {
        Pip::Status => pip::decode_status(m.get_vcp(pip::VCP_COMMAND)?.current),
        Pip::On => actions::pip_command(m, pip::Command::On, allow)?,
        Pip::Off => actions::pip_command(m, pip::Command::Off, allow)?,
        Pip::Mode { mode } => {
            let cmd = match mode.as_str() {
                "pip" => pip::Command::PipMode,
                "pbp11" => pip::Command::PbpLayout(0),
                "pbp21" => pip::Command::PbpLayout(1),
                _ => pip::Command::PbpLayout(2),
            };
            if cmd != pip::Command::PipMode {
                actions::pip_command(m, pip::Command::PbpMode, allow)?;
            }
            actions::pip_command(m, cmd, allow)?
        }
        Pip::Size { size } => actions::pip_command(m, pip::Command::PipSize(size - 1), allow)?,
        Pip::Position { corner } => {
            actions::pip_command(m, pip::Command::PipPosition(corner - 1), allow)?
        }
        Pip::Source { window, input } => {
            let channel = pip::Channel::parse(&window).expect("validated by clap");
            let src = pip::Source::parse(&input).ok_or_else(|| {
                let names: Vec<&str> = pip::SOURCES.iter().map(|(n, _)| *n).collect();
                Error::Other(format!(
                    "unknown input '{input}' (one of: {})",
                    names.join(", ")
                ))
            })?;
            let (main, sub) = actions::pip_source(m, channel, src, allow)?;
            println!("sources: main {} · sub {}", main.label(), sub.label());
            return Ok(());
        }
    };
    println!(
        "pip: {}  mode {:?}  size {}/{}  corner {}  supports {}{}",
        on_off(status.on),
        status.mode,
        status.pip_size + 1,
        status.pip_sizes,
        ["top-left", "top-right", "bottom-left", "bottom-right"][usize::from(status.pip_position)],
        if status.pip_supported { "PIP " } else { "" },
        if status.pbp_supported { "PBP" } else { "" },
    );
    if let Some((main, sub)) = pip::decode_sources(m.get_vcp(pip::VCP_SOURCE)?.current) {
        println!("sources: main {} · sub {}", main.label(), sub.label());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn list() -> Result<(), Error> {
    let displays = odyssey_ctl::linux::discover();
    if displays.is_empty() {
        println!("No monitor found. Is i2c-dev loaded and do you have access to /dev/i2c-*?");
    }
    for d in displays {
        println!(
            "/dev/i2c-{}  {} {}  serial {}  [{}]  {}",
            d.bus,
            d.edid.manufacturer,
            d.edid.name.as_deref().unwrap_or("?"),
            d.edid.serial.as_deref().unwrap_or("?"),
            d.connector.as_deref().unwrap_or("no DRM match"),
            d.adapter,
        );
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn list() -> Result<(), Error> {
    Err(Error::Other("this platform is not supported yet".into()))
}

fn print_features() {
    println!("{:<16} {:<5} {:<14} DESCRIPTION", "NAME", "VCP", "STATUS");
    for f in FEATURES {
        println!(
            "{:<16} {:#04x}  {:<14} {}",
            f.name,
            f.code,
            f.status.as_str(),
            f.label
        );
        if !f.values.is_empty() {
            let names: Vec<String> = f.values.iter().map(|(v, n)| format!("{n}={v}")).collect();
            println!("{:<37}values: {}", "", names.join(", "));
        }
    }
    println!();
    println!("{:<22} {:<14} ACTION", "NAME", "STATUS");
    for (name, status, label) in features::ACTIONS {
        println!("{name:<22} {:<14} {label}", status.as_str());
    }
}

fn info(m: &mut dyn Vcp) -> Result<(), Error> {
    let caps = Capabilities::parse(&m.capabilities()?);
    println!("model:   {}", caps.model.as_deref().unwrap_or("?"));
    println!("profile: {:?}", caps.profile());
    println!("MCCS:    {}", caps.mccs_version.as_deref().unwrap_or("?"));
    let codes: Vec<String> = caps.vcp.keys().map(|c| format!("{c:02X}")).collect();
    println!("vcp:     {}", codes.join(" "));
    println!("raw:     {}", caps.raw);
    Ok(())
}

fn feature(name: &str) -> Result<&'static Feature, Error> {
    features::find(name)
        .ok_or_else(|| Error::Other(format!("unknown feature '{name}' (see `features`)")))
}

fn describe(f: &Feature, raw: u16) -> String {
    if features::disabled(f, raw, 0) && f.value_name(raw).is_none() {
        return format!("{raw} (disabled now)");
    }
    let current = features::state(f, raw);
    match f.value_name(current) {
        Some(n) => format!("{current} ({n})"),
        None => current.to_string(),
    }
}

fn get(m: &mut dyn Vcp, names: &[String]) -> Result<(), Error> {
    let list: Vec<&Feature> = if names.is_empty() {
        FEATURES.iter().collect()
    } else {
        names.iter().map(|n| feature(n)).collect::<Result<_, _>>()?
    };
    for f in list {
        match m.get_vcp(f.code) {
            Ok(v) => println!(
                "{:<16} {:<18} max {:<5} [{}]",
                f.name,
                describe(f, v.current),
                v.max,
                f.status.as_str()
            ),
            Err(e) => println!("{:<16} {e}", f.name),
        }
    }
    Ok(())
}

fn set(m: &mut dyn Vcp, name: &str, value: &str, unverified: bool) -> Result<(), Error> {
    let f = feature(name)?;
    let c = match parse_relative(value) {
        Some(delta) => actions::adjust(m, f, delta, unverified)?,
        None => {
            let target = f
                .parse_value(value)
                .ok_or_else(|| Error::Other(format!("invalid value '{value}' for {}", f.name)))?;
            actions::write(m, f, target, unverified)?
        }
    };
    println!(
        "{}: {} -> {}",
        f.name,
        describe(f, c.before),
        describe(f, c.after)
    );
    Ok(())
}

fn preset_list() {
    let presets = Presets::load();
    if presets.list.is_empty() {
        println!("no presets (save one with `preset save <name>` or from the desktop app)");
    }
    for p in &presets.list {
        let mode = features::find("picture-mode")
            .zip(p.values.get("picture-mode"))
            .and_then(|(f, &v)| f.value_name(v))
            .map(|n| format!(" (picture mode {n})"))
            .unwrap_or_default();
        println!("{:<20} {} settings{mode}", p.name, p.values.len());
    }
}

fn preset_save(m: &mut dyn Vcp, name: &str, unverified: bool) -> Result<(), Error> {
    let readings = actions::read(m, &FEATURES.iter().collect::<Vec<_>>())?;
    let current = readings
        .into_iter()
        .filter_map(|r| Some((r.feature.code, r.value.ok()?)))
        .collect();
    let values = presets::capture(&current, |f| f.status.safe_to_write() || unverified);
    if values.is_empty() {
        return Err(Error::Other(
            "nothing to save: no writable setting was read".into(),
        ));
    }
    let mut presets = Presets::load();
    presets.put(Preset {
        name: name.to_string(),
        values,
    });
    presets.save().map_err(Error::Io)?;
    let saved = presets.find(name).map_or(0, |p| p.values.len());
    println!("preset '{name}' saved with {saved} settings");
    Ok(())
}

fn preset_apply(m: &mut dyn Vcp, name: &str, unverified: bool) -> Result<(), Error> {
    let presets = Presets::load();
    let preset = presets
        .find(name)
        .ok_or_else(|| Error::Other(format!("no preset called '{name}' (see `preset list`)")))?;
    let report = presets::apply(m, &preset.values, unverified);
    for f in &report.changed {
        println!("{:<16} written", f.name);
    }
    for f in &report.skipped {
        println!("{:<16} skipped (locked or disabled now)", f.name);
    }
    for (f, e) in &report.failed {
        println!("{:<16} failed: {e}", f.name);
    }
    println!(
        "preset '{}': {} written, {} unchanged or skipped, {} failed",
        preset.name,
        report.changed.len(),
        preset.values.len() - report.changed.len() - report.failed.len(),
        report.failed.len()
    );
    if report.failed.is_empty() {
        Ok(())
    } else {
        Err(Error::Other(format!(
            "{} setting(s) could not be written",
            report.failed.len()
        )))
    }
}

fn sdm(m: &mut dyn Vcp, on: bool) -> Result<(), Error> {
    let target = if on { SDM_ENABLE } else { SDM_DISABLE };
    let (eb, caps) = actions::set_sdm(m, on)?;
    println!(
        "EB = {eb:#04x} ({})",
        if eb == target { "ok" } else { "unexpected" }
    );
    println!("profile: {:?}", caps.profile());
    Ok(())
}

fn code_arg(s: &str) -> Result<u8, Error> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u8::from_str_radix(s, 16)
        .map_err(|_| Error::Other(format!("invalid VCP code '{s}' (hex, e.g. 2f)")))
}

fn raw_cmd(m: &mut dyn Vcp, raw: Raw) -> Result<(), Error> {
    match raw {
        Raw::Get { code } => {
            let v = m.get_vcp(code_arg(&code)?)?;
            let b = v.bytes();
            println!(
                "VCP {:02X}: current {} max {} type {} bytes {:02x} {:02x} {:02x} {:02x}",
                v.code, v.current, v.max, v.kind, b[0], b[1], b[2], b[3]
            );
        }
        Raw::Set { code, value } => {
            let code = code_arg(&code)?;
            let value = parse_number(&value)
                .ok_or_else(|| Error::Other(format!("invalid value '{value}'")))?;
            m.set_vcp(code, value)?;
            println!(
                "VCP {code:02X}: wrote {value}, reads back {}",
                actions::read_back(m, code)?
            );
        }
        Raw::Caps => println!("{}", m.capabilities()?),
    }
    Ok(())
}
