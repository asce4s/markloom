//! Markdown column: header, snippet menu, editor, status.

use gpui_kit::Focusable as _;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Editor, Search};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{App, Entity, InteractiveElement as _, IntoElement, ParentElement, Styled, div, px};

use crate::document::Document;
use crate::probe;
use crate::text::Snippet;
use crate::views::status::{self, cursor_label, word_line};

pub fn column(document: Entity<Document>, cx: &App) -> impl IntoElement {
    let words = word_line(document.read(cx));
    let cursor = cursor_label(document.read(cx));
    let editor = document.read(cx).editor().clone();

    v_flex()
        .size_full()
        .bg(cx.theme().background)
        .child(header(document.clone(), cx))
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .child(Editor::new(&editor).size_full().bordered(false)),
        )
        .child(status_line(words, cursor, cx))
}

fn header(document: Entity<Document>, cx: &App) -> impl IntoElement {
    h_flex()
        .h(px(36.))
        .px_3()
        .gap_2()
        .items_center()
        .justify_between()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child("MARKDOWN")
        .child(
            h_flex()
                .gap_1()
                .child(find_button(document.clone()))
                .child(insert_menu(document)),
        )
}

fn find_button(document: Entity<Document>) -> impl IntoElement {
    Button::new("find")
        .label("Find")
        .ghost()
        .on_click(move |_, window, cx| {
            let editor = document.read(cx).editor().clone();
            editor.read(cx).focus_handle(cx).focus(window, cx);
            window.dispatch_action(Box::new(Search), cx);
        })
}

fn insert_menu(document: Entity<Document>) -> impl IntoElement {
    div()
        .id("insert")
        .flex()
        .flex_none()
        .on_prepaint(|bounds, _, _| probe::record("insert", bounds))
        .child(
            Button::new("insert-button")
                .label("+ Insert")
                .ghost()
                .dropdown_menu(move |menu, _, _| {
                    menu.item(snippet_item(
                        "insert-table",
                        "Table",
                        Snippet::Table,
                        document.clone(),
                    ))
                    .item(snippet_item(
                        "",
                        "Code block",
                        Snippet::CodeBlock,
                        document.clone(),
                    ))
                    .item(snippet_item(
                        "",
                        "Mermaid",
                        Snippet::Mermaid,
                        document.clone(),
                    ))
                    .item(snippet_item("", "Image", Snippet::Image, document.clone()))
                    .item(snippet_item("", "Quote", Snippet::Quote, document.clone()))
                    .item(snippet_item(
                        "",
                        "Horizontal rule",
                        Snippet::Rule,
                        document.clone(),
                    ))
                }),
        )
}

fn snippet_item(
    probe_id: &'static str,
    label: &'static str,
    kind: Snippet,
    document: Entity<Document>,
) -> PopupMenuItem {
    let item = if probe_id.is_empty() {
        PopupMenuItem::new(label)
    } else {
        PopupMenuItem::element(move |_, _| {
            div()
                .id(probe_id)
                .on_prepaint(move |bounds, _, _| probe::record(probe_id, bounds))
                .child(label)
        })
    };
    item.on_click(move |_, window, cx| {
        document.update(cx, |document, cx| {
            document.insert_snippet(kind, window, cx);
        });
    })
}

fn status_line(words: String, cursor: String, cx: &App) -> impl IntoElement {
    let right = format!("UTF-8  {cursor}");
    let shown = words.clone();
    div()
        .id("editor-words")
        .on_prepaint(move |bounds, _, _| {
            probe::record("editor-words", bounds);
            probe::set_text("editor-words", shown.clone());
        })
        .child(status::bar(words, right, false, cx))
}
