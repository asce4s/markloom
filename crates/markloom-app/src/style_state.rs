//! Preset plus overrides, resolved only when the style actually changes.

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
}

impl EventEmitter<StyleEvent> for StyleState {}

impl StyleState {
    /// Start from a built-in preset with no overrides.
    pub fn new(preset: impl Into<String>, _cx: &mut Context<Self>) -> Self {
        let registry = PresetRegistry::builtin();
        let session = StyleSession::new(preset);
        let sheet = session
            .resolve(&registry)
            .expect("built-in preset resolves");
        Self {
            registry,
            session,
            sheet: Arc::new(sheet),
        }
    }

    /// Last sheet that resolved.
    pub fn sheet(&self) -> &Arc<StyleSheet> {
        &self.sheet
    }

    /// Preset name currently selected.
    pub fn preset(&self) -> &str {
        self.session.preset()
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
