//! One settings row: label, accent dot, and the control.

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerState};
use gpui_kit::component::input::{Editor, EditorState, Input, InputState, NumberInput};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Entity, Focusable as _, InteractiveElement as _, IntoElement, ParentElement, Styled,
    Window, div, px,
};

use super::fields::{Choice, NumberKind};
use crate::probe;
use crate::style_state::StyleState;

pub(super) fn reset_row(
    id: &'static str,
    style: Entity<StyleState>,
    prefixes: &'static [&'static str],
) -> impl IntoElement {
    div()
        .id(id)
        .mt_2()
        .on_prepaint(move |bounds, _, _| probe::record(id, bounds))
        .child(
            Button::new(format!("{id}-button"))
                .label("Reset to preset")
                .ghost()
                .xsmall()
                .on_click(move |_, _, cx| {
                    style.update(cx, |style, cx| {
                        for prefix in prefixes {
                            style.clear(prefix, cx);
                        }
                    });
                }),
        )
}

pub(super) fn number_row(
    id: &'static str,
    label: &'static str,
    kind: NumberKind,
    overridden: bool,
    input: Entity<InputState>,
    cx: &App,
) -> impl IntoElement {
    let field = input.clone();
    row(label, overridden, cx).child(
        div()
            .id(id)
            .flex_1()
            .min_w(px(0.))
            .on_prepaint(move |bounds, _, _| probe::record(id, bounds))
            .on_mouse_down(gpui_kit::MouseButton::Left, move |_, window, cx| {
                field.read(cx).focus_handle(cx).focus(window, cx);
            })
            .child(if matches!(kind, NumberKind::Text) {
                Input::new(&input).xsmall().w_full().into_any_element()
            } else {
                NumberInput::new(&input)
                    .xsmall()
                    .w_full()
                    .accessibility_label(label)
                    .into_any_element()
            }),
    )
}

pub(super) fn choice_row(
    id: &'static str,
    label: &'static str,
    overridden: bool,
    state: &Entity<SelectState<Vec<Choice>>>,
    cx: &App,
) -> impl IntoElement {
    row(label, overridden, cx).child(
        div()
            .id(id)
            .flex_1()
            .min_w(px(0.))
            .on_prepaint(move |bounds, _, _| probe::record(id, bounds))
            .child(Select::new(state).xsmall().w_full()),
    )
}

pub(super) fn font_row(
    id: &'static str,
    label: &'static str,
    overridden: bool,
    state: &Entity<SelectState<Vec<String>>>,
    cx: &App,
) -> impl IntoElement {
    row(label, overridden, cx).child(
        div()
            .id(id)
            .flex_1()
            .min_w(px(0.))
            .on_prepaint(move |bounds, _, _| probe::record(id, bounds))
            .child(Select::new(state).xsmall().w_full()),
    )
}

pub(super) fn color_row(
    label: &'static str,
    overridden: bool,
    state: &Entity<ColorPickerState>,
    cx: &App,
) -> impl IntoElement {
    let hex = state.read(cx).hex_input().clone();
    row(label, overridden, cx).child(
        h_flex()
            .flex_1()
            .min_w(px(0.))
            .gap_1()
            .items_center()
            .child(ColorPicker::new(state).xsmall())
            .child(Input::new(&hex).xsmall().flex_1()),
    )
}

pub(super) fn switch_row(
    id: &'static str,
    label: &'static str,
    checked: bool,
    overridden: bool,
    on_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    row(label, overridden, cx).child(
        Switch::new(id)
            .checked(checked)
            .xsmall()
            .accessibility_label(label)
            .on_change(on_change),
    )
}

pub(super) fn custom_editor(editor: Entity<EditorState>) -> impl IntoElement {
    let focus = editor.clone();
    div()
        .id("custom-typst")
        .w_full()
        .h(px(72.))
        .on_prepaint(|bounds, _, _| probe::record("custom-typst", bounds))
        .on_mouse_down(gpui_kit::MouseButton::Left, move |_, window, cx| {
            focus.read(cx).focus_handle(cx).focus(window, cx);
        })
        .child(Editor::new(&editor).h(px(72.)).bordered(true).w_full())
}

pub(super) fn column_header(style: Entity<StyleState>, cx: &App) -> impl IntoElement {
    h_flex()
        .h(px(36.))
        .px_2()
        .items_center()
        .justify_between()
        .flex_none()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("DOCUMENT SETTINGS"),
        )
        .child(
            div()
                .id("reset-style")
                .on_prepaint(|bounds, _, _| probe::record("reset-style", bounds))
                .child(
                    Button::new("reset-style-button")
                        .label("↺")
                        .ghost()
                        .xsmall()
                        .accessibility_label("Reset to preset")
                        .tooltip("Reset to preset")
                        .on_click(move |_, _, cx| {
                            style.update(cx, |style, cx| style.clear_all(cx));
                        }),
                ),
        )
}

fn row(label: &'static str, overridden: bool, cx: &App) -> gpui_kit::Div {
    h_flex()
        .w_full()
        .gap_2()
        .items_center()
        .child(dot(overridden, cx))
        .child(
            div()
                .w(px(108.))
                .flex_none()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
}

fn dot(overridden: bool, cx: &App) -> impl IntoElement {
    div()
        .w_2()
        .h_2()
        .flex_none()
        .rounded(cx.theme().radius_full())
        .when(overridden, |dot| dot.bg(cx.theme().accent))
}

pub(super) fn group(children: impl IntoIterator<Item = impl IntoElement>) -> impl IntoElement {
    v_flex().w_full().gap_1().children(children)
}
