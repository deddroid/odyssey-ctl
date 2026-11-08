//! Application settings (not monitor settings), stored in `<config dir>/odyssey-ctl/settings.toml`.

use eframe::egui;
use odyssey_ctl::config::{load_toml, save_toml};
use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

/// Light or dark, or whatever the system uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    pub fn key(self) -> &'static str {
        match self {
            ThemeChoice::System => "theme.system",
            ThemeChoice::Light => "theme.light",
            ThemeChoice::Dark => "theme.dark",
        }
    }

    pub fn preference(self) -> egui::ThemePreference {
        match self {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Lang,
    pub theme: ThemeChoice,
    pub allow_unverified: bool,
    /// Closing the window keeps the program in the tray.
    pub close_to_tray: bool,
    /// Start with the window closed (tray only).
    pub start_hidden: bool,
    /// Launched at login (hidden); mirrors the autostart entry.
    pub autostart: bool,
    /// Apply presets from the per-application rules when the focused window changes (KDE).
    pub per_app: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: Lang::from_env(),
            theme: ThemeChoice::System,
            allow_unverified: false,
            close_to_tray: true,
            start_hidden: false,
            autostart: false,
            per_app: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        load_toml("settings.toml")
    }

    pub fn save(&self) -> std::io::Result<()> {
        save_toml("settings.toml", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let s = Settings {
            language: Lang::It,
            allow_unverified: true,
            start_hidden: true,
            ..Default::default()
        };
        let text = toml::to_string(&s).unwrap();
        assert!(text.contains("language = \"it\"") && text.contains("start_hidden = true"));
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
        let partial: Settings = toml::from_str("language = \"en\"").unwrap();
        assert!(!partial.allow_unverified && partial.close_to_tray && !partial.autostart);
        assert!(!partial.per_app);
        assert_eq!(partial.theme, ThemeChoice::System);
    }
}
