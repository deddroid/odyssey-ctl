//! Operations shared by the CLI and the GUI.

use std::thread::sleep;
use std::time::Duration;

use crate::caps::Capabilities;
use crate::ddc::VcpValue;
use crate::device::{Error, Vcp};
use crate::features::{Feature, is_disabled};
use crate::{SDM_DISABLE, SDM_ENABLE, VCP_SDM_CONTROL};

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
    if is_disabled(before.current, before.max) {
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
    if after != target {
        return Err(Error::Other(format!(
            "{}: wrote {target} but the monitor reports {after} (write ignored?)",
            f.name
        )));
    }
    Ok(Change {
        before: before.current,
        after,
    })
}

/// How long the monitor needs to apply a write before it reads back the new value. Samsung Display
/// Manager waits 400 ms after a picture mode change; 200 ms was enough for every other write tested.
fn settle_time(f: &Feature) -> Duration {
    Duration::from_millis(if f.name == "picture-mode" { 400 } else { 200 })
}

/// Reads a value shortly after a write; the monitor needs a moment to apply it.
pub fn read_back(m: &mut dyn Vcp, code: u8) -> Result<u16, Error> {
    read_back_after(m, code, Duration::from_millis(200))
}

fn read_back_after(m: &mut dyn Vcp, code: u8, wait: Duration) -> Result<u16, Error> {
    sleep(wait);
    Ok(m.get_vcp(code)?.current)
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
