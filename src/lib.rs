//! Control library for Samsung Odyssey monitors over DDC/CI.
//!
//! Packet building and parsing (`ddc`, `caps`, `edid`) is pure and tested without hardware;
//! `device` adds timing and retries over any [`device::Bus`]; `linux` provides the i2c-dev bus.

pub mod actions;
pub mod aim;
pub mod caps;
pub mod config;
pub mod ddc;
pub mod demo;
pub mod device;
pub mod edid;
pub mod features;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod pip;
pub mod presets;

/// VCP code of Samsung's SDM control register (see docs/PROTOCOL.md).
pub const VCP_SDM_CONTROL: u8 = 0xEB;
/// EB value that switches the monitor to the extended (LEMON) profile.
pub const SDM_ENABLE: u16 = 0xA1;
/// EB value that returns the monitor to the retail (SEC) profile.
pub const SDM_DISABLE: u16 = 0xA2;

/// Raw DDC traffic is printed to stderr when `ODYSSEY_CTL_DEBUG=1`.
pub fn debug_enabled() -> bool {
    std::env::var("ODYSSEY_CTL_DEBUG").is_ok_and(|v| v == "1")
}
