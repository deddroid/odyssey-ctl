use std::process::ExitCode;

use clap::{Parser, Subcommand};

use odyssey_ctl::actions::{self, open};
use odyssey_ctl::caps::Capabilities;
use odyssey_ctl::device::{Error, Vcp};
use odyssey_ctl::features::{self, FEATURES, Feature, parse_number};
use odyssey_ctl::{SDM_DISABLE, SDM_ENABLE};

#[derive(Parser)]
#[command(version, about = "Control Samsung Odyssey monitors over DDC/CI")]
struct Cli {
    /// I2C bus number (/dev/i2c-N); detected automatically when omitted.
    #[arg(long, global = true)]
    bus: Option<u32>,
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
    /// Write a feature. The value is a number or one of the names shown by `features`.
    Set {
        feature: String,
        value: String,
        /// Allow writing features that have not been verified on hardware.
        #[arg(long)]
        unverified: bool,
    },
    /// List known features and their verification status.
    Features,
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
    match cli.command {
        Command::List => list(),
        Command::Features => {
            print_features();
            Ok(())
        }
        Command::Info => info(&mut *open(cli.bus)?.monitor),
        Command::Get { features } => get(&mut *open(cli.bus)?.monitor, &features),
        Command::Set { feature, value, unverified } => set(&mut *open(cli.bus)?.monitor, &feature, &value, unverified),
        Command::Sdm { state } => sdm(&mut *open(cli.bus)?.monitor, state == "on"),
        Command::Raw(raw) => raw_cmd(&mut *open(cli.bus)?.monitor, raw),
    }
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
        println!("{:<16} {:#04x}  {:<14} {}", f.name, f.code, f.status.as_str(), f.label);
        if !f.values.is_empty() {
            let names: Vec<String> = f.values.iter().map(|(v, n)| format!("{n}={v}")).collect();
            println!("{:<37}values: {}", "", names.join(", "));
        }
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
    features::find(name).ok_or_else(|| Error::Other(format!("unknown feature '{name}' (see `features`)")))
}

fn describe(f: &Feature, current: u16) -> String {
    if current == features::DISABLED && f.value_name(current).is_none() {
        return format!("{current} (disabled now)");
    }
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
            Ok(v) => println!("{:<16} {:<18} max {:<5} [{}]", f.name, describe(f, v.current), v.max, f.status.as_str()),
            Err(e) => println!("{:<16} {e}", f.name),
        }
    }
    Ok(())
}

fn set(m: &mut dyn Vcp, name: &str, value: &str, unverified: bool) -> Result<(), Error> {
    let f = feature(name)?;
    let target = f
        .parse_value(value)
        .ok_or_else(|| Error::Other(format!("invalid value '{value}' for {}", f.name)))?;
    let c = actions::write(m, f, target, unverified)?;
    println!("{}: {} -> {}", f.name, describe(f, c.before), describe(f, c.after));
    Ok(())
}

fn sdm(m: &mut dyn Vcp, on: bool) -> Result<(), Error> {
    let target = if on { SDM_ENABLE } else { SDM_DISABLE };
    let (eb, caps) = actions::set_sdm(m, on)?;
    println!("EB = {eb:#04x} ({})", if eb == target { "ok" } else { "unexpected" });
    println!("profile: {:?}", caps.profile());
    Ok(())
}

fn code_arg(s: &str) -> Result<u8, Error> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u8::from_str_radix(s, 16).map_err(|_| Error::Other(format!("invalid VCP code '{s}' (hex, e.g. 2f)")))
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
            let value = parse_number(&value).ok_or_else(|| Error::Other(format!("invalid value '{value}'")))?;
            m.set_vcp(code, value)?;
            println!("VCP {code:02X}: wrote {value}, reads back {}", actions::read_back(m, code)?);
        }
        Raw::Caps => println!("{}", m.capabilities()?),
    }
    Ok(())
}
