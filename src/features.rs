//! Known features of the Samsung Odyssey G60SD (firmware 1006.0) and how far each one is verified.
//!
//! Samsung-specific codes come from reverse engineering Samsung Display Manager
//! (see docs/PROTOCOL.md); `Status` records what has been confirmed on real hardware.

/// How much of a feature has been confirmed on real hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// Code and values taken from Samsung Display Manager, never tested.
    Unverified,
    /// Reading matches the OSD; writing not tested yet.
    ReadVerified,
    /// Standard MCCS code advertised in the capability string.
    Standard,
    /// Writing tested and confirmed on the OSD.
    Verified,
}

impl Status {
    /// Writing needs an explicit opt-in below this level.
    pub fn safe_to_write(self) -> bool {
        self >= Status::Standard
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Unverified => "unverified",
            Status::ReadVerified => "read-verified",
            Status::Standard => "standard",
            Status::Verified => "verified",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feature {
    pub name: &'static str,
    pub code: u8,
    pub label: &'static str,
    pub status: Status,
    /// Named values, when the feature is a list.
    pub values: &'static [(u16, &'static str)],
    /// Packed or not yet understood value: shown, never written.
    pub read_only: bool,
}

/// Value the monitor reports for a feature that is unavailable in its current state
/// (e.g. Black Equalizer while Eye Saver is on).
pub const DISABLED: u16 = 0xFD;

/// True when a reading means "disabled right now" rather than a value.
pub fn is_disabled(current: u16, max: u16) -> bool {
    current == DISABLED && max < DISABLED
}

/// Like [`is_disabled`], knowing the feature: Adaptive-Sync packs "disabled" into its low nibble.
pub fn disabled(f: &Feature, current: u16, max: u16) -> bool {
    is_disabled(current, max) || (f.code == 0xF2 && current & 0xF == 0xD)
}

/// The part of a raw reading that is the feature's value: what a write sets and a preset stores.
/// Adaptive-Sync keeps its type in the high nibble, the lighting its CoreSync bits above bit 0.
pub fn state(f: &Feature, raw: u16) -> u16 {
    match f.code {
        0xF2 => raw & 0xF,
        0xF8 => raw & 1,
        _ => raw,
    }
}

/// Operations that are not a single feature value (see `aim`, `pip`, `actions`), with the same
/// verification record as features so the CLI and GUI can gate them alike.
pub const ACTIONS: &[(&str, Status, &str)] = &[
    (
        "aim-move",
        Status::Verified,
        "Virtual Aim Point: move one step (EE)",
    ),
    (
        "aim-position",
        Status::Verified,
        "Virtual Aim Point: position (EF)",
    ),
    (
        "pip",
        Status::Verified,
        "PIP/PBP: on, off, mode, size, position (E2)",
    ),
    (
        "pip-source",
        Status::Verified,
        "PIP/PBP: input of each window (E3)",
    ),
    (
        "coresync",
        Status::Verified,
        "CoreSync: lighting follows the screen (F8)",
    ),
    (
        "white-balance-reset",
        Status::Unverified,
        "Reset white balance (E0, LEMON profile; no effect on the G60SD)",
    ),
];

pub fn action_status(name: &str) -> Option<Status> {
    ACTIONS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|&(_, s, _)| s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    Off,
    On,
    /// Not available in the current state.
    Disabled,
}

/// Adaptive-Sync / FreeSync reading (VCP F2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveSync {
    pub state: SyncState,
    /// The monitor presents the feature as FreeSync rather than Adaptive-Sync.
    pub freesync: bool,
}

/// F2: low nibble 0 off, 1 on, D disabled; high nibble 1 = FreeSync.
pub fn decode_adaptive_sync(v: u16) -> AdaptiveSync {
    let state = match v & 0xF {
        1 => SyncState::On,
        0xD => SyncState::Disabled,
        _ => SyncState::Off,
    };
    AdaptiveSync {
        state,
        freesync: v & 0xF0 == 0x10,
    }
}

/// PIP/PBP status reading (VCP E2), the fields odyssey-ctl shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipStatus {
    pub on: bool,
    pub pip_supported: bool,
    pub pbp_supported: bool,
}

pub fn decode_pip_status(v: u16) -> PipStatus {
    PipStatus {
        on: v & 1 != 0,
        pip_supported: v & 0x1000 != 0,
        pbp_supported: v & 0x2000 != 0,
    }
}

/// Name of a PIP/PBP source code (`type << 4 | n`, as in VCP E3).
pub fn source_name(code: u8) -> Option<String> {
    let kind = match code >> 4 {
        0 => "HDMI",
        1 => "DP",
        2 => "USB-C",
        3 => "Thunderbolt",
        4 => "DVI",
        5 => "D-SUB",
        _ => return None,
    };
    Some(format!("{kind} {}", (code & 0xF) + 1))
}

impl Feature {
    pub fn value_name(&self, value: u16) -> Option<&'static str> {
        self.values
            .iter()
            .find(|(v, _)| *v == value)
            .map(|(_, n)| *n)
    }

    /// Accepts a number (decimal or 0x hex) or one of the value names.
    pub fn parse_value(&self, s: &str) -> Option<u16> {
        parse_number(s).or_else(|| {
            self.values
                .iter()
                .find(|(_, n)| n.eq_ignore_ascii_case(s))
                .map(|(v, _)| *v)
        })
    }
}

/// `+N` / `-N`: a change relative to the current value (a bare number is absolute).
pub fn parse_relative(s: &str) -> Option<i32> {
    let (sign, digits) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    digits.parse::<u16>().ok().map(|n| sign * i32::from(n))
}

pub fn parse_number(s: &str) -> Option<u16> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u16::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

const PICTURE_MODES: &[(u16, &str)] = &[
    (0, "entertain"),
    (1, "graphic"),
    (2, "eco"),
    (3, "game-standard"),
    (4, "rpg"),
    (5, "rts"),
    (6, "fps"),
    (7, "sports"),
    (8, "original"),
    (9, "custom"),
];

const INPUTS: &[(u16, &str)] = &[(0x0F, "displayport"), (0x11, "hdmi1"), (0x12, "hdmi2")];

// Value names below come from Samsung Display Manager's Swift enums (docs/PROTOCOL.md).
const COLOR_TONES: &[(u16, &str)] = &[
    (0, "cool"),
    (1, "standard"),
    (2, "warm1"),
    (3, "warm2"),
    (4, "natural"),
];
const EYE_SAVER: &[(u16, &str)] = &[(0, "off"), (1, "low"), (2, "high")];
const OFF_ON: &[(u16, &str)] = &[(0, "off"), (1, "on")];
const PC_AV: &[(u16, &str)] = &[(0, "pc"), (1, "av")];
// Wire values on the G60SD (fw 1006.0, verified on screen): red and blue are swapped compared with
// SDM's enum order, as SDM itself does for LEMON23v3 firmware.
const AIM_POINT: &[(u16, &str)] = &[
    (0, "off"),
    (1, "a-blue"),
    (2, "a-red"),
    (3, "a-white"),
    (4, "b-blue"),
    (5, "b-red"),
    (6, "b-white"),
    (7, "reset-position"),
];
const LIGHTING_EFFECTS: &[(u16, &str)] = &[
    (0, "aurora"),
    (1, "aurora-rotation"),
    (2, "comet"),
    (3, "static"),
    (4, "rainbow"),
    (5, "breathing"),
];
const PIP_SOUND: &[(u16, &str)] = &[(0, "main"), (1, "channel1"), (2, "channel2")];
const LANGUAGES: &[(u16, &str)] = &[
    (0x01, "chinese"),
    (0x02, "english"),
    (0x03, "french"),
    (0x04, "german"),
    (0x05, "italian"),
    (0x06, "japanese"),
    (0x07, "korean"),
    (0x08, "portuguese"),
    (0x09, "russian"),
    (0x0A, "spanish"),
    (0x0B, "swedish"),
    (0x0C, "turkish"),
    (0x1A, "hungarian"),
    (0x1E, "polish"),
    (0x24, "ukrainian"),
    (0x26, "kyrgyz"),
];

const fn feature(
    name: &'static str,
    code: u8,
    label: &'static str,
    status: Status,
    values: &'static [(u16, &'static str)],
) -> Feature {
    Feature {
        name,
        code,
        label,
        status,
        values,
        read_only: false,
    }
}

const fn read_only(name: &'static str, code: u8, label: &'static str) -> Feature {
    Feature {
        name,
        code,
        label,
        status: Status::Unverified,
        values: &[],
        read_only: true,
    }
}

pub const FEATURES: &[Feature] = &[
    feature("brightness", 0x10, "Brightness", Status::Verified, &[]),
    feature("contrast", 0x12, "Contrast", Status::Standard, &[]),
    feature("sharpness", 0x87, "Sharpness", Status::Standard, &[]),
    feature(
        "red-gain",
        0x16,
        "White balance: red",
        Status::Standard,
        &[],
    ),
    feature(
        "green-gain",
        0x18,
        "White balance: green",
        Status::Standard,
        &[],
    ),
    feature(
        "blue-gain",
        0x1A,
        "White balance: blue",
        Status::Standard,
        &[],
    ),
    feature(
        "picture-mode",
        0x2D,
        "Picture mode (PC mode values)",
        Status::Verified,
        PICTURE_MODES,
    ),
    feature(
        "black-equalizer",
        0x2F,
        "Black Equalizer",
        Status::Verified,
        &[],
    ),
    feature(
        "color-tone",
        0x14,
        "Color tone",
        Status::Verified,
        COLOR_TONES,
    ),
    feature(
        "eye-saver",
        0x0A,
        "Eye Saver Mode",
        Status::Verified,
        EYE_SAVER,
    ),
    feature("input", 0x60, "Input source", Status::Standard, INPUTS),
    feature("volume", 0x62, "Volume", Status::Standard, &[]),
    read_only("mute", 0x8D, "Audio mute"),
    feature(
        "language",
        0xCC,
        "OSD language",
        Status::Standard,
        LANGUAGES,
    ),
    // F2 packs status (low nibble: 0 off, 1 on, D disabled) and type (high nibble 1 = FreeSync);
    // only the status is written (see `state`).
    feature(
        "adaptive-sync",
        0xF2,
        "Adaptive-Sync / FreeSync",
        Status::Verified,
        OFF_ON,
    ),
    feature(
        "aim-point",
        0xED,
        "Virtual Aim Point",
        Status::Verified,
        AIM_POINT,
    ),
    // F8 bit 0 = lighting, bits 1-2 = CoreSync; only the lighting bit is written.
    feature(
        "lighting",
        0xF8,
        "Infinity Core Lighting",
        Status::Verified,
        OFF_ON,
    ),
    feature(
        "lighting-effect",
        0xF9,
        "Lighting effect",
        Status::Verified,
        LIGHTING_EFFECTS,
    ),
    // Colour index: writes work, but SDM's colour names did not match what was seen (7 "blue" looked whitish).
    feature(
        "lighting-color",
        0xFA,
        "Lighting color",
        Status::Verified,
        &[],
    ),
    read_only("pip", 0xE2, "PIP/PBP status"),
    read_only("pip-source", 0xE3, "PIP/PBP sources"),
    feature(
        "pip-sound",
        0xE8,
        "PIP/PBP sound source",
        Status::Unverified,
        PIP_SOUND,
    ),
    feature("pc-av-mode", 0xE4, "PC/AV mode", Status::Unverified, PC_AV),
];

/// Features whose value or availability changes when `name` is written, so they must be read again.
/// Picture mode resets the per-mode values; Eye Saver locks picture settings (verified on the G60SD);
/// lighting and effect decide whether a colour applies (verified).
pub fn dependents(name: &str) -> &'static [&'static str] {
    const PICTURE: &[&str] = &[
        "picture-mode",
        "brightness",
        "contrast",
        "sharpness",
        "color-tone",
        "red-gain",
        "green-gain",
        "blue-gain",
        "black-equalizer",
        "eye-saver",
    ];
    match name {
        "picture-mode" | "eye-saver" | "pc-av-mode" => PICTURE,
        "lighting" | "lighting-effect" => &["lighting", "lighting-effect", "lighting-color"],
        _ => &[],
    }
}

pub fn find(name: &str) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.name.eq_ignore_ascii_case(name))
}

pub fn by_code(code: u8) -> Option<&'static Feature> {
    FEATURES.iter().find(|f| f.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_codes_are_unique() {
        for (i, a) in FEATURES.iter().enumerate() {
            for b in &FEATURES[i + 1..] {
                assert_ne!(a.name, b.name);
                assert_ne!(a.code, b.code, "{} / {}", a.name, b.name);
            }
        }
    }

    #[test]
    fn lookup_and_values() {
        let pm = find("Picture-Mode").unwrap();
        assert_eq!(pm.code, 0x2D);
        assert_eq!(pm.value_name(8), Some("original"));
        assert_eq!(pm.parse_value("FPS"), Some(6));
        assert_eq!(pm.parse_value("0x09"), Some(9));
        assert_eq!(pm.parse_value("nope"), None);
        assert_eq!(parse_relative("+5"), Some(5));
        assert_eq!(parse_relative("-12"), Some(-12));
        assert_eq!(parse_relative("5"), None);
        assert_eq!(parse_relative("+"), None);
        assert_eq!(by_code(0x2F).unwrap().name, "black-equalizer");
    }

    #[test]
    fn corrected_codes() {
        // E0 is White Balance Reset (LEMON) / a colour-tone alias (SEC): never the aim point.
        assert_eq!(find("aim-point").unwrap().code, 0xED);
        assert!(by_code(0xE0).is_none());
        assert_eq!(find("language").unwrap().value_name(5), Some("italian"));
        assert!(find("pip-source").unwrap().read_only);
        assert!(is_disabled(0xFD, 0x2F) && !is_disabled(0xFD, 0xFF) && !is_disabled(6, 10));
    }

    #[test]
    fn packed_state_and_disabled() {
        let sync = find("adaptive-sync").unwrap();
        assert_eq!(state(sync, 0x10), 0);
        assert_eq!(state(sync, 0x11), 1);
        assert!(disabled(sync, 0x1D, 1) && !disabled(sync, 0x11, 1));
        let lighting = find("lighting").unwrap();
        assert_eq!(state(lighting, 0x03), 1);
        assert_eq!(state(find("brightness").unwrap(), 42), 42);
        assert_eq!(action_status("pip"), Some(Status::Verified));
        assert_eq!(
            action_status("white-balance-reset"),
            Some(Status::Unverified)
        );
        assert_eq!(action_status("nope"), None);
    }

    #[test]
    fn dependents_are_known_features() {
        for f in FEATURES {
            for d in dependents(f.name) {
                assert!(find(d).is_some(), "{} -> {d}", f.name);
            }
        }
        assert!(dependents("eye-saver").contains(&"brightness"));
        assert!(dependents("brightness").is_empty());
    }

    #[test]
    fn decodes_packed_readings() {
        // Readings from the real G60SD (fw 1006.0).
        assert_eq!(
            decode_adaptive_sync(0x10),
            AdaptiveSync {
                state: SyncState::Off,
                freesync: true
            }
        );
        assert_eq!(
            decode_adaptive_sync(0x00),
            AdaptiveSync {
                state: SyncState::Off,
                freesync: false
            }
        );
        assert_eq!(decode_adaptive_sync(0x11).state, SyncState::On);
        assert_eq!(decode_adaptive_sync(0x0D).state, SyncState::Disabled);
        let pip = decode_pip_status(0x150E);
        assert_eq!(
            pip,
            PipStatus {
                on: false,
                pip_supported: true,
                pbp_supported: false
            }
        );
        assert_eq!(source_name(0x00).as_deref(), Some("HDMI 1"));
        assert_eq!(source_name(0x11).as_deref(), Some("DP 2"));
        assert_eq!(source_name(0x70), None);
    }

    #[test]
    fn write_policy() {
        assert!(find("brightness").unwrap().status.safe_to_write());
        assert!(find("contrast").unwrap().status.safe_to_write());
        assert!(find("picture-mode").unwrap().status.safe_to_write());
        assert!(find("adaptive-sync").unwrap().status.safe_to_write());
        assert!(!find("adaptive-sync").unwrap().read_only);
        assert!(!find("pc-av-mode").unwrap().status.safe_to_write());
        assert!(find("aim-point").unwrap().status.safe_to_write());
        assert_eq!(find("aim-point").unwrap().value_name(2), Some("a-red"));
    }
}
