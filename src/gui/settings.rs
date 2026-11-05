//! Application settings (not monitor settings), stored in `<config dir>/odyssey-ctl/settings.toml`.
//! `ODYSSEY_CTL_CONFIG_DIR` overrides the directory, for tests.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Lang,
    pub allow_unverified: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { language: Lang::from_env(), allow_unverified: false }
    }
}

/// `<config dir>/odyssey-ctl/<name>`.
pub fn config_file(name: &str) -> Option<PathBuf> {
    let dir = match std::env::var_os("ODYSSEY_CTL_CONFIG_DIR") {
        Some(d) => PathBuf::from(d),
        None => dirs::config_dir()?.join("odyssey-ctl"),
    };
    Some(dir.join(name))
}

/// Reads a TOML file, falling back to the default when it is missing or invalid.
pub fn load_toml<T: serde::de::DeserializeOwned + Default>(name: &str) -> T {
    config_file(name)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_toml<T: Serialize>(name: &str, value: &T) -> std::io::Result<()> {
    let path = config_file(name).ok_or_else(|| std::io::Error::other("no configuration directory"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = toml::to_string(value).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
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
        let s = Settings { language: Lang::It, allow_unverified: true };
        let text = toml::to_string(&s).unwrap();
        assert_eq!(text, "language = \"it\"\nallow_unverified = true\n");
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
        let partial: Settings = toml::from_str("language = \"en\"").unwrap();
        assert!(!partial.allow_unverified);
    }
}
