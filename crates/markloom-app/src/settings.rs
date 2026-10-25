//! `~/.config/markloom/settings.toml`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Light or dark chrome. The document preset is separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppTheme {
    #[default]
    Light,
    Dark,
}

/// App preferences. A missing or broken file loads the defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: AppTheme,
    pub editor_font: String,
    pub default_preset: String,
    pub autosave: bool,
    pub recent: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: AppTheme::Light,
            editor_font: "DejaVu Sans Mono".into(),
            default_preset: "github".into(),
            autosave: false,
            recent: Vec::new(),
        }
    }
}

/// Directory that holds settings and user presets.
pub fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("markloom");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config").join("markloom")
}

/// Read settings. Absence and parse errors both yield [`Settings::default`].
pub fn load(path: &Path) -> Settings {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Settings::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

/// Write settings, creating the parent directory.
pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("create settings dir: {error}"))?;
    }
    let text =
        toml::to_string_pretty(settings).map_err(|error| format!("encode settings: {error}"))?;
    std::fs::write(path, text).map_err(|error| format!("write settings: {error}"))
}

/// Remember `folder` at the front of the recent list, capped at 8.
pub fn remember(settings: &mut Settings, folder: &Path) {
    settings.recent.retain(|path| path != folder);
    settings.recent.insert(0, folder.to_path_buf());
    settings.recent.truncate(8);
}

/// Autosave only writes a buffer that differs from disk.
pub fn should_autosave(settings: &Settings, dirty: bool) -> bool {
    settings.autosave && dirty
}
