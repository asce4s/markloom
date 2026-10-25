//! Tagline, document theme, and the settings toggle.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{Context, Entity, IntoElement, ParentElement, Styled, div, px};

use crate::app::{AppRoot, ToggleSettings};
use crate::style_state::StyleState;

pub fn sub_bar(style: Entity<StyleState>, cx: &mut Context<AppRoot>) -> impl IntoElement {
    let names = style.read(cx).names();
    let quiet = cx.theme().muted_foreground;

    h_flex()
        .h(px(36.))
        .px_3()
        .gap_3()
        .items_center()
        .bg(cx.theme().background)
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex_1()
                .text_xs()
                .text_color(quiet)
                .child("Write Markdown. See the page."),
        )
        .child(preset_menu(style, names))
        .child(
            Button::new("settings-toggle")
                .label("Settings")
                .ghost()
                .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleSettings), cx)),
        )
}

fn preset_menu(style: Entity<StyleState>, names: Vec<String>) -> impl IntoElement {
    Button::new("document-theme")
        .label("Document theme")
        .ghost()
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu;
            for name in names.clone() {
                let style = style.clone();
                let picked = name.clone();
                menu = menu.item(PopupMenuItem::new(name).on_click(move |_, _, cx| {
                    let picked = picked.clone();
                    style.update(cx, |style, cx| style.set_preset(picked, cx));
                }));
            }
            menu
        })
}
