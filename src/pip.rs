//! Picture-in-Picture / Picture-by-Picture: VCP `E2` (one command per write, bit-field status
//! on read), `E3` (sources) and `E8` (sound, a plain feature). Encoding from Samsung Display
//! Manager (docs/PROTOCOL.md); the G60SD supports PIP only.

use std::thread::sleep;
use std::time::Duration;

use crate::device::{Error, Vcp};

pub const VCP_COMMAND: u8 = 0xE2;
pub const VCP_SOURCE: u8 = 0xE3;

/// A command written to `E2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Off,
    On,
    /// Switch to PBP (then pick a layout).
    PbpMode,
    /// PBP layout: 0 = 1:1, 1 = 2:1, 2 = 1:2.
    PbpLayout(u8),
    PipMode,
    /// PIP window size, 0 (small) to 2 (large).
    PipSize(u8),
    /// PIP window corner: 0 top-left, 1 top-right, 2 bottom-left, 3 bottom-right.
    PipPosition(u8),
}

impl Command {
    pub fn value(self) -> u16 {
        match self {
            Command::Off => 0x00,
            Command::On => 0x01,
            Command::PbpMode => 0x02,
            Command::PbpLayout(n) => 0x03 + u16::from(n.min(2)),
            Command::PipMode => 0x10,
            Command::PipSize(n) => 0x11 + u16::from(n.min(2)),
            Command::PipPosition(n) => 0x14 + u16::from(n.min(3)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pbp11,
    Pbp21,
    Pbp12,
    Pbp111,
    Pbp211,
    Pbp121,
    Pbp112,
    Pip,
}

/// The `E2` status bit-field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub on: bool,
    pub mode: Mode,
    /// 0..=2
    pub pip_size: u8,
    /// 0..=3 (top-left, top-right, bottom-left, bottom-right)
    pub pip_position: u8,
    /// Number of PIP sizes the monitor offers (1..=3).
    pub pip_sizes: u8,
    pub pip_supported: bool,
    pub pbp_supported: bool,
}

pub fn decode_status(v: u16) -> Status {
    let mode = match (v >> 1) & 0x7 {
        0 => Mode::Pbp11,
        1 => Mode::Pbp21,
        2 => Mode::Pbp12,
        3 => Mode::Pbp111,
        4 => Mode::Pbp211,
        5 => Mode::Pbp121,
        6 => Mode::Pbp112,
        _ => Mode::Pip,
    };
    Status {
        on: v & 1 != 0,
        mode,
        pip_size: ((v >> 4) & 0x3) as u8,
        pip_position: ((v >> 6) & 0x3) as u8,
        pip_sizes: (((v >> 8) & 0x3) as u8 + 1).min(3),
        pip_supported: v & 0x1000 != 0,
        pbp_supported: v & 0x2000 != 0,
    }
}

/// Video inputs as `E3` names them: `type << 4 | n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source(pub u8);

/// (name, code) of the sources a Samsung monitor can name. Only those in the monitor's own
/// input list (VCP 60) make sense.
pub const SOURCES: &[(&str, u8)] = &[
    ("hdmi1", 0x00),
    ("hdmi2", 0x01),
    ("hdmi3", 0x02),
    ("hdmi4", 0x03),
    ("dp1", 0x10),
    ("dp2", 0x11),
    ("dp3", 0x12),
    ("dp4", 0x13),
    ("usbc1", 0x20),
    ("usbc2", 0x21),
    ("thunderbolt1", 0x30),
    ("thunderbolt2", 0x31),
    ("dvi1", 0x40),
    ("dvi2", 0x41),
    ("dsub1", 0x50),
    ("dsub2", 0x51),
];

impl Source {
    pub fn parse(s: &str) -> Option<Source> {
        SOURCES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(s))
            .map(|&(_, c)| Source(c))
    }

    pub fn name(self) -> Option<&'static str> {
        SOURCES.iter().find(|&&(_, c)| c == self.0).map(|&(n, _)| n)
    }

    /// Human form, e.g. "HDMI 1".
    pub fn label(self) -> String {
        let kind = match self.0 >> 4 {
            0 => "HDMI",
            1 => "DP",
            2 => "USB-C",
            3 => "Thunderbolt",
            4 => "DVI",
            5 => "D-SUB",
            _ => return format!("{:#04x}", self.0),
        };
        format!("{kind} {}", (self.0 & 0xF) + 1)
    }

    /// The same input as the main-input feature (VCP 60) names it, when it has a code there.
    pub fn from_input_code(code: u16) -> Option<Source> {
        match code {
            0x11 => Some(Source(0x00)),
            0x12 => Some(Source(0x01)),
            0x0F => Some(Source(0x10)),
            0x10 => Some(Source(0x11)),
            _ => None,
        }
    }
}

/// Which window a source is written to (high byte of an `E3` write).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Main = 0,
    Sub = 1,
    Sub2 = 2,
}

impl Channel {
    pub fn parse(s: &str) -> Option<Channel> {
        match s.to_ascii_lowercase().as_str() {
            "main" => Some(Channel::Main),
            "sub" | "sub1" | "channel1" => Some(Channel::Sub),
            "sub2" | "channel2" => Some(Channel::Sub2),
            _ => None,
        }
    }
}

pub fn encode_source(channel: Channel, src: Source) -> u16 {
    (channel as u16) << 8 | u16::from(src.0)
}

/// Two-window reading of `E3`: main in the high byte, sub in the low byte.
pub fn decode_sources(v: u16) -> Option<(Source, Source)> {
    if v & 0x8000 != 0 {
        return None; // three-window packing, not on this monitor
    }
    let [main, sub] = v.to_be_bytes();
    Some((Source(main), Source(sub)))
}

/// Reads `code` after `wait_ms`. Switching PIP re-syncs the video and the monitor leaves the bus
/// for a moment (verified on the G60SD): an I/O error gets one more try a second later.
fn read_after(m: &mut dyn Vcp, code: u8, wait_ms: u64) -> Result<u16, Error> {
    sleep(Duration::from_millis(wait_ms));
    match m.get_vcp(code) {
        Err(Error::Io(_)) => {
            sleep(Duration::from_secs(1));
            Ok(m.get_vcp(code)?.current)
        }
        r => Ok(r?.current),
    }
}

/// Sends one command and reads the status back. Switching the window layout re-syncs the video,
/// so the monitor gets a moment before the read-back.
pub fn command(m: &mut dyn Vcp, cmd: Command) -> Result<Status, Error> {
    m.set_vcp(VCP_COMMAND, cmd.value())?;
    let status = decode_status(read_after(m, VCP_COMMAND, 1000)?);
    let expected_on = match cmd {
        Command::Off => Some(false),
        Command::On => Some(true),
        _ => None,
    };
    if expected_on.is_some_and(|on| on != status.on) {
        return Err(Error::Other(format!(
            "PIP: asked for {} but the monitor reports {}",
            if cmd == Command::On { "on" } else { "off" },
            if status.on { "on" } else { "off" }
        )));
    }
    Ok(status)
}

/// Sets the input shown in `channel` and reads the sources back.
pub fn set_source(
    m: &mut dyn Vcp,
    channel: Channel,
    src: Source,
) -> Result<(Source, Source), Error> {
    m.set_vcp(VCP_SOURCE, encode_source(channel, src))?;
    let (main, sub) = decode_sources(read_after(m, VCP_SOURCE, 1500)?)
        .ok_or_else(|| Error::Other("unexpected PIP source format".into()))?;
    let got = match channel {
        Channel::Main => main,
        _ => sub,
    };
    if got != src {
        return Err(Error::Other(format!(
            "PIP source: asked for {} but the monitor reports {}",
            src.label(),
            got.label()
        )));
    }
    Ok((main, sub))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::DemoDevice;

    #[test]
    fn decodes_the_g60sd_status() {
        // 0x150E: off, last mode PIP, size 0, position 0, two PIP sizes, PIP supported, PBP not.
        let s = decode_status(0x150E);
        assert_eq!(
            s,
            Status {
                on: false,
                mode: Mode::Pip,
                pip_size: 0,
                pip_position: 0,
                pip_sizes: 2,
                pip_supported: true,
                pbp_supported: false,
            }
        );
        assert!(decode_status(0x150F).on);
        assert_eq!(decode_status(0x0003).mode, Mode::Pbp21);
    }

    #[test]
    fn command_values_match_sdm() {
        assert_eq!(Command::PbpLayout(1).value(), 0x04);
        assert_eq!(Command::PipSize(2).value(), 0x13);
        assert_eq!(Command::PipPosition(3).value(), 0x17);
        assert_eq!(Command::PipPosition(9).value(), 0x17);
    }

    #[test]
    fn sources() {
        assert_eq!(Source::parse("DP1"), Some(Source(0x10)));
        assert_eq!(Source(0x11).label(), "DP 2");
        assert_eq!(Source(0x01).name(), Some("hdmi2"));
        assert_eq!(encode_source(Channel::Sub, Source(0x10)), 0x0110);
        assert_eq!(decode_sources(0x0010), Some((Source(0x00), Source(0x10))));
        assert_eq!(decode_sources(0x8000), None);
        assert_eq!(Source::from_input_code(0x11), Some(Source(0x00)));
        assert_eq!(Channel::parse("sub"), Some(Channel::Sub));
    }

    #[test]
    fn demo_round_trip() {
        let mut d = DemoDevice::new();
        assert!(!command(&mut d, Command::Off).unwrap().on);
        let s = command(&mut d, Command::On).unwrap();
        assert!(s.on && s.mode == Mode::Pip);
        assert_eq!(command(&mut d, Command::PipSize(2)).unwrap().pip_size, 2);
        assert_eq!(
            command(&mut d, Command::PipPosition(3))
                .unwrap()
                .pip_position,
            3
        );
        let (main, sub) = set_source(&mut d, Channel::Sub, Source(0x10)).unwrap();
        assert_eq!((main, sub), (Source(0x00), Source(0x10)));
    }
}
