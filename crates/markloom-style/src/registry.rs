//! Built-in presets, user overrides, and `extends` inheritance.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::merge::deep_merge;
use crate::sheet::{StyleError, StyleSheet};

const BUILTINS: &[(&str, &str)] = &[
    ("github", include_str!("../presets/github.toml")),
    ("compact", include_str!("../presets/compact.toml")),
    ("academic", include_str!("../presets/academic.toml")),
    ("minimal", include_str!("../presets/minimal.toml")),
    (
        "print-friendly",
        include_str!("../presets/print-friendly.toml"),
    ),
];

/// A user preset file that was skipped.
#[derive(Debug)]
pub struct PresetLoadError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for PresetLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for PresetLoadError {}

/// Built-in presets plus any user files loaded later.
#[derive(Debug, Clone)]
pub struct PresetRegistry {
    user: BTreeMap<String, String>,
}

impl PresetRegistry {
    /// The five built-in presets, in display order.
    pub fn builtin() -> Self {
        Self {
            user: BTreeMap::new(),
        }
    }

    /// Load `*.toml` from `dir`. A bad file is reported and skipped.
    pub fn load_user_dir(&mut self, dir: &Path) -> Vec<PresetLoadError> {
        let mut errors = Vec::new();
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => {
                errors.push(PresetLoadError {
                    path: dir.to_path_buf(),
                    message: error.to_string(),
                });
                return errors;
            }
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .collect();
        paths.sort();
        for path in paths {
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            match fs::read_to_string(&path) {
                Ok(text) => match toml::from_str::<toml::Table>(&text) {
                    Ok(_) => {
                        self.user.insert(name.to_string(), text);
                    }
                    Err(error) => errors.push(PresetLoadError {
                        path,
                        message: error.to_string(),
                    }),
                },
                Err(error) => errors.push(PresetLoadError {
                    path,
                    message: error.to_string(),
                }),
            }
        }
        errors
    }

    /// Built-ins first, then user-only names. A shadowing user file keeps the built-in slot.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = BUILTINS
            .iter()
            .map(|(name, _)| (*name).to_string())
            .collect();
        for name in self.user.keys() {
            if !names.iter().any(|existing| existing == name) {
                names.push(name.clone());
            }
        }
        names
    }

    /// Resolve `name`, then apply `overrides` from first to last.
    pub fn resolve(&self, name: &str, overrides: &[toml::Table]) -> Result<StyleSheet, StyleError> {
        let chain = self.chain(name)?;
        let mut merged = toml::Table::new();
        for source in chain {
            let mut table: toml::Table =
                toml::from_str(&source).map_err(|error| StyleError::Message(error.to_string()))?;
            table.remove("extends");
            deep_merge(&mut merged, &table);
        }
        for over in overrides {
            deep_merge(&mut merged, over);
        }
        merged.remove("extends");
        let text =
            toml::to_string(&merged).map_err(|error| StyleError::Message(error.to_string()))?;
        let sheet: StyleSheet =
            toml::from_str(&text).map_err(|error| StyleError::Message(error.to_string()))?;
        sheet.validate()?;
        Ok(sheet)
    }

    fn source_of(&self, name: &str) -> Option<&str> {
        self.user.get(name).map(String::as_str).or_else(|| {
            BUILTINS
                .iter()
                .find(|(builtin, _)| *builtin == name)
                .map(|(_, text)| *text)
        })
    }

    fn chain(&self, name: &str) -> Result<Vec<String>, StyleError> {
        let mut seen = Vec::new();
        let mut current = name.to_string();
        let mut sources = Vec::new();
        loop {
            if seen.iter().any(|item: &String| item == &current) {
                return Err(StyleError::Cycle(current));
            }
            seen.push(current.clone());
            let source = self
                .source_of(&current)
                .ok_or_else(|| StyleError::Unknown(current.clone()))?;
            let table: toml::Table =
                toml::from_str(source).map_err(|error| StyleError::Message(error.to_string()))?;
            let parent = table
                .get("extends")
                .and_then(toml::Value::as_str)
                .map(str::to_string);
            sources.push(source.to_string());
            match parent {
                Some(parent) => current = parent,
                None => break,
            }
        }
        sources.reverse();
        Ok(sources)
    }
}
