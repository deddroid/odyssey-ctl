//! Linux backend: DDC/CI over i2c-dev (`/dev/i2c-N`).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use crate::ddc::{DDC_ADDR, EDID_ADDR};
use crate::device::{Bus, DdcDevice};
use crate::edid::Edid;

const I2C_SLAVE: libc::c_ulong = 0x0703;

pub struct I2cBus {
    file: File,
}

impl I2cBus {
    pub fn open(bus: u32, addr: u16) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/i2c-{bus}"))?;
        // SAFETY: I2C_SLAVE takes the 7-bit slave address as an integer argument.
        let r = unsafe { libc::ioctl(file.as_raw_fd(), I2C_SLAVE as _, addr as libc::c_ulong) };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(I2cBus { file })
    }
}

impl Bus for I2cBus {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.file.write_all(data)
    }
    fn read(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.file.read_exact(buf)
    }
}

/// Opens the DDC/CI interface of the monitor on `/dev/i2c-{bus}`.
pub fn open(bus: u32) -> io::Result<DdcDevice<I2cBus>> {
    Ok(DdcDevice::new(I2cBus::open(bus, DDC_ADDR)?))
}

#[derive(Debug, Clone)]
pub struct Display {
    pub bus: u32,
    pub adapter: String,
    /// DRM connector, e.g. `card1-HDMI-A-1`, when its EDID matches.
    pub connector: Option<String>,
    pub edid: Edid,
}

/// Finds monitors by reading the EDID on every i2c bus that belongs to a graphics card.
pub fn discover() -> Vec<Display> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/bus/i2c/devices") else {
        return out;
    };
    let mut buses: Vec<u32> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_prefix("i2c-")?.parse().ok())
        .collect();
    buses.sort_unstable();
    let connectors = drm_edids();
    for bus in buses {
        let sys = format!("/sys/bus/i2c/devices/i2c-{bus}");
        if !is_display_adapter(Path::new(&sys)) {
            continue;
        }
        let Some(raw) = read_edid(bus) else { continue };
        let Some(edid) = Edid::parse(&raw) else {
            continue;
        };
        out.push(Display {
            bus,
            adapter: fs::read_to_string(format!("{sys}/name"))
                .unwrap_or_default()
                .trim()
                .to_string(),
            connector: connectors
                .iter()
                .find(|(_, e)| e[..] == raw[..])
                .map(|(n, _)| n.clone()),
            edid,
        });
    }
    out
}

/// True when the adapter's parent PCI device is a display controller (class 0x03xxxx),
/// which excludes motherboard SMBus controllers.
fn is_display_adapter(sys: &Path) -> bool {
    let Ok(real) = fs::canonicalize(sys) else {
        return false;
    };
    let Some(parent) = real.parent() else {
        return false;
    };
    fs::read_to_string(parent.join("class")).is_ok_and(|c| c.trim().starts_with("0x03"))
}

/// Reads the EDID base block. Retries only when something answers with bad data: a bus with no
/// monitor fails at the first write and is skipped at once.
fn read_edid(bus: u32) -> Option<Vec<u8>> {
    let mut b = I2cBus::open(bus, EDID_ADDR).ok()?;
    for attempt in 1..=3 {
        b.write(&[0]).ok()?;
        let mut buf = vec![0u8; 128];
        if b.read(&mut buf).is_ok() && Edid::parse(&buf).is_some() {
            return Some(buf);
        }
        sleep(Duration::from_millis(50 * attempt));
    }
    None
}

/// First 128 EDID bytes of every connected DRM connector.
fn drm_edids() -> Vec<(String, Vec<u8>)> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let edid = fs::read(e.path().join("edid")).ok()?;
            (edid.len() >= 128).then(|| (name, edid[..128].to_vec()))
        })
        .collect()
}
