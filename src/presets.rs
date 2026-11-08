//! Presets: named sets of feature values, applied by writing only what differs from the monitor.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::actions;
use crate::config::{load_toml, save_toml};
use crate::ddc::VcpValue;
use crate::device::Vcp;
use crate::features::{self, Feature};

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
    // Before the lighting: the monitor hides the lighting while Adaptive-Sync is on.
    "adaptive-sync",
    "lighting",
    "lighting-effect",
    "lighting-color",
    "volume",
];

/// Eye Saver locks brightness, colour and picture mode: switched off before them, on after them.
const EYE_SAVER: &str = "eye-saver";

/// The value a feature is compared and stored with (the lighting and Adaptive-Sync readings
/// carry extra bits).
fn comparable(f: &Feature, raw: u16) -> u16 {
    features::state(f, raw)
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
            (!features::disabled(f, v.current, v.max))
                .then(|| (f.name.to_string(), comparable(f, v.current)))
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
        && order(target).iter().all(|(f, t)| {
            current
                .get(&f.code)
                .is_some_and(|v| comparable(f, v.current) == *t)
        })
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
        if features::disabled(f, now.current, now.max) {
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

// ---- the presets file -------------------------------------------------------

const FILE: &str = "presets.toml";

/// A named preset as stored in `presets.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    /// Feature name -> value, as in `odyssey-ctl features`.
    pub values: Values,
}

/// A saved crosshair position (firmware units, see [`crate::aim`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AimSpot {
    pub name: String,
    pub x: u16,
    pub y: u16,
}

/// A per-application rule: when a window whose class (and title, when given) matches gets the
/// focus, `preset` is applied. Patterns are case-insensitive globs with `*` and `?`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppRule {
    pub class: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    pub preset: String,
}

/// `<config dir>/odyssey-ctl/presets.toml`, shared by the GUI, the tray and the command line.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Presets {
    #[serde(default, rename = "preset")]
    pub list: Vec<Preset>,
    #[serde(default, rename = "aim")]
    pub aim: Vec<AimSpot>,
    /// Checked in order: the first rule that matches wins, so a `*` rule goes last.
    #[serde(default, rename = "app")]
    pub apps: Vec<AppRule>,
    /// Preset applied when the focused window matches no rule (back to normal after a game).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_default: Option<String>,
}

/// Case-insensitive glob: `*` matches any run of characters, `?` one character.
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &t[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    go(&p, &t)
}

impl Presets {
    pub fn load() -> Self {
        load_toml(FILE)
    }

    pub fn save(&self) -> std::io::Result<()> {
        save_toml(FILE, self)
    }

    /// The preset called `name` (case-insensitive).
    pub fn find(&self, name: &str) -> Option<&Preset> {
        self.list.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// The preset of the first rule matching a window's class and title, else `app_default`.
    pub fn preset_for_window(&self, class: &str, title: &str) -> Option<&str> {
        self.apps
            .iter()
            .find(|r| {
                glob_matches(&r.class, class)
                    && (r.title.is_empty() || glob_matches(&r.title, title))
            })
            .map(|r| r.preset.as_str())
            .or(self.app_default.as_deref())
    }

    /// Adds a preset, or replaces the one with the same name (case-insensitive).
    pub fn put(&mut self, preset: Preset) {
        match self
            .list
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&preset.name))
        {
            Some(existing) => *existing = preset,
            None => self.list.push(preset),
        }
    }

    /// Adds a crosshair position, or replaces the one with the same name (case-insensitive).
    pub fn put_aim(&mut self, spot: AimSpot) {
        match self
            .aim
            .iter_mut()
            .find(|a| a.name.eq_ignore_ascii_case(&spot.name))
        {
            Some(existing) => *existing = spot,
            None => self.aim.push(spot),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::DemoDevice;

    fn readings(d: &mut DemoDevice) -> BTreeMap<u8, VcpValue> {
        features::FEATURES
            .iter()
            .filter_map(|f| Some((f.code, d.get_vcp(f.code).ok()?)))
            .collect()
    }

    #[test]
    fn capture_skips_locked_disabled_and_excluded() {
        let mut d = DemoDevice::new();
        let values = capture(&readings(&mut d), |f| f.status.safe_to_write());
        assert_eq!(values["brightness"], 50);
        assert_eq!(values["lighting"], 1);
        assert_eq!(values["picture-mode"], 8); // verified, so presets hold it by default
        assert_eq!(values["adaptive-sync"], 0); // state only, not the raw 0x10
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

    #[test]
    fn toml_round_trip_and_put() {
        let mut p = Presets::default();
        let values: Values = [
            ("brightness".to_string(), 30),
            ("picture-mode".to_string(), 6),
        ]
        .into();
        p.put(Preset {
            name: "FPS".into(),
            values: values.clone(),
        });
        p.put(Preset {
            name: "fps".into(),
            values: [("brightness".to_string(), 40)].into(),
        });
        assert_eq!(p.list.len(), 1);
        assert_eq!(p.list[0].values["brightness"], 40);
        p.put(Preset {
            name: "Notte".into(),
            values,
        });
        p.put_aim(AimSpot {
            name: "Head".into(),
            x: 849,
            y: 200,
        });
        p.put_aim(AimSpot {
            name: "head".into(),
            x: 849,
            y: 210,
        });
        assert_eq!(p.aim.len(), 1);
        assert_eq!(p.aim[0].y, 210);
        assert_eq!(p.find("notte").map(|x| x.name.as_str()), Some("Notte"));
        assert!(p.find("night").is_none());
        p.apps.push(AppRule {
            class: "steam_app_*".into(),
            title: String::new(),
            preset: "FPS".into(),
        });
        p.apps.push(AppRule {
            class: "*".into(),
            title: String::new(),
            preset: "Notte".into(),
        });
        let text = toml::to_string(&p).unwrap();
        assert!(text.contains("[[preset]]") && text.contains("name = \"Notte\""));
        assert!(text.contains("[[aim]]"));
        assert!(text.contains("[[app]]") && !text.contains("title ="));
        assert_eq!(toml::from_str::<Presets>(&text).unwrap(), p);
        // Files from 0.1.0 have no [[aim]] table.
        assert!(
            toml::from_str::<Presets>("[[preset]]\nname = \"a\"\n[preset.values]\n")
                .unwrap()
                .aim
                .is_empty()
        );
    }

    #[test]
    fn globs_and_window_rules() {
        assert!(glob_matches("steam_app_*", "Steam_App_730"));
        assert!(glob_matches("*", ""));
        assert!(glob_matches("cs?", "cs2"));
        assert!(!glob_matches("cs?", "cs"));
        assert!(!glob_matches("firefox", "firefox-nightly"));
        assert!(glob_matches("*fox*", "org.mozilla.firefox"));
        let mut p = Presets::default();
        assert_eq!(p.preset_for_window("cs2", ""), None);
        p.apps = vec![
            AppRule {
                class: "cs2".into(),
                title: "*Counter-Strike*".into(),
                preset: "FPS".into(),
            },
            AppRule {
                class: "*".into(),
                title: String::new(),
                preset: "Desk".into(),
            },
        ];
        assert_eq!(p.preset_for_window("cs2", "Counter-Strike 2"), Some("FPS"));
        assert_eq!(p.preset_for_window("cs2", "Launcher"), Some("Desk"));
        assert_eq!(p.preset_for_window("kate", ""), Some("Desk"));
        p.apps.pop();
        assert_eq!(p.preset_for_window("kate", ""), None);
        p.app_default = Some("Desk".into());
        assert_eq!(p.preset_for_window("kate", ""), Some("Desk"));
        let text = toml::to_string(&p).unwrap();
        assert!(text.contains("app_default = \"Desk\""));
        assert_eq!(toml::from_str::<Presets>(&text).unwrap(), p);
    }
}
