//! Minimal EDID base block parser: just what is needed to identify a monitor.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edid {
    /// Three-letter PNP id, e.g. `SAM`.
    pub manufacturer: String,
    pub product_code: u16,
    pub serial_number: u32,
    /// Display product name descriptor (0xFC), e.g. `Odyssey G60SD`.
    pub name: Option<String>,
    /// Display serial string descriptor (0xFF).
    pub serial: Option<String>,
}

const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

impl Edid {
    /// Parses the first 128 bytes. Returns `None` when the header is missing.
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 128 || data[..8] != HEADER {
            return None;
        }
        let id = u16::from_be_bytes([data[8], data[9]]);
        let letter = |shift: u16| (b'@' + ((id >> shift) & 0x1F) as u8) as char;
        let mut edid = Edid {
            manufacturer: [letter(10), letter(5), letter(0)].iter().collect(),
            product_code: u16::from_le_bytes([data[10], data[11]]),
            serial_number: u32::from_le_bytes([data[12], data[13], data[14], data[15]]),
            name: None,
            serial: None,
        };
        for d in data[54..126].chunks(18) {
            if d[..3] != [0, 0, 0] {
                continue; // detailed timing descriptor
            }
            let text = descriptor_text(&d[5..18]);
            match d[3] {
                0xFC => edid.name = Some(text),
                0xFF => edid.serial = Some(text),
                _ => {}
            }
        }
        Some(edid)
    }

    pub fn is_samsung(&self) -> bool {
        self.manufacturer == "SAM"
    }
}

fn descriptor_text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0x0A).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end])
        .trim_end()
        .to_string()
}

#[cfg(test)]
pub(crate) fn test_edid(name: &str, serial: &str) -> Vec<u8> {
    let mut e = vec![0u8; 128];
    e[..8].copy_from_slice(&HEADER);
    e[8..10].copy_from_slice(&[0x4C, 0x2D]); // SAM
    e[10..12].copy_from_slice(&0x75C3u16.to_le_bytes());
    e[12..16].copy_from_slice(&1234u32.to_le_bytes());
    e[54] = 0x14; // first descriptor is a detailed timing
    for (slot, tag, text) in [(1, 0xFC, name), (2, 0xFF, serial)] {
        let d = &mut e[54 + slot * 18..54 + (slot + 1) * 18];
        d[3] = tag;
        let mut t = [0x20u8; 13];
        t[..text.len()].copy_from_slice(text.as_bytes());
        if text.len() < 13 {
            t[text.len()] = 0x0A;
        }
        d[5..].copy_from_slice(&t);
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_samsung_edid() {
        let e = Edid::parse(&test_edid("Odyssey G60SD", "TEST000001")).unwrap();
        assert_eq!(e.manufacturer, "SAM");
        assert!(e.is_samsung());
        assert_eq!(e.product_code, 0x75C3);
        assert_eq!(e.serial_number, 1234);
        assert_eq!(e.name.as_deref(), Some("Odyssey G60SD"));
        assert_eq!(e.serial.as_deref(), Some("TEST000001"));
    }

    #[test]
    fn rejects_non_edid() {
        assert_eq!(Edid::parse(&[0u8; 128]), None);
        assert_eq!(Edid::parse(&[0u8; 10]), None);
    }
}
