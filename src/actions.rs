//! Operations shared by the CLI and the GUI.

use std::thread::sleep;
use std::time::Duration;

use crate::caps::Capabilities;
use crate::ddc::VcpValue;
use crate::device::{Error, Vcp};
use crate::features::{Feature, is_disabled};
use crate::{SDM_DISABLE, SDM_ENABLE, VCP_SDM_CONTROL};

pub type Monitor = Box<dyn Vcp + Send>;

/// Opens the monitor on `bus`, or the first Samsung monitor found.
/// `ODYSSEY_CTL_DEMO=1` returns an in-memory monitor instead.
pub fn open(bus: Option<u32>) -> Result<Monitor, Error> {
    if std::env::var("ODYSSEY_CTL_DEMO").is_ok_and(|v| v == "1") {
        return Ok(Box::new(crate::demo::DemoDevice::new()));
    }
    open_platform(bus)
}

#[cfg(target_os = "linux")]
fn open_platform(bus: Option<u32>) -> Result<Monitor, Error> {
    let bus = match bus {
        Some(b) => b,
        None => crate::linux::discover()
            .into_iter()
            .find(|d| d.edid.is_samsung())
            .map(|d| d.bus)
            .ok_or_else(|| Error::Other("no Samsung monitor found (try `list` or --bus)".into()))?,
    };
    Ok(Box::new(crate::linux::open(bus)?))
}

#[cfg(not(target_os = "linux"))]
fn open_platform(_bus: Option<u32>) -> Result<Monitor, Error> {
    Err(Error::Other("this platform is not supported yet".into()))
}

#[derive(Debug, Clone)]
pub struct Reading {
    pub feature: &'static Feature,
    pub value: Result<VcpValue, String>,
}

pub fn read(m: &mut dyn Vcp, features: &[&'static Feature]) -> Vec<Reading> {
    features
        .iter()
        .map(|&feature| Reading { feature, value: m.get_vcp(feature.code).map_err(|e| e.to_string()) })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub before: u16,
    pub after: u16,
}

/// Writes `target` after checking the verification status and the monitor's maximum,
/// then reads the value back. Fails when the monitor does not apply the write.
pub fn write(m: &mut dyn Vcp, f: &Feature, target: u16, allow_unverified: bool) -> Result<Change, Error> {
    if f.read_only {
        return Err(Error::Other(format!("'{}' is read-only (its value format is not decoded yet)", f.name)));
    }
    if !f.status.safe_to_write() && !allow_unverified {
        return Err(Error::Other(format!(
            "'{}' is {} on hardware; pass --unverified to write it anyway",
            f.name,
            f.status.as_str()
        )));
    }
    let before = m.get_vcp(f.code)?;
    if is_disabled(before.current, before.max) {
        return Err(Error::Other(format!(
            "'{}' is disabled in the monitor's current state (e.g. by Eye Saver, PIP or the picture mode)",
            f.name
        )));
    }
    if target > before.max {
        return Err(Error::Other(format!("{target} is above the maximum {} of {}", before.max, f.name)));
    }
    m.set_vcp(f.code, target)?;
    let after = read_back(m, f.code)?;
    if after != target {
        return Err(Error::Other(format!(
            "{}: wrote {target} but the monitor reports {after} (write ignored?)",
            f.name
        )));
    }
    Ok(Change { before: before.current, after })
}

/// Reads a value shortly after a write; the monitor needs a moment to apply it
/// (Samsung Display Manager waits 400 ms after a picture mode change).
pub fn read_back(m: &mut dyn Vcp, code: u8) -> Result<u16, Error> {
    sleep(Duration::from_millis(400));
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
        assert_eq!(write(&mut d, be, 4, false).unwrap(), Change { before: 6, after: 4 });
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
        let r = read(&mut d, &[find("brightness").unwrap(), find("picture-mode").unwrap()]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].value.as_ref().unwrap().current, 50);
        assert_eq!(r[1].value.as_ref().unwrap().current, 8);
    }
}
