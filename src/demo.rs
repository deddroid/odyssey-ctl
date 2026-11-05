//! In-memory monitor for screenshots and testing without hardware (`ODYSSEY_CTL_DEMO=1`).
//! Values are a snapshot of a real G60SD on firmware 1006.0.

use std::collections::BTreeMap;

use crate::ddc::{ReplyError, VcpValue};
use crate::device::{Error, Vcp};
use crate::{SDM_ENABLE, VCP_SDM_CONTROL};

pub const SEC_CAPS: &str = "(prot(monitor)type(lcd)model(SEC)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(05 08 0B 0C) 16 18 1A 52 60(11 12 0F ) 62 87 8D CC mswhql(1)mccs_ver(2.0)asset_eep(32)mpu_ver(01))";
pub const LEMON_CAPS: &str = "(prot(monitor)type(lcd)model(LEMON24v1)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 0A 10 12 14(05 08 0B 0C) 16 18 1A 2D 52 2F 60(0F 11 12 ) 62 87 8D CC E0 E2 E3 E8 E4 EB ED EE EF F2 F8 F9 FA FC mswhql(1)mccs_ver(2.0)asset_eep(32)mpu_ver(01))";

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
    (0xE2, 127, 5390),
    (0xE3, 20817, 0),
    (0xE4, 1, 0),
    (0xE8, 2, 0),
    (0xEB, 177, 0xA0),
    (0xED, 7, 0),
    (0xF2, 1, 16),
    (0xF8, 17, 1),
    (0xF9, 5, 0),
    (0xFA, 47, 253),
];

pub struct DemoDevice {
    values: BTreeMap<u8, (u16, u16)>,
}

impl DemoDevice {
    pub fn new() -> Self {
        DemoDevice { values: SNAPSHOT.iter().map(|&(c, m, v)| (c, (m, v))).collect() }
    }
}

impl Default for DemoDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl Vcp for DemoDevice {
    fn get_vcp(&mut self, code: u8) -> Result<VcpValue, Error> {
        let &(max, current) = self.values.get(&code).ok_or(ReplyError::Unsupported(code))?;
        Ok(VcpValue { code, kind: 0, max, current })
    }

    fn set_vcp(&mut self, code: u8, value: u16) -> Result<(), Error> {
        let entry = self.values.get_mut(&code).ok_or(ReplyError::Unsupported(code))?;
        entry.1 = value;
        Ok(())
    }

    fn capabilities(&mut self) -> Result<String, Error> {
        let eb = self.values[&VCP_SDM_CONTROL].1;
        Ok(if eb == SDM_ENABLE { LEMON_CAPS } else { SEC_CAPS }.to_string())
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
        assert_eq!(Capabilities::parse(&d.capabilities().unwrap()).profile(), Profile::Sec);
        d.set_vcp(VCP_SDM_CONTROL, SDM_ENABLE).unwrap();
        assert!(matches!(Capabilities::parse(&d.capabilities().unwrap()).profile(), Profile::Lemon(_)));
        d.set_vcp(VCP_SDM_CONTROL, SDM_DISABLE).unwrap();
        assert_eq!(Capabilities::parse(&d.capabilities().unwrap()).profile(), Profile::Sec);
    }

    #[test]
    fn unknown_code_is_unsupported() {
        let mut d = DemoDevice::new();
        assert!(matches!(d.get_vcp(0x01), Err(Error::Reply(ReplyError::Unsupported(1)))));
    }
}
