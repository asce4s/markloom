//! Wordmark, file name, and the quiet actions on the top row.

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{
    App, Context, Entity, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    Styled, div, px,
};

use crate::probe;

use crate::app::{AppRoot, ExportDocx, ExportPdf, FindInFiles, OpenPath, QuickOpen};
use crate::document::Document;
use crate::views::status::save_label;

pub fn top_bar(
    app: Entity<AppRoot>,
    document: Entity<Document>,
    cx: &mut Context<AppRoot>,
) -> impl IntoElement {
    let name = file_name(&document, cx);
    let saved = save_label(document.read(cx));
    let theme_glyph: SharedString = if cx.theme().is_dark() { "☀" } else { "☾" }.into();
    let quiet = cx.theme().muted_foreground;

    h_flex()
        .h(px(44.))
        .px_3()
        .gap_2()
        .items_center()
        .bg(cx.theme().background)
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().text_sm().child("markloom"))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_sm()
                .text_color(quiet)
                .child(format!("{name} · {saved}")),
        )
        .child(
            Button::new("open")
                .label("Open")
                .outline()
                .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPath), cx)),
        )
        .child(
            div()
                .id("more")
                .flex()
                .flex_none()
                .on_prepaint(|bounds, _, _| probe::record("more", bounds))
                .child(more_menu(app.clone())),
        )
        .child(Button::new("theme").label(theme_glyph).ghost().on_click({
            let app = app.clone();
            move |_, window, cx| {
                app.update(cx, |app, cx| app.toggle_theme(window, cx));
            }
        }))
        .child(
            Button::new("export-pdf")
                .label("Export PDF")
                .primary()
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ExportPdf), cx)),
        )
}

fn file_name(document: &Entity<Document>, cx: &App) -> String {
    document
        .read(cx)
        .workspace()
        .active()
        .map(|buffer| {
            buffer
                .relative
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| buffer.relative.display().to_string())
        })
        .unwrap_or_else(|| "Untitled".into())
}

fn more_menu(app: Entity<AppRoot>) -> impl IntoElement {
    Button::new("more-button")
        .label("⋯")
        .ghost()
        .dropdown_menu(move |menu, _, _| {
            let new_file = app.clone();
            let preset = app.clone();
            menu.item(
                PopupMenuItem::new("New file").on_click(move |_, window, cx| {
                    new_file.update(cx, |app, cx| app.new_file(window, cx));
                }),
            )
            .item(PopupMenuItem::new("Quick open").on_click(|_, window, cx| {
                window.dispatch_action(Box::new(QuickOpen), cx);
            }))
            .item(
                PopupMenuItem::new("Find in files").on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(FindInFiles), cx);
                }),
            )
            .item(PopupMenuItem::new("Export DOCX").on_click(|_, window, cx| {
                window.dispatch_action(Box::new(ExportDocx), cx);
            }))
            .item(
                PopupMenuItem::element(|_, _| {
                    div()
                        .id("save-preset")
                        .on_prepaint(|bounds, _, _| probe::record("save-preset", bounds))
                        .child("Save style as preset…")
                })
                .on_click(move |_, window, cx| {
                    preset.update(cx, |app, cx| app.save_style_as_preset(window, cx));
                }),
            )
        })
}
