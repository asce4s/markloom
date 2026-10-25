//! Preset plus overrides, resolved only when the style actually changes.

use std::path::Path;
use std::sync::Arc;

use gpui_kit::{Context, EventEmitter};
use markloom_style::{PresetRegistry, StyleSheet};
use toml::Value;

use crate::style_session::StyleSession;

/// The style panel changed the resolved sheet, or the edit did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleEvent {
    Changed,
    Error(String),
}

/// Registry, session, and the last sheet that resolved.
pub struct StyleState {
    registry: PresetRegistry,
    session: StyleSession,
    sheet: Arc<StyleSheet>,
    /// Shown once when startup could not resolve the requested preset.
    fallback: Option<String>,
}

impl EventEmitter<StyleEvent> for StyleState {}

impl StyleState {
    /// Start from `preset`. `presets_dir` is loaded first when it is set.
    ///
    /// An unknown or broken preset falls back to `github`. [`Self::take_fallback`]
    /// returns the notice for that one case.
    pub fn new(
        preset: impl Into<String>,
        presets_dir: Option<&Path>,
        _cx: &mut Context<Self>,
    ) -> Self {
        let mut registry = PresetRegistry::builtin();
        if let Some(dir) = presets_dir {
            let _ = registry.load_user_dir(dir);
        }
        let mut session = StyleSession::new(preset);
        let mut fallback = None;
        let sheet = match session.resolve(&registry) {
            Ok(sheet) => sheet,
            Err(error) => {
                fallback = Some(format!("{error} — using github"));
                session.set_preset("github");
                session.resolve(&registry).expect("github preset resolves")
            }
        };
        Self {
            registry,
            session,
            sheet: Arc::new(sheet),
            fallback,
        }
    }

    /// Consume the startup fallback notice, if the requested preset did not resolve.
    pub fn take_fallback(&mut self) -> Option<String> {
        self.fallback.take()
    }

    /// Last sheet that resolved.
    pub fn sheet(&self) -> &Arc<StyleSheet> {
        &self.sheet
    }

    /// Preset name currently selected.
    pub fn preset(&self) -> &str {
        self.session.preset()
    }

    /// Built-in names, then user presets.
    pub fn names(&self) -> Vec<String> {
        self.registry.names()
    }

    /// The override at `path`, when it is a string.
    pub fn override_text(&self, path: &str) -> Option<String> {
        let root = Value::Table(self.session.overrides().clone());
        let mut cursor = &root;
        for part in path.split('.') {
            cursor = match cursor {
                Value::Table(table) => table.get(part)?,
                Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        match cursor {
            Value::String(text) => Some(text.clone()),
            _ => None,
        }
    }

    /// Read `*.toml` presets from `dir` into the registry.
    pub fn load_user_presets(&mut self, dir: &std::path::Path) {
        let _ = self.registry.load_user_dir(dir);
    }

    /// Write the resolved sheet. The caller adds `extends` when it wants inheritance.
    pub fn save_preset(&self, dir: &std::path::Path, name: &str) -> Result<(), String> {
        self.session.save_preset(dir, name, &self.sheet)
    }

    /// Whether `prefix` or something under it is overridden.
    pub fn is_overridden(&self, prefix: &str) -> bool {
        self.session.is_overridden(prefix)
    }

    /// Set one override and re-resolve. A bad value keeps the previous sheet.
    pub fn set(&mut self, path: &str, value: Value, cx: &mut Context<Self>) {
        self.session.set(path, normalize(path, value));
        self.reresolve(cx);
    }

    /// Remove one override prefix and re-resolve.
    pub fn clear(&mut self, prefix: &str, cx: &mut Context<Self>) {
        self.session.clear(prefix);
        self.reresolve(cx);
    }

    /// Drop every override and re-resolve.
    pub fn clear_all(&mut self, cx: &mut Context<Self>) {
        self.session.clear_all();
        self.reresolve(cx);
    }

    /// Switch preset, drop overrides, and re-resolve.
    pub fn set_preset(&mut self, name: impl Into<String>, cx: &mut Context<Self>) {
        self.session.set_preset(name);
        self.reresolve(cx);
    }

    fn reresolve(&mut self, cx: &mut Context<Self>) {
        match self.session.resolve(&self.registry) {
            Ok(sheet) => {
                self.sheet = Arc::new(sheet);
                cx.emit(StyleEvent::Changed);
            }
            Err(error) => cx.emit(StyleEvent::Error(error.to_string())),
        }
        cx.notify();
    }
}

/// `PaperSize` serde tags are PascalCase. The settings paths also accept the
/// lowercase names from the plan (`letter`, `a4`, `a5`, `legal`).
fn normalize(path: &str, value: Value) -> Value {
    if path != "page.size" {
        return value;
    }
    let Value::String(text) = value else {
        return value;
    };
    let lowered = text.to_ascii_lowercase();
    let mapped = match lowered.as_str() {
        "a4" => "A4",
        "letter" => "Letter",
        "a5" => "A5",
        "legal" => "Legal",
        _ => return Value::String(text),
    };
    Value::String(mapped.to_string())
}
