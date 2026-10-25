//! Preset plus document-level overrides for the style panel.

use std::path::Path;

use markloom_style::{PresetRegistry, StyleError, StyleSheet};
use toml::Value;

/// The preset name and the TOML overrides layered on top of it.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleSession {
    preset: String,
    overrides: toml::Table,
}

impl StyleSession {
    /// Start from a preset with no overrides.
    pub fn new(preset: impl Into<String>) -> Self {
        Self {
            preset: preset.into(),
            overrides: toml::Table::new(),
        }
    }

    /// Preset name.
    pub fn preset(&self) -> &str {
        &self.preset
    }

    /// Override table.
    pub fn overrides(&self) -> &toml::Table {
        &self.overrides
    }

    /// Resolve the preset and the overrides.
    pub fn resolve(&self, registry: &PresetRegistry) -> Result<StyleSheet, StyleError> {
        registry.resolve(&self.preset, std::slice::from_ref(&self.overrides))
    }

    /// Switch preset and drop overrides.
    pub fn set_preset(&mut self, name: impl Into<String>) {
        self.preset = name.into();
        self.overrides.clear();
    }

    /// Back to `default_preset` with no overrides.
    pub fn reset(&mut self, default_preset: &str) {
        self.set_preset(default_preset);
    }

    /// Set `typography.base_size`.
    pub fn set_base_size_pt(&mut self, points: f32) {
        self.set_path(
            &["typography", "base_size"],
            Value::String(format!("{points}pt")),
        );
    }

    /// Set `colors.text` from `#rrggbb`.
    pub fn set_text_color(&mut self, hex: &str) {
        self.set_path(&["colors", "text"], Value::String(hex.to_string()));
    }

    /// Set `table.borders`.
    pub fn set_table_borders(&mut self, name: &str) {
        self.set_path(&["table", "borders"], Value::String(name.to_string()));
    }

    /// Set `diagram.theme`.
    pub fn set_diagram_theme(&mut self, name: &str) {
        self.set_path(&["diagram", "theme"], Value::String(name.to_string()));
    }

    /// Set `diagram.scale`.
    pub fn set_diagram_scale(&mut self, scale: f32) {
        self.set_path(&["diagram", "scale"], float_value(scale));
    }

    /// Set the first body font.
    pub fn set_body_font(&mut self, font: &str) {
        self.set_path(
            &["typography", "body_font"],
            Value::Array(vec![Value::String(font.to_string())]),
        );
    }

    /// Write the resolved sheet as a user preset.
    pub fn save_preset(&self, dir: &Path, name: &str, sheet: &StyleSheet) -> Result<(), String> {
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err("preset name is empty".into());
        }
        std::fs::create_dir_all(dir).map_err(|error| format!("create preset dir: {error}"))?;
        let text =
            toml::to_string_pretty(sheet).map_err(|error| format!("encode preset: {error}"))?;
        let path = dir.join(format!("{name}.toml"));
        std::fs::write(path, text).map_err(|error| format!("write preset: {error}"))
    }

    fn set_path(&mut self, path: &[&str], value: Value) {
        let mut table = &mut self.overrides;
        for (index, key) in path.iter().enumerate() {
            if index + 1 == path.len() {
                table.insert((*key).to_string(), value);
                return;
            }
            let entry = table
                .entry((*key).to_string())
                .or_insert_with(|| Value::Table(toml::Table::new()));
            match entry {
                Value::Table(next) => table = next,
                other => {
                    *other = Value::Table(toml::Table::new());
                    let Value::Table(next) = other else {
                        return;
                    };
                    table = next;
                }
            }
        }
    }
}

fn float_value(value: f32) -> Value {
    Value::Float(f64::from(value))
}
