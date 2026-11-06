//! MCCS capability string parser.
//!
//! Samsung firmware does not close `vcp(` in its SEC/LEMON strings (the remaining fields end up
//! nested inside it), so fields are located by name instead of by balanced parentheses.

use std::collections::BTreeMap;

/// Parsed capability string, e.g. `(prot(monitor)type(lcd)model(SEC)vcp(10 14(05 08) 60(11 12 0F))…)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub raw: String,
    pub model: Option<String>,
    pub mccs_version: Option<String>,
    /// VCP codes with the values listed for them (empty when none are listed).
    pub vcp: BTreeMap<u8, Vec<u8>>,
}

/// Samsung firmware profile, recognised from the `model(…)` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profile {
    /// Extended profile used by Samsung Display Manager (e.g. `LEMON24v1`), enabled with VCP EB=A1.
    Lemon(String),
    /// Retail profile of recent firmware.
    Sec,
    /// Profile of older firmware (G60SD 1003.0) with a restricted write list.
    Falcon,
    Other(String),
}

impl Capabilities {
    pub fn parse(raw: &str) -> Self {
        let raw = raw
            .trim_matches(|c: char| c == '\0' || c.is_whitespace())
            .to_string();
        Capabilities {
            model: field(&raw, "model").map(|s| s.trim().to_string()),
            mccs_version: field(&raw, "mccs_ver").map(|s| s.trim().to_string()),
            vcp: field_start(&raw, "vcp")
                .map(parse_vcp_list)
                .unwrap_or_default(),
            raw,
        }
    }

    pub fn supports(&self, code: u8) -> bool {
        self.vcp.contains_key(&code)
    }

    pub fn profile(&self) -> Profile {
        match self.model.as_deref() {
            Some(m) if m.starts_with("LEMON") => Profile::Lemon(m.to_string()),
            Some("SEC") => Profile::Sec,
            Some("FALCON") => Profile::Falcon,
            Some(m) => Profile::Other(m.to_string()),
            None => Profile::Other(String::new()),
        }
    }
}

/// Text right after `key(`, where `key` starts a field (preceded by `(`, `)` or nothing).
fn field_start<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("{key}(");
    let mut from = 0;
    while let Some(pos) = s[from..].find(&pat) {
        let at = from + pos;
        if at == 0 || matches!(s.as_bytes()[at - 1], b'(' | b')') {
            return Some(&s[at + pat.len()..]);
        }
        from = at + pat.len();
    }
    None
}

/// Value of a non-nested field such as `model(SEC)`.
fn field<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let rest = field_start(s, key)?;
    Some(&rest[..rest.find(')')?])
}

/// Parses `02 04 14(05 08 0B) 60(11 12 0F ) …` into code -> values, stopping at the closing `)`
/// or at the first word that is not a hex byte (e.g. `mswhql`).
fn parse_vcp_list(s: &str) -> BTreeMap<u8, Vec<u8>> {
    let mut map = BTreeMap::new();
    let mut last = None;
    let mut rest = s;
    loop {
        rest = rest.trim_start();
        match rest.chars().next() {
            None | Some(')') => break,
            Some('(') => {
                let end = rest.find(')').unwrap_or(rest.len());
                if let Some(code) = last {
                    let values = rest[1..end]
                        .split_whitespace()
                        .filter_map(hex_byte)
                        .collect();
                    map.insert(code, values);
                }
                rest = rest.get(end + 1..).unwrap_or("");
            }
            Some(_) => {
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    .unwrap_or(rest.len());
                let Some(code) = hex_byte(&rest[..end]) else {
                    break;
                };
                map.entry(code).or_insert_with(Vec::new);
                last = Some(code);
                rest = &rest[end..];
            }
        }
    }
    map
}

fn hex_byte(s: &str) -> Option<u8> {
    if s.len() == 2 {
        u8::from_str_radix(s, 16).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::{LEMON_CAPS as LEMON, SEC_CAPS as SEC};

    // Read from a real G60SD on firmware 1003.0 (SEC and LEMON come from 1006.0).
    const FALCON: &str = "(prot(monitor)type(lcd)model(FALCON)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(05 08 0B 0C) 16 18 1A 52 60(01 03 04 11 12 0F 10) 62 8D FF)mswhql(1)mccs_ver(2.0)asset_eep(32)mpu_ver(01))";

    #[test]
    fn parses_falcon() {
        let c = Capabilities::parse(FALCON);
        assert_eq!(c.profile(), Profile::Falcon);
        assert_eq!(c.mccs_version.as_deref(), Some("2.0"));
        assert_eq!(c.vcp[&0x14], [0x05, 0x08, 0x0B, 0x0C]);
        assert_eq!(c.vcp[&0x60], [0x01, 0x03, 0x04, 0x11, 0x12, 0x0F, 0x10]);
        assert!(c.supports(0xFF) && !c.supports(0x2F));
        assert_eq!(c.vcp.len(), 15);
    }

    #[test]
    fn parses_sec_with_unclosed_vcp() {
        let c = Capabilities::parse(SEC);
        assert_eq!(c.profile(), Profile::Sec);
        assert_eq!(c.mccs_version.as_deref(), Some("2.0"));
        assert_eq!(c.vcp[&0x60], [0x11, 0x12, 0x0F]);
        assert_eq!(c.vcp[&0xCC], Vec::<u8>::new());
        assert_eq!(c.vcp.len(), 16);
    }

    #[test]
    fn parses_lemon() {
        let c = Capabilities::parse(LEMON);
        assert_eq!(c.profile(), Profile::Lemon("LEMON24v1".into()));
        for code in [0x0A, 0x2D, 0x2F, 0xE0, 0xEB, 0xEF, 0xF2, 0xFA, 0xFC] {
            assert!(c.supports(code), "{code:#04x}");
        }
        assert_eq!(c.vcp.len(), 33);
    }

    #[test]
    fn tolerates_garbage() {
        let c = Capabilities::parse("\0\0garbage(");
        assert_eq!(c.model, None);
        assert!(c.vcp.is_empty());
        assert_eq!(
            Capabilities::parse("").profile(),
            Profile::Other(String::new())
        );
    }
}
