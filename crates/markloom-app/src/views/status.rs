//! Per-column status lines. Copy matches the spec.

use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{App, IntoElement, ParentElement, SharedString, Styled, div, px};
use markloom_style::{Orientation, PaperSize, StyleSheet};

use crate::document::Document;
use crate::preview_state::PreviewStatus;

/// `{words} words · {lines} lines`
pub fn word_line(document: &Document) -> String {
    let stats = document.stats();
    format!("{} words · {} lines", stats.words, stats.lines)
}

/// `Ln {l}, Col {c}` with the editor's 1-based cursor.
pub fn cursor_label(document: &Document) -> String {
    let (line, column) = document.cursor();
    format!("Ln {line}, Col {column}")
}

/// Saved, Edited, or Changed on disk.
pub fn save_label(document: &Document) -> &'static str {
    let Some(buffer) = document.workspace().active() else {
        return "Saved";
    };
    if buffer.conflict {
        "Changed on disk"
    } else if buffer.is_dirty() {
        "Edited"
    } else {
        "Saved"
    }
}

/// `A4 · Portrait` and the other paper and orientation names.
pub fn paper_label(sheet: &StyleSheet) -> String {
    let paper = match sheet.page.size {
        PaperSize::A4 => "A4",
        PaperSize::Letter => "Letter",
        PaperSize::A5 => "A5",
        PaperSize::Legal => "Legal",
    };
    let orientation = match sheet.page.orientation {
        Orientation::Portrait => "Portrait",
        Orientation::Landscape => "Landscape",
    };
    format!("{paper} · {orientation}")
}

/// Preview status text, and whether it is an error.
pub fn preview_text(status: &PreviewStatus) -> (SharedString, bool) {
    match status {
        PreviewStatus::LoadingFonts => ("Loading fonts…".into(), false),
        PreviewStatus::Compiling => ("Compiling…".into(), false),
        PreviewStatus::Ready { diagrams, .. } => {
            (format!("Preview ready · {diagrams} diagrams").into(), false)
        }
        PreviewStatus::Error(message) => (message.clone().into(), true),
    }
}

pub fn bar(
    left: impl Into<SharedString>,
    right: impl Into<SharedString>,
    danger: bool,
    cx: &App,
) -> impl IntoElement {
    let left = left.into();
    let right = right.into();
    let quiet = cx.theme().muted_foreground;
    h_flex()
        .h(px(28.))
        .px_3()
        .gap_3()
        .justify_between()
        .items_center()
        .text_xs()
        .text_color(quiet)
        .border_t_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .child(
            div()
                .text_color(if danger { cx.theme().danger } else { quiet })
                .child(left),
        )
        .child(right)
}
