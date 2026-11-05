//! Presets: named sets of feature values, applied by writing only what differs from the monitor.

use std::collections::BTreeMap;

use crate::actions;
use crate::ddc::VcpValue;
use crate::device::Vcp;
use crate::features::{self, Feature, is_disabled};

/// Feature name -> value.
pub type Values = BTreeMap<String, u16>;

/// Features a preset can hold, in the order they are applied. The picture mode comes first because
/// it resets the per-mode values. Input and OSD language are left out on purpose.
const ORDER: &[&str] = &[
    "picture-mode",
    "color-tone",
    "red-gain",
    "green-gain",
    "blue-gain",
    "brightness",
    "contrast",
    "sharpness",
    "black-equalizer",
    "aim-point",
    "lighting",
    "lighting-effect",
    "lighting-color",
    "volume",
];

/// Eye Saver locks brightness, colour and picture mode: switched off before them, on after them.
const EYE_SAVER: &str = "eye-saver";

/// The value a feature is compared and stored with. For the lighting only bit 0 is the on/off
/// switch (bits 1-2 are CoreSync).
fn comparable(f: &Feature, raw: u16) -> u16 {
    if f.name == "lighting" { raw & 1 } else { raw }
}

/// Current values of the features a preset can hold and `writable` allows.
pub fn capture(current: &BTreeMap<u8, VcpValue>, writable: impl Fn(&Feature) -> bool) -> Values {
    ORDER
        .iter()
        .chain([&EYE_SAVER])
        .filter_map(|name| features::find(name))
        .filter(|f| !f.read_only && writable(f))
        .filter_map(|f| {
            let v = current.get(&f.code)?;
            (!is_disabled(v.current, v.max)).then(|| (f.name.to_string(), comparable(f, v.current)))
        })
        .collect()
}

/// The features of `target` in the order they must be applied.
pub fn order(target: &Values) -> Vec<(&'static Feature, u16)> {
    let eye = target.get(EYE_SAVER).copied();
    let mut names: Vec<&str> = Vec::new();
    if eye == Some(0) {
        names.push(EYE_SAVER);
    }
    names.extend(ORDER);
    if eye.is_some_and(|v| v != 0) {
        names.push(EYE_SAVER);
    }
    names
        .into_iter()
        .filter_map(|n| Some((features::find(n)?, *target.get(n)?)))
        .collect()
}

/// True when every value of `target` matches `current`.
pub fn matches(target: &Values, current: &BTreeMap<u8, VcpValue>) -> bool {
    !target.is_empty()
        && order(target)
            .iter()
            .all(|(f, t)| current.get(&f.code).is_some_and(|v| comparable(f, v.current) == *t))
}

#[derive(Debug, Default)]
pub struct Report {
    pub changed: Vec<&'static Feature>,
    /// Not available right now (e.g. the colour with an effect that has none).
    pub skipped: Vec<&'static Feature>,
    pub failed: Vec<(&'static Feature, String)>,
}

/// Applies `target`, reading each feature right before deciding whether to write it, because the
/// picture mode and Eye Saver change other values. Locked features are skipped.
pub fn apply(m: &mut dyn Vcp, target: &Values, allow_unverified: bool) -> Report {
    let mut report = Report::default();
    for (f, t) in order(target) {
        if !f.status.safe_to_write() && !allow_unverified {
            report.skipped.push(f);
            continue;
        }
        let now = match m.get_vcp(f.code) {
            Ok(v) => v,
            Err(e) => {
                report.failed.push((f, e.to_string()));
                continue;
            }
        };
        if is_disabled(now.current, now.max) {
            report.skipped.push(f);
        } else if comparable(f, now.current) != t {
            match actions::write_from(m, f, now, t) {
                Ok(_) => report.changed.push(f),
                Err(e) => report.failed.push((f, e.to_string())),
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::DemoDevice;

    fn readings(d: &mut DemoDevice) -> BTreeMap<u8, VcpValue> {
        features::FEATURES.iter().filter_map(|f| Some((f.code, d.get_vcp(f.code).ok()?))).collect()
    }

    #[test]
    fn capture_skips_locked_disabled_and_excluded() {
        let mut d = DemoDevice::new();
        let values = capture(&readings(&mut d), |f| f.status.safe_to_write());
        assert_eq!(values["brightness"], 50);
        assert_eq!(values["lighting"], 1);
        assert!(!values.contains_key("picture-mode")); // not writable without the unverified switch
        assert!(!values.contains_key("lighting-color")); // 0xFD in the snapshot
        assert!(!values.contains_key("input") && !values.contains_key("language"));
        let all = capture(&readings(&mut d), |_| true);
        assert_eq!(all["picture-mode"], 8);
    }

    #[test]
    fn eye_saver_goes_first_when_off_and_last_when_on() {
        let mut t = Values::new();
        t.insert("brightness".into(), 30);
        t.insert("picture-mode".into(), 3);
        t.insert("eye-saver".into(), 0);
        let names: Vec<_> = order(&t).iter().map(|(f, _)| f.name).collect();
        assert_eq!(names, ["eye-saver", "picture-mode", "brightness"]);
        t.insert("eye-saver".into(), 2);
        let names: Vec<_> = order(&t).iter().map(|(f, _)| f.name).collect();
        assert_eq!(names, ["picture-mode", "brightness", "eye-saver"]);
    }

    #[test]
    fn apply_writes_only_differences_and_restores() {
        let mut d = DemoDevice::new();
        let saved = capture(&readings(&mut d), |f| f.status.safe_to_write());
        assert!(matches(&saved, &readings(&mut d)));
        d.set_vcp(0x10, 20).unwrap();
        d.set_vcp(0x2F, 2).unwrap();
        assert!(!matches(&saved, &readings(&mut d)));
        let report = apply(&mut d, &saved, false);
        let changed: Vec<_> = report.changed.iter().map(|f| f.name).collect();
        assert_eq!(changed, ["brightness", "black-equalizer"]);
        assert!(report.failed.is_empty());
        assert!(matches(&saved, &readings(&mut d)));
    }
}
