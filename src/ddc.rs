//! DDC/CI packet encoding and decoding. Pure functions, no I/O.
//!
//! Host -> monitor messages go to I2C address 0x37:
//! `51, 80|len, payload…, checksum` where the checksum XORs every byte with 0x6E.
//! Monitor -> host replies are read from the same address:
//! `6E, 80|len, payload…, checksum` where the checksum XORs every byte with 0x50.

/// I2C address of the DDC/CI command interface.
pub const DDC_ADDR: u16 = 0x37;
/// I2C address of the EDID EEPROM.
pub const EDID_ADDR: u16 = 0x50;

const HOST_ADDR: u8 = 0x51;
const DEST_WRITE: u8 = 0x6E;
const REPLY_SEED: u8 = 0x50;

const OP_GET_VCP: u8 = 0x01;
const OP_GET_VCP_REPLY: u8 = 0x02;
const OP_SET_VCP: u8 = 0x03;
const OP_CAPS_REPLY: u8 = 0xE3;
const OP_CAPS_REQUEST: u8 = 0xF3;

/// Bytes to read for a Get VCP Feature reply.
pub const VCP_REPLY_LEN: usize = 11;
/// Bytes to read for a capabilities fragment (up to 32 data bytes).
pub const CAPS_REPLY_LEN: usize = 38;

pub fn checksum(seed: u8, bytes: &[u8]) -> u8 {
    bytes.iter().fold(seed, |acc, b| acc ^ b)
}

/// Wraps a payload into a host -> monitor message.
pub fn encode(payload: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(payload.len() + 3);
    msg.push(HOST_ADDR);
    msg.push(0x80 | payload.len() as u8);
    msg.extend_from_slice(payload);
    msg.push(checksum(DEST_WRITE, &msg));
    msg
}

pub fn get_vcp_request(code: u8) -> Vec<u8> {
    encode(&[OP_GET_VCP, code])
}

pub fn set_vcp_request(code: u8, value: u16) -> Vec<u8> {
    let [hi, lo] = value.to_be_bytes();
    encode(&[OP_SET_VCP, code, hi, lo])
}

pub fn capabilities_request(offset: u16) -> Vec<u8> {
    let [hi, lo] = offset.to_be_bytes();
    encode(&[OP_CAPS_REQUEST, hi, lo])
}

/// Value of a VCP feature as returned by the monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VcpValue {
    pub code: u8,
    /// 0 = set parameter, 1 = momentary.
    pub kind: u8,
    pub max: u16,
    pub current: u16,
}

impl VcpValue {
    /// The four raw value bytes `MH ML SH SL`; some Samsung codes pack fields into them.
    pub fn bytes(&self) -> [u8; 4] {
        let [mh, ml] = self.max.to_be_bytes();
        let [sh, sl] = self.current.to_be_bytes();
        [mh, ml, sh, sl]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplyError {
    #[error("monitor sent a null message (busy)")]
    Null,
    #[error("reply too short or with a bad length byte")]
    BadLength,
    #[error("reply checksum mismatch")]
    BadChecksum,
    #[error("unexpected reply opcode {0:#04x}")]
    UnexpectedOpcode(u8),
    #[error("VCP code {0:#04x} is not supported by the monitor")]
    Unsupported(u8),
    #[error("reply is for VCP code {got:#04x}, expected {expected:#04x}")]
    WrongCode { expected: u8, got: u8 },
    #[error("capabilities reply for offset {got}, expected {expected}")]
    WrongOffset { expected: u16, got: u16 },
}

/// Validates a reply frame and returns its payload (opcode included).
pub fn parse_reply(buf: &[u8]) -> Result<&[u8], ReplyError> {
    if buf.len() < 3 || buf[0] != DEST_WRITE || buf[1] & 0x80 == 0 {
        return Err(ReplyError::BadLength);
    }
    let len = (buf[1] & 0x7F) as usize;
    if buf.len() < len + 3 {
        return Err(ReplyError::BadLength);
    }
    if checksum(REPLY_SEED, &buf[..len + 2]) != buf[len + 2] {
        return Err(ReplyError::BadChecksum);
    }
    if len == 0 {
        return Err(ReplyError::Null);
    }
    Ok(&buf[2..len + 2])
}

pub fn parse_vcp_reply(buf: &[u8], expected: u8) -> Result<VcpValue, ReplyError> {
    let p = parse_reply(buf)?;
    if p[0] != OP_GET_VCP_REPLY {
        return Err(ReplyError::UnexpectedOpcode(p[0]));
    }
    if p.len() != 8 {
        return Err(ReplyError::BadLength);
    }
    if p[2] != expected {
        return Err(ReplyError::WrongCode {
            expected,
            got: p[2],
        });
    }
    if p[1] != 0 {
        return Err(ReplyError::Unsupported(expected));
    }
    Ok(VcpValue {
        code: p[2],
        kind: p[3],
        max: u16::from_be_bytes([p[4], p[5]]),
        current: u16::from_be_bytes([p[6], p[7]]),
    })
}

/// Returns the data bytes of a capabilities fragment (empty = end of string).
pub fn parse_capabilities_reply(buf: &[u8], expected: u16) -> Result<&[u8], ReplyError> {
    let p = parse_reply(buf)?;
    if p[0] != OP_CAPS_REPLY {
        return Err(ReplyError::UnexpectedOpcode(p[0]));
    }
    if p.len() < 3 {
        return Err(ReplyError::BadLength);
    }
    let got = u16::from_be_bytes([p[1], p[2]]);
    if got != expected {
        return Err(ReplyError::WrongOffset { expected, got });
    }
    Ok(&p[3..])
}

#[cfg(test)]
mod tests {
    use super::*;

    // Byte sequences below were captured on a real G60SD (fw 1003.0 / 1006.0).

    #[test]
    fn get_request_bytes() {
        assert_eq!(get_vcp_request(0x10), [0x51, 0x82, 0x01, 0x10, 0xAC]);
        assert_eq!(get_vcp_request(0x2F), [0x51, 0x82, 0x01, 0x2F, 0x93]);
        assert_eq!(get_vcp_request(0xEB), [0x51, 0x82, 0x01, 0xEB, 0x57]);
    }

    #[test]
    fn set_request_bytes() {
        assert_eq!(
            set_vcp_request(0x2F, 6),
            [0x51, 0x84, 0x03, 0x2F, 0x00, 0x06, 0x91]
        );
        assert_eq!(
            set_vcp_request(0x10, 45),
            [0x51, 0x84, 0x03, 0x10, 0x00, 0x2D, 0x85]
        );
        assert_eq!(
            set_vcp_request(0xEB, 0xA1),
            [0x51, 0x84, 0x03, 0xEB, 0x00, 0xA1, 0xF2]
        );
    }

    #[test]
    fn capabilities_request_bytes() {
        // 51 83 F3 00 00, checksum 6E^51^83^F3 = 0x4F
        assert_eq!(
            capabilities_request(0),
            [0x51, 0x83, 0xF3, 0x00, 0x00, 0x4F]
        );
    }

    #[test]
    fn parses_vcp_reply() {
        let r = [
            0x6E, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x32, 0x00, 0x32, 0xA4,
        ];
        let v = parse_vcp_reply(&r, 0x10).unwrap();
        assert_eq!((v.code, v.kind, v.max, v.current), (0x10, 0, 50, 50));
        let r = [
            0x6E, 0x88, 0x02, 0x00, 0xEB, 0x00, 0x00, 0xB1, 0x00, 0xA1, 0x4F,
        ];
        assert_eq!(parse_vcp_reply(&r, 0xEB).unwrap().current, 0xA1);
    }

    #[test]
    fn rejects_bad_vcp_replies() {
        let mut r = [
            0x6E, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x32, 0x00, 0x32, 0xA4,
        ];
        assert_eq!(
            parse_vcp_reply(&r, 0x12),
            Err(ReplyError::WrongCode {
                expected: 0x12,
                got: 0x10
            })
        );
        r[9] = 0x31;
        assert_eq!(parse_vcp_reply(&r, 0x10), Err(ReplyError::BadChecksum));
        assert_eq!(
            parse_vcp_reply(&[0x6E, 0x80, 0xBE], 0x10),
            Err(ReplyError::Null)
        );
        assert_eq!(
            parse_vcp_reply(&[0xFF; 11], 0x10),
            Err(ReplyError::BadLength)
        );
    }

    #[test]
    fn unsupported_code_result() {
        let mut r = vec![0x6E, 0x88, 0x02, 0x01, 0xE7, 0x00, 0x00, 0x00, 0x00, 0x00];
        r.push(checksum(0x50, &r));
        assert_eq!(
            parse_vcp_reply(&r, 0xE7),
            Err(ReplyError::Unsupported(0xE7))
        );
    }

    #[test]
    fn parses_capabilities_fragment() {
        let mut r = vec![0x6E, 0x80 | 8, 0xE3, 0x00, 0x20];
        r.extend_from_slice(b"(prot");
        r.push(checksum(0x50, &r));
        assert_eq!(parse_capabilities_reply(&r, 0x20).unwrap(), b"(prot");
        assert_eq!(
            parse_capabilities_reply(&r, 0),
            Err(ReplyError::WrongOffset {
                expected: 0,
                got: 0x20
            })
        );
    }

    #[test]
    fn value_bytes() {
        let v = VcpValue {
            code: 0xE3,
            kind: 0,
            max: 0x5151,
            current: 0x0102,
        };
        assert_eq!(v.bytes(), [0x51, 0x51, 0x01, 0x02]);
    }
}
