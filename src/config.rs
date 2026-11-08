//! Configuration files in `<config dir>/odyssey-ctl/` (`settings.toml`, `presets.toml`).
//! `ODYSSEY_CTL_CONFIG_DIR` overrides the directory, for tests and scripts.

use std::path::PathBuf;

use serde::Serialize;

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
    let path =
        config_file(name).ok_or_else(|| std::io::Error::other("no configuration directory"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = toml::to_string(value).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}
