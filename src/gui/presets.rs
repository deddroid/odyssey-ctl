//! Named presets of monitor settings, stored in `<config dir>/odyssey-ctl/presets.toml`
//! (separate from the application settings).

use odyssey_ctl::presets::Values;
use serde::{Deserialize, Serialize};

use crate::settings::{load_toml, save_toml};

const FILE: &str = "presets.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    /// Feature name -> value, as in `odyssey-ctl features`.
    pub values: Values,
}

/// A saved crosshair position (firmware units, see `odyssey_ctl::aim`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AimSpot {
    pub name: String,
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Presets {
    #[serde(default, rename = "preset")]
    pub list: Vec<Preset>,
    #[serde(default, rename = "aim")]
    pub aim: Vec<AimSpot>,
}

impl Presets {
    pub fn load() -> Self {
        load_toml(FILE)
    }

    pub fn save(&self) -> std::io::Result<()> {
        save_toml(FILE, self)
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
        let text = toml::to_string(&p).unwrap();
        assert!(text.contains("[[preset]]") && text.contains("name = \"Notte\""));
        assert!(text.contains("[[aim]]"));
        assert_eq!(toml::from_str::<Presets>(&text).unwrap(), p);
        // Files from 0.1.0 have no [[aim]] table.
        assert!(
            toml::from_str::<Presets>("[[preset]]\nname = \"a\"\n[preset.values]\n")
                .unwrap()
                .aim
                .is_empty()
        );
    }
}
