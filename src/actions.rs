//! Operations shared by the CLI and the GUI.

use std::thread::sleep;
use std::time::Duration;

use crate::caps::Capabilities;
use crate::ddc::VcpValue;
use crate::device::{Error, Vcp};
use crate::features::{self, Feature};
use crate::{SDM_DISABLE, SDM_ENABLE, VCP_SDM_CONTROL, aim, pip};

pub type Monitor = Box<dyn Vcp + Send>;

pub struct Opened {
    pub monitor: Monitor,
    /// Product name from the EDID (e.g. "Odyssey G60SD"), when known.
    pub name: Option<String>,
}

/// Opens the monitor on `bus`, or the first Samsung monitor found.
/// `ODYSSEY_CTL_DEMO=1` returns an in-memory monitor instead.
pub fn open(bus: Option<u32>) -> Result<Opened, Error> {
    if std::env::var("ODYSSEY_CTL_DEMO").is_ok_and(|v| v == "1") {
        let monitor: Monitor = Box::<crate::demo::DemoDevice>::default();
        return Ok(Opened {
            monitor,
            name: Some("Odyssey G60SD (demo)".into()),
        });
    }
    open_platform(bus)
}

#[cfg(target_os = "linux")]
fn open_platform(bus: Option<u32>) -> Result<Opened, Error> {
    let displays = crate::linux::discover();
    let display = match bus {
        Some(b) => displays.into_iter().find(|d| d.bus == b),
        None => displays.into_iter().find(|d| d.edid.is_samsung()),
    };
    let bus = match (bus, &display) {
        (Some(b), _) => b,
        (None, Some(d)) => d.bus,
        (None, None) => {
            return Err(Error::Other(
                "no Samsung monitor found (try `list` or --bus)".into(),
            ));
        }
    };
    let monitor: Monitor = Box::new(crate::linux::open(bus)?);
    Ok(Opened {
        monitor,
        name: display.and_then(|d| d.edid.name),
    })
}

#[cfg(not(target_os = "linux"))]
fn open_platform(_bus: Option<u32>) -> Result<Opened, Error> {
    Err(Error::Other("this platform is not supported yet".into()))
}

#[derive(Debug, Clone)]
pub struct Reading {
    pub feature: &'static Feature,
    pub value: Result<VcpValue, String>,
}

/// Reads `features`. A reply error (unsupported code, bad checksum after retries) is kept per
/// feature; an I/O error means the monitor is gone, so the whole batch fails at once.
pub fn read(m: &mut dyn Vcp, features: &[&'static Feature]) -> Result<Vec<Reading>, Error> {
    features
        .iter()
        .map(|&feature| match m.get_vcp(feature.code) {
            Err(e @ Error::Io(_)) => Err(e),
            value => Ok(Reading {
                feature,
                value: value.map_err(|e| e.to_string()),
            }),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub before: u16,
    pub after: u16,
}

/// Writes `target` after checking the verification status and the monitor's maximum,
/// then reads the value back. Fails when the monitor does not apply the write.
pub fn write(
    m: &mut dyn Vcp,
    f: &Feature,
    target: u16,
    allow_unverified: bool,
) -> Result<Change, Error> {
    check_policy(f, allow_unverified)?;
    let before = m.get_vcp(f.code)?;
    write_from(m, f, before, target)
}

fn check_policy(f: &Feature, allow_unverified: bool) -> Result<(), Error> {
    if f.read_only {
        return Err(Error::Other(format!(
            "'{}' is read-only (its value format is not decoded yet)",
            f.name
        )));
    }
    if !f.status.safe_to_write() && !allow_unverified {
        return Err(Error::Other(format!(
            "'{}' is {} on hardware; pass --unverified to write it anyway",
            f.name,
            f.status.as_str()
        )));
    }
    Ok(())
}

/// Like [`write`], for callers that have just read `before` themselves (saves one transaction).
pub fn write_from(
    m: &mut dyn Vcp,
    f: &Feature,
    before: VcpValue,
    target: u16,
) -> Result<Change, Error> {
    if features::disabled(f, before.current, before.max) {
        return Err(Error::Other(format!(
            "'{}' is disabled in the monitor's current state (e.g. by Eye Saver, PIP or the picture mode)",
            f.name
        )));
    }
    if target > before.max {
        return Err(Error::Other(format!(
            "{target} is above the maximum {} of {}",
            before.max, f.name
        )));
    }
    m.set_vcp(f.code, target)?;
    let after = read_back_after(m, f.code, settle_time(f))?;
    if features::state(f, after) != target {
        return Err(Error::Other(format!(
            "{}: wrote {target} but the monitor reports {after} (write ignored?)",
            f.name
        )));
    }
    Ok(Change {
        before: features::state(f, before.current),
        after: features::state(f, after),
    })
}

/// Refuses an operation from [`features::ACTIONS`] that is not verified, unless allowed.
pub fn check_action(name: &str, allow_unverified: bool) -> Result<(), Error> {
    let status = features::action_status(name)
        .ok_or_else(|| Error::Other(format!("unknown action '{name}'")))?;
    if !status.safe_to_write() && !allow_unverified {
        return Err(Error::Other(format!(
            "'{name}' is {} on hardware; pass --unverified to do it anyway",
            status.as_str()
        )));
    }
    Ok(())
}

/// Position and border of the Virtual Aim Point (both stateful reads, done in full cycles).
pub fn aim_state(m: &mut dyn Vcp) -> Result<(aim::Position, aim::Border), Error> {
    Ok((aim::read_position(m)?, aim::read_border(m)?))
}

/// The aim point must be shown for a move or a position to mean anything.
fn check_aim_on(m: &mut dyn Vcp) -> Result<(), Error> {
    let icon = features::find("aim-point").expect("known feature");
    if m.get_vcp(icon.code)?.current == 0 {
        return Err(Error::Other(
            "the Virtual Aim Point is off: choose an icon first".into(),
        ));
    }
    Ok(())
}

/// Moves the crosshair `steps` times in `dir`, then reads where it is.
pub fn aim_move(
    m: &mut dyn Vcp,
    dir: aim::Move,
    steps: u16,
    allow_unverified: bool,
) -> Result<aim::Position, Error> {
    check_action("aim-move", allow_unverified)?;
    check_aim_on(m)?;
    for i in 0..steps.max(1) {
        if i > 0 {
            // Verified on the G60SD: steps closer than ~100 ms are partly dropped and hide the
            // crosshair until the next isolated write; 500 ms apart every step lands and the
            // crosshair stays. For many steps prefer `aim_position`.
            sleep(Duration::from_millis(aim::STEP_PACING_MS));
        }
        aim::step(m, dir)?;
    }
    sleep(Duration::from_millis(200));
    aim::read_position(m)
}

pub fn aim_position(
    m: &mut dyn Vcp,
    p: aim::Position,
    allow_unverified: bool,
) -> Result<aim::Position, Error> {
    check_action("aim-position", allow_unverified)?;
    check_aim_on(m)?;
    aim::set_position(m, p)
}

pub fn pip_command(
    m: &mut dyn Vcp,
    cmd: pip::Command,
    allow_unverified: bool,
) -> Result<pip::Status, Error> {
    check_action("pip", allow_unverified)?;
    pip::command(m, cmd)
}

pub fn pip_source(
    m: &mut dyn Vcp,
    channel: pip::Channel,
    src: pip::Source,
    allow_unverified: bool,
) -> Result<(pip::Source, pip::Source), Error> {
    check_action("pip-source", allow_unverified)?;
    pip::set_source(m, channel, src)
}

/// CoreSync (the lighting follows the screen content): F8 bit 1, written as Samsung Display
/// Manager for Windows does (`0x10` / `0x11`). Verified on the G60SD: the write only takes
/// effect with the Static effect selected, so that is set first, as SDM does. While CoreSync is
/// on, effect and colour read "disabled". Returns the lighting reading afterwards: bit 0
/// lighting, bit 1 CoreSync.
pub fn set_coresync(m: &mut dyn Vcp, on: bool, allow_unverified: bool) -> Result<u16, Error> {
    check_action("coresync", allow_unverified)?;
    let lighting = features::find("lighting").expect("known feature");
    if on {
        let effect = features::find("lighting-effect").expect("known feature");
        let now = m.get_vcp(effect.code)?;
        if !features::disabled(effect, now.current, now.max) && now.current != CORESYNC_EFFECT {
            write_from(m, effect, now, CORESYNC_EFFECT)?;
        }
    }
    m.set_vcp(lighting.code, 0x10 | u16::from(on))?;
    let after = read_back(m, lighting.code)?;
    if (after & 2 != 0) != on {
        return Err(Error::Other(format!(
            "CoreSync: asked for {} but the lighting reads {after:#04x}",
            if on { "on" } else { "off" }
        )));
    }
    Ok(after)
}

/// Resets the white balance of the current picture mode (E0 = 0). The register only acts in the
/// LEMON profile, which is switched on for the write and back off afterwards. Returns the new
/// red, green and blue gains.
pub fn reset_white_balance(m: &mut dyn Vcp, allow_unverified: bool) -> Result<[u16; 3], Error> {
    check_action("white-balance-reset", allow_unverified)?;
    let gains = ["red-gain", "green-gain", "blue-gain"].map(|n| features::find(n).expect("known"));
    let was_lemon = matches!(
        Capabilities::parse(&m.capabilities()?).profile(),
        crate::caps::Profile::Lemon(_)
    );
    if !was_lemon {
        set_sdm(m, true)?;
    }
    let result = m.set_vcp(VCP_WHITE_BALANCE_RESET, 0).and_then(|()| {
        sleep(Duration::from_millis(400));
        Ok([
            m.get_vcp(gains[0].code)?.current,
            m.get_vcp(gains[1].code)?.current,
            m.get_vcp(gains[2].code)?.current,
        ])
    });
    if !was_lemon {
        set_sdm(m, false)?;
    }
    result
}

/// Lighting effect CoreSync needs before it can be switched on (3 = Static).
const CORESYNC_EFFECT: u16 = 3;

/// VCP E0 in the LEMON profile: writing 0 resets the white balance.
pub const VCP_WHITE_BALANCE_RESET: u8 = 0xE0;

/// A monitor the library can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayInfo {
    pub bus: u32,
    pub name: String,
    pub samsung: bool,
}

/// Monitors found on this machine (one fake monitor in demo mode).
pub fn discover() -> Vec<DisplayInfo> {
    if std::env::var("ODYSSEY_CTL_DEMO").is_ok_and(|v| v == "1") {
        return vec![DisplayInfo {
            bus: 0,
            name: "Odyssey G60SD (demo)".into(),
            samsung: true,
        }];
    }
    discover_platform()
}

#[cfg(target_os = "linux")]
fn discover_platform() -> Vec<DisplayInfo> {
    crate::linux::discover()
        .into_iter()
        .map(|d| DisplayInfo {
            bus: d.bus,
            samsung: d.edid.is_samsung(),
            name: format!(
                "{} {}{}",
                d.edid.manufacturer,
                d.edid.name.as_deref().unwrap_or("?"),
                d.connector
                    .as_deref()
                    .map(|c| format!(" · {c}"))
                    .unwrap_or_default()
            ),
        })
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn discover_platform() -> Vec<DisplayInfo> {
    Vec::new()
}

/// How long the monitor needs to apply a write before it reads back the new value. Samsung Display
/// Manager waits 400 ms after a picture mode change; 200 ms was enough for every other write tested.
fn settle_time(f: &Feature) -> Duration {
    Duration::from_millis(match f.name {
        "picture-mode" => 400,
        // The video link is renegotiated: the monitor is away from the bus for a moment.
        "adaptive-sync" => 1000,
        _ => 200,
    })
}

/// Reads a value shortly after a write; the monitor needs a moment to apply it.
pub fn read_back(m: &mut dyn Vcp, code: u8) -> Result<u16, Error> {
    read_back_after(m, code, Duration::from_millis(200))
}

fn read_back_after(m: &mut dyn Vcp, code: u8, wait: Duration) -> Result<u16, Error> {
    sleep(wait);
    match m.get_vcp(code) {
        // Seen when a write makes the monitor re-sync its input (FreeSync): one more try.
        Err(Error::Io(_)) => {
            sleep(Duration::from_secs(1));
            Ok(m.get_vcp(code)?.current)
        }
        r => Ok(r?.current),
    }
}

/// Switches Samsung's extended (SDM) profile on or off. Returns the EB value read back and
/// the capabilities reported afterwards.
pub fn set_sdm(m: &mut dyn Vcp, on: bool) -> Result<(u16, Capabilities), Error> {
    m.set_vcp(VCP_SDM_CONTROL, if on { SDM_ENABLE } else { SDM_DISABLE })?;
    let eb = read_back(m, VCP_SDM_CONTROL)?;
    Ok((eb, Capabilities::parse(&m.capabilities()?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::DemoDevice;
    use crate::features::find;

    #[test]
    fn write_checks_policy_range_and_readback() {
        let mut d = DemoDevice::new();
        let be = find("black-equalizer").unwrap();
        assert_eq!(
            write(&mut d, be, 4, false).unwrap(),
            Change {
                before: 6,
                after: 4
            }
        );
        assert!(write(&mut d, be, 11, false).is_err());
        assert!(write(&mut d, find("pip-sound").unwrap(), 1, false).is_err());
        assert!(write(&mut d, find("pip-sound").unwrap(), 1, true).is_ok());
        assert!(write(&mut d, find("pip-source").unwrap(), 0, true).is_err());
    }

    #[test]
    fn write_refuses_disabled_feature() {
        // The demo snapshot has lighting-color = 0xFD (no colour for the Aurora effect).
        let mut d = DemoDevice::new();
        let err = write(&mut d, find("lighting-color").unwrap(), 3, true).unwrap_err();
        assert!(err.to_string().contains("disabled"));
    }

    #[test]
    fn packed_features_compare_their_state() {
        // Adaptive-Sync reads 0x10 (FreeSync, off): writing 1 must be accepted when it reads 0x11.
        let mut d = DemoDevice::new();
        let sync = find("adaptive-sync").unwrap();
        assert_eq!(
            write(&mut d, sync, 1, false).unwrap(),
            Change {
                before: 0,
                after: 1
            }
        );
        assert_eq!(d.get_vcp(0xF2).unwrap().current, 1); // the demo stores the raw write
    }

    #[test]
    fn actions_are_gated_and_work_on_the_demo() {
        let mut d = DemoDevice::new();
        assert!(reset_white_balance(&mut d, false).is_err());
        // Aim point off: refused even when allowed.
        d.set_vcp(0xED, 0).unwrap();
        assert!(aim_move(&mut d, aim::Move::Up, 1, true).is_err());
        d.set_vcp(0xED, 1).unwrap();
        assert_eq!(aim_move(&mut d, aim::Move::Up, 2, true).unwrap().y, 580);
        let p = aim::Position { x: 100, y: 100 };
        assert_eq!(aim_position(&mut d, p, true).unwrap(), p);
        assert!(pip_command(&mut d, pip::Command::On, true).unwrap().on);
        // Read layout: bit 0 lighting, bit 1 CoreSync.
        assert_eq!(set_coresync(&mut d, true, true).unwrap(), 0x03);
        assert_eq!(set_coresync(&mut d, false, true).unwrap(), 0x01);
        // The reset switches to LEMON for the write and back to SEC afterwards.
        reset_white_balance(&mut d, true).unwrap();
        assert_eq!(d.get_vcp(VCP_SDM_CONTROL).unwrap().current, SDM_DISABLE);
    }

    #[test]
    fn read_returns_one_reading_per_feature() {
        let mut d = DemoDevice::new();
        let r = read(
            &mut d,
            &[find("brightness").unwrap(), find("picture-mode").unwrap()],
        )
        .unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].value.as_ref().unwrap().current, 50);
        assert_eq!(r[1].value.as_ref().unwrap().current, 8);
    }
}
