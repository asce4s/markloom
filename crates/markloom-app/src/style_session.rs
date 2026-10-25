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

    /// Set one override. Numeric segments are array indexes.
    pub fn set(&mut self, path: &str, value: Value) {
        let segments = segments(path);
        if segments.is_empty() {
            return;
        }
        let mut root = Value::Table(std::mem::take(&mut self.overrides));
        write_at(&mut root, &segments, value);
        self.overrides = take_table(root);
    }

    /// Remove the override at `prefix` and every override under it.
    pub fn clear(&mut self, prefix: &str) {
        let segments = segments(prefix);
        if segments.is_empty() {
            self.overrides.clear();
            return;
        }
        let mut root = Value::Table(std::mem::take(&mut self.overrides));
        let _ = remove_at(&mut root, &segments);
        prune(&mut root);
        self.overrides = take_table(root);
    }

    /// Drop every override.
    pub fn clear_all(&mut self) {
        self.overrides.clear();
    }

    /// Whether `prefix` or something under it is overridden.
    pub fn is_overridden(&self, prefix: &str) -> bool {
        let segments = segments(prefix);
        if segments.is_empty() {
            return !self.overrides.is_empty();
        }
        let head = segments[0];
        let Some(child) = self.overrides.get(head) else {
            return false;
        };
        lookup(child, &segments[1..]).is_some()
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
}

fn segments(path: &str) -> Vec<&str> {
    path.split('.').filter(|part| !part.is_empty()).collect()
}

fn take_table(value: Value) -> toml::Table {
    match value {
        Value::Table(table) => table,
        _ => toml::Table::new(),
    }
}

fn write_at(cursor: &mut Value, segments: &[&str], value: Value) {
    let Some(head) = segments.first().copied() else {
        *cursor = value;
        return;
    };
    let rest = &segments[1..];
    if let Ok(index) = head.parse::<usize>() {
        if !matches!(cursor, Value::Array(_)) {
            *cursor = Value::Array(Vec::new());
        }
        let Value::Array(items) = cursor else {
            return;
        };
        if index >= items.len() {
            items.resize(index + 1, Value::String(String::new()));
        }
        if rest.is_empty() {
            items[index] = value;
        } else {
            write_at(&mut items[index], rest, value);
        }
        return;
    }
    if !matches!(cursor, Value::Table(_)) {
        *cursor = Value::Table(toml::Table::new());
    }
    let Value::Table(table) = cursor else {
        return;
    };
    if rest.is_empty() {
        table.insert(head.to_string(), value);
        return;
    }
    let wants_array = rest[0].parse::<usize>().is_ok();
    let entry = table.entry(head.to_string()).or_insert_with(|| {
        if wants_array {
            Value::Array(Vec::new())
        } else {
            Value::Table(toml::Table::new())
        }
    });
    if wants_array && !matches!(entry, Value::Array(_)) {
        *entry = Value::Array(Vec::new());
    }
    if !wants_array && !matches!(entry, Value::Table(_)) {
        *entry = Value::Table(toml::Table::new());
    }
    write_at(entry, rest, value);
}

fn remove_at(cursor: &mut Value, segments: &[&str]) -> bool {
    let Some(head) = segments.first().copied() else {
        return true;
    };
    let rest = &segments[1..];
    if let Ok(index) = head.parse::<usize>() {
        let Value::Array(items) = cursor else {
            return false;
        };
        if index >= items.len() {
            return items.is_empty();
        }
        if rest.is_empty() {
            items.remove(index);
            return items.is_empty();
        }
        if remove_at(&mut items[index], rest) {
            items.remove(index);
        }
        return items.is_empty();
    }
    let Value::Table(table) = cursor else {
        return false;
    };
    if rest.is_empty() {
        table.remove(head);
        return table.is_empty();
    }
    let Some(child) = table.get_mut(head) else {
        return table.is_empty();
    };
    if remove_at(child, rest) {
        table.remove(head);
    }
    table.is_empty()
}

fn prune(value: &mut Value) {
    match value {
        Value::Table(table) => {
            let keys: Vec<String> = table.keys().cloned().collect();
            for key in keys {
                if let Some(child) = table.get_mut(&key) {
                    prune(child);
                }
                let empty = match table.get(&key) {
                    Some(Value::Table(child)) => child.is_empty(),
                    Some(Value::Array(child)) => child.is_empty(),
                    _ => false,
                };
                if empty {
                    table.remove(&key);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                prune(item);
            }
        }
        Value::String(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Boolean(_)
        | Value::Datetime(_) => {}
    }
}

fn lookup<'a>(cursor: &'a Value, segments: &[&str]) -> Option<&'a Value> {
    let Some(head) = segments.first().copied() else {
        return Some(cursor);
    };
    let rest = &segments[1..];
    if let Ok(index) = head.parse::<usize>() {
        let Value::Array(items) = cursor else {
            return None;
        };
        let child = items.get(index)?;
        return lookup(child, rest);
    }
    let Value::Table(table) = cursor else {
        return None;
    };
    let child = table.get(head)?;
    lookup(child, rest)
}
