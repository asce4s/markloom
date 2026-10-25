//! Painted bounds for the layout tests.
//!
//! gpui-kit's `test-support` feature does not build in this workspace: it pulls
//! gpui-fast, and turning on gpui-base's own feature changes `.test_support()`
//! into a wrapper the component crate cannot chain. These probes are the
//! stand-in. A frame clears them, and only elements that prepaint record again.

use std::cell::RefCell;
use std::collections::HashMap;

use gpui_kit::{Bounds, Pixels};

thread_local! {
    static BOUNDS: RefCell<HashMap<&'static str, Bounds<Pixels>>> = RefCell::new(HashMap::new());
    static TEXT: RefCell<HashMap<&'static str, String>> = RefCell::new(HashMap::new());
}

/// Drop last frame's probes. Call this at the start of the window render.
pub fn clear() {
    BOUNDS.with(|bounds| bounds.borrow_mut().clear());
    TEXT.with(|text| text.borrow_mut().clear());
}

/// Remember an element that actually painted.
pub fn record(name: &'static str, bounds: Bounds<Pixels>) {
    BOUNDS.with(|slots| {
        slots.borrow_mut().insert(name, bounds);
    });
}

/// Remember the text a painted element is showing.
pub fn set_text(name: &'static str, text: impl Into<String>) {
    TEXT.with(|slots| {
        slots.borrow_mut().insert(name, text.into());
    });
}

/// Bounds from the last completed frame, if `name` painted.
pub fn bounds(name: &str) -> Option<Bounds<Pixels>> {
    BOUNDS.with(|slots| slots.borrow().get(name).copied())
}

/// Text recorded for `name` during the last frame.
pub fn text(name: &str) -> Option<String> {
    TEXT.with(|slots| slots.borrow().get(name).cloned())
}
