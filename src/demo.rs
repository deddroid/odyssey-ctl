//! In-memory monitor for screenshots and testing without hardware (`ODYSSEY_CTL_DEMO=1`).
//! Values are a snapshot of a real G60SD on firmware 1006.0, plus a model of the stateful
//! registers (aim point border/position, PIP command/status) that behaves like the firmware.

use std::collections::BTreeMap;

use crate::aim::{self, Border, Position};
use crate::ddc::{ReplyError, VcpValue};
use crate::device::{Error, Vcp};
use crate::pip;
use crate::{SDM_ENABLE, VCP_SDM_CONTROL};

pub const SEC_CAPS: &str = "(prot(monitor)type(lcd)model(SEC)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(05 08 0B 0C) 16 18 1A 52 60(11 12 0F ) 62 87 8D CC mswhql(1)mccs_ver(2.0)asset_eep(32)mpu_ver(01))";
pub const LEMON_CAPS: &str = "(prot(monitor)type(lcd)model(LEMON24v1)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 0A 10 12 14(05 08 0B 0C) 16 18 1A 2D 52 2F 60(0F 11 12 ) 62 87 8D CC E0 E2 E3 E8 E4 EB ED EE EF F2 F8 F9 FA FC mswhql(1)mccs_ver(2.0)asset_eep(32)mpu_ver(01))";

/// Pixels one `EE` step moves the crosshair in the demo.
pub const AIM_STEP: u16 = 10;

/// (code, max, current)
const SNAPSHOT: &[(u8, u16, u16)] = &[
    (0x0A, 2, 0),
    (0x10, 50, 50),
    (0x12, 50, 50),
    (0x14, 4, 2),
    (0x16, 100, 65),
    (0x18, 100, 43),
    (0x1A, 100, 40),
    (0x2D, 10, 8),
    (0x2F, 10, 6),
    (0x60, 18, 0x11),
    (0x62, 100, 50),
    (0x87, 20, 13),
    (0x8D, 255, 0),
    (0xCC, 38, 5),
    (0xE0, 0, 0),
    (0xE2, 127, 0x150E),
    (0xE3, 0x5151, 0),
    (0xE4, 1, 0),
    (0xE8, 2, 0),
    (0xEB, 177, 0xA0),
    (0xED, 7, 1),
    (0xEE, 3, 0),
    (0xEF, 0x3FFF, 0),
    (0xF2, 1, 0x10),
    (0xF8, 17, 1),
    (0xF9, 5, 0),
    (0xFA, 47, 253),
];

pub struct DemoDevice {
    values: BTreeMap<u8, (u16, u16)>,
    border: Border,
    position: Position,
    /// Next `EE` item (0 x, 1 y, 2 width, 3 height).
    border_item: u8,
    /// Next `EF` item is Y.
    position_y: bool,
}

impl DemoDevice {
    pub fn new() -> Self {
        DemoDevice {
            values: SNAPSHOT.iter().map(|&(c, m, v)| (c, (m, v))).collect(),
            border: Border {
                x: 35,
                y: 20,
                width: 2490,
                height: 1400,
            },
            position: Position { x: 852, y: 600 },
            border_item: 0,
            position_y: false,
        }
    }

    fn pip_command(&mut self, v: u16) {
        let s = &mut self.values.get_mut(&pip::VCP_COMMAND).expect("snapshot").1;
        match v {
            0x00 => *s &= !1,
            0x01 => *s |= 1,
            0x02..=0x09 => *s = (*s & !0xE) | ((v.saturating_sub(3).min(6)) << 1),
            0x10 => *s = (*s & !0xE) | (7 << 1),
            0x11..=0x13 => *s = (*s & !0x30) | ((v - 0x11) << 4),
            0x14..=0x17 => *s = (*s & !0xC0) | ((v - 0x14) << 6),
            _ => {}
        }
    }
}

impl Default for DemoDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl Vcp for DemoDevice {
    fn get_vcp(&mut self, code: u8) -> Result<VcpValue, Error> {
        let &(max, mut current) = self
            .values
            .get(&code)
            .ok_or(ReplyError::Unsupported(code))?;
        match code {
            aim::VCP_MOVE => {
                let b = self.border;
                let value = [b.x, b.y, b.width, b.height][usize::from(self.border_item)];
                current = u16::from(self.border_item) << 13 | value;
                self.border_item = (self.border_item + 1) % 4;
            }
            aim::VCP_POSITION => {
                current = if self.position_y {
                    0x2000 | self.position.y
                } else {
                    self.position.x
                };
                self.position_y = !self.position_y;
            }
            _ => {}
        }
        Ok(VcpValue {
            code,
            kind: 0,
            max,
            current,
        })
    }

    fn set_vcp(&mut self, code: u8, value: u16) -> Result<(), Error> {
        match code {
            aim::VCP_MOVE => {
                let p = &mut self.position;
                match value {
                    0 => p.y = p.y.saturating_sub(AIM_STEP),
                    1 => p.y += AIM_STEP,
                    2 => p.x = p.x.saturating_sub(AIM_STEP),
                    3 => p.x += AIM_STEP,
                    _ => {}
                }
            }
            aim::VCP_POSITION => {
                if value & 0x2000 != 0 {
                    self.position.y = value & aim::MAX_COORD;
                } else {
                    self.position.x = value & aim::MAX_COORD;
                }
            }
            pip::VCP_COMMAND => self.pip_command(value),
            // F8: `0x10 | v` sets CoreSync (read as bit 1); `0`/`1` sets the lighting (bit 0).
            0xF8 => {
                let s = &mut self.values.get_mut(&code).expect("snapshot").1;
                *s = if value & 0x10 != 0 {
                    (*s & !2) | ((value & 1) << 1)
                } else {
                    (*s & !1) | (value & 1)
                };
            }
            pip::VCP_SOURCE => {
                let [channel, src] = value.to_be_bytes();
                let s = &mut self.values.get_mut(&code).expect("snapshot").1;
                let [main, sub] = s.to_be_bytes();
                *s = match channel {
                    0 => u16::from_be_bytes([src, sub]),
                    _ => u16::from_be_bytes([main, src]),
                };
            }
            _ => {
                let entry = self
                    .values
                    .get_mut(&code)
                    .ok_or(ReplyError::Unsupported(code))?;
                entry.1 = value;
            }
        }
        Ok(())
    }

    fn capabilities(&mut self) -> Result<String, Error> {
        let eb = self.values[&VCP_SDM_CONTROL].1;
        Ok(if eb == SDM_ENABLE {
            LEMON_CAPS
        } else {
            SEC_CAPS
        }
        .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SDM_DISABLE;
    use crate::caps::{Capabilities, Profile};

    #[test]
    fn sdm_switches_profile() {
        let mut d = DemoDevice::new();
        assert_eq!(
            Capabilities::parse(&d.capabilities().unwrap()).profile(),
            Profile::Sec
        );
        d.set_vcp(VCP_SDM_CONTROL, SDM_ENABLE).unwrap();
        assert!(matches!(
            Capabilities::parse(&d.capabilities().unwrap()).profile(),
            Profile::Lemon(_)
        ));
        d.set_vcp(VCP_SDM_CONTROL, SDM_DISABLE).unwrap();
        assert_eq!(
            Capabilities::parse(&d.capabilities().unwrap()).profile(),
            Profile::Sec
        );
    }

    #[test]
    fn unknown_code_is_unsupported() {
        let mut d = DemoDevice::new();
        assert!(matches!(
            d.get_vcp(0x01),
            Err(Error::Reply(ReplyError::Unsupported(1)))
        ));
    }

    #[test]
    fn stateful_registers_cycle_like_the_firmware() {
        let mut d = DemoDevice::new();
        // EE: x, y, width, height, then x again.
        let items: Vec<u16> = (0..5).map(|_| d.get_vcp(0xEE).unwrap().current).collect();
        assert_eq!(items, [35, 0x2000 | 20, 0x4000 | 2490, 0x6000 | 1400, 35]);
        // EF: x, y, x.
        assert_eq!(d.get_vcp(0xEF).unwrap().current, 852);
        assert_eq!(d.get_vcp(0xEF).unwrap().current, 0x2000 | 600);
        assert_eq!(d.get_vcp(0xEF).unwrap().current, 852);
    }
}
