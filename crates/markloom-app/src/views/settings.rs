//! Document settings column. Task 8 fills the body.

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    Context, Entity, InteractiveElement as _, IntoElement, ParentElement, Render, Styled, Window,
    div, px,
};

use crate::probe;
use crate::style_state::StyleState;

/// Header and an empty body. The header reset drops every override.
pub struct SettingsColumn {
    style: Entity<StyleState>,
}

impl SettingsColumn {
    pub fn new(style: Entity<StyleState>) -> Self {
        Self { style }
    }
}

impl Render for SettingsColumn {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let style = self.style.clone();
        v_flex()
            .id("settings")
            .on_prepaint(|bounds, _, _| probe::record("settings", bounds))
            .size_full()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .h(px(36.))
                    .px_3()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("DOCUMENT SETTINGS")
                    .child(Button::new("reset-style").label("↺").ghost().on_click(
                        move |_, _, cx| {
                            style.update(cx, |style, cx| style.clear_all(cx));
                        },
                    )),
            )
            .child(div().flex_1())
    }
}
