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
    /// Clicking the desktop applies the no-rule preset (false: the last rule stays).
    pub desktop_resets: bool,
    /// Stable EDID identity of the last monitor selected by the user.
    pub preferred_monitor: Option<String>,
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
            desktop_resets: true,
            preferred_monitor: None,
        }
    }
}

impl Settings {
    /// An absent preferred monitor must never fall back to a different display.
    pub fn select_bus(
        &self,
        explicit: Option<u32>,
        displays: &[odyssey_ctl::actions::DisplayInfo],
    ) -> Option<u32> {
        explicit.or_else(|| {
            self.preferred_monitor.as_ref().map(|id| {
                displays
                    .iter()
                    .find(|d| &d.identity == id)
                    .map(|d| d.bus)
                    .unwrap_or(u32::MAX)
            })
        })
    }

    pub fn resolve_bus(&self, explicit: Option<u32>) -> Option<u32> {
        if explicit.is_some() || self.preferred_monitor.is_none() {
            explicit
        } else {
            self.select_bus(None, &odyssey_ctl::actions::discover())
        }
    }

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
    fn restores_identity_after_bus_changes_and_never_selects_another_monitor() {
        use odyssey_ctl::actions::DisplayInfo;
        let mut s = Settings::default();
        s.preferred_monitor = Some("selected-serial".into());
        let display = |bus, identity: &str| DisplayInfo {
            bus,
            identity: identity.into(),
            name: "monitor".into(),
            samsung: true,
        };
        assert_eq!(
            s.select_bus(None, &[display(3, "other"), display(9, "selected-serial")]),
            Some(9)
        );
        assert_eq!(s.select_bus(None, &[display(3, "other")]), Some(u32::MAX));
        assert_eq!(s.select_bus(Some(3), &[]), Some(3));
        let restored: Settings = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored.preferred_monitor, s.preferred_monitor);
    }

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
        assert!(!partial.per_app && partial.desktop_resets);
        assert_eq!(partial.theme, ThemeChoice::System);
    }
}
