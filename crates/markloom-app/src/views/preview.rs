//! Page column. Scroll position is the list's, not the caret's.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    App, Entity, ImageSource, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, UniformListScrollHandle, Window, div, img, px,
    uniform_list,
};

use crate::document::Document;
use crate::preview_pipeline::{self, Zoom, visible_span};
use crate::preview_state::PreviewState;
use crate::probe;
use crate::style_state::StyleState;
use crate::views::status::{self, paper_label, preview_text};

const PAGE_GAP: f32 = 24.0;

pub fn view(
    document: Entity<Document>,
    style: Entity<StyleState>,
    preview: Entity<PreviewState>,
    scroll: UniformListScrollHandle,
    on_sync: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let count = preview.read(cx).page_count().unwrap_or(0);
    let (left, danger) = preview_text(preview.read(cx).status());
    let paper = paper_label(style.read(cx).sheet());
    let paper_probe = paper.clone();
    v_flex()
        .size_full()
        .bg(cx.theme().background)
        .on_prepaint(move |_, _, _| probe::set_text("preview-paper", paper_probe.clone()))
        .child(header(preview.clone(), on_sync, cx))
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .bg(cx.theme().muted)
                .on_prepaint({
                    let preview = preview.clone();
                    let scroll = scroll.clone();
                    move |bounds, window, cx| {
                        let width = bounds.size.width.as_f32();
                        let height = bounds.size.height.as_f32();
                        let span = visible_pages(&scroll, count);
                        window.defer(cx, move |_, cx| {
                            preview.update(cx, |preview, cx| {
                                preview.set_viewport(width, height);
                                if let Some(span) = span {
                                    preview.set_visible(span, cx);
                                }
                            });
                        });
                    }
                })
                .child(page_list(document, preview, scroll, count)),
        )
        .child(status::bar(left, paper, danger, cx))
}

fn header(
    preview: Entity<PreviewState>,
    on_sync: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let zoom = preview.read(cx).zoom();
    h_flex()
        .h(px(36.))
        .px_3()
        .gap_1()
        .items_center()
        .justify_between()
        .child(
            Button::new("sync-scroll")
                .label("⇅")
                .ghost()
                .on_click(move |_, window, cx| on_sync(window, cx)),
        )
        .child(
            h_flex()
                .gap_1()
                .child(zoom_button("zoom-out", "−", preview.clone(), false))
                .child(zoom_menu(preview.clone(), zoom))
                .child(zoom_button("zoom-in", "+", preview, true)),
        )
}

fn zoom_button(
    id: &'static str,
    label: &'static str,
    preview: Entity<PreviewState>,
    larger: bool,
) -> impl IntoElement {
    Button::new(id)
        .label(label)
        .ghost()
        .on_click(move |_, _, cx| {
            preview.update(cx, |preview, cx| {
                if larger {
                    preview.zoom_in(cx);
                } else {
                    preview.zoom_out(cx);
                }
            });
        })
}

fn zoom_menu(preview: Entity<PreviewState>, zoom: Zoom) -> impl IntoElement {
    let label = match zoom {
        Zoom::FitWidth => "Fit width".to_string(),
        Zoom::FitPage => "Fit page".to_string(),
        Zoom::Percent(percent) => format!("{percent:.0}%"),
    };
    Button::new("zoom-menu")
        .label(label)
        .ghost()
        .dropdown_menu(move |menu, _, _| {
            let fit_width = preview.clone();
            let fit_page = preview.clone();
            let mut menu = menu
                .item(PopupMenuItem::new("Fit width").on_click(move |_, _, cx| {
                    fit_width.update(cx, |preview, cx| preview.set_zoom(Zoom::FitWidth, cx));
                }))
                .item(PopupMenuItem::new("Fit page").on_click(move |_, _, cx| {
                    fit_page.update(cx, |preview, cx| preview.set_zoom(Zoom::FitPage, cx));
                }));
            for percent in preview_pipeline::ZOOM_STEPS {
                let preview = preview.clone();
                menu = menu.item(PopupMenuItem::new(format!("{percent:.0}%")).on_click(
                    move |_, _, cx| {
                        preview.update(cx, |preview, cx| {
                            preview.set_zoom(Zoom::Percent(percent), cx);
                        });
                    },
                ));
            }
            menu
        })
}

fn page_list(
    document: Entity<Document>,
    preview: Entity<PreviewState>,
    scroll: UniformListScrollHandle,
    count: usize,
) -> impl IntoElement {
    uniform_list("pages", count, move |range, _, cx| {
        let mut rows = Vec::new();
        for index in range {
            rows.push(page_row(index, document.clone(), preview.clone(), cx));
        }
        rows
    })
    .track_scroll(&scroll)
    .size_full()
}

fn page_row(
    index: usize,
    document: Entity<Document>,
    preview: Entity<PreviewState>,
    cx: &App,
) -> gpui_kit::AnyElement {
    let (width, height) = preview
        .read(cx)
        .page_display_px(index)
        .unwrap_or((420.0, 594.0));
    let image = preview.read(cx).page_image(index);
    let top = Rc::new(Cell::new(0.0f32));
    let record = top.clone();
    div()
        .w_full()
        .h(px(height + PAGE_GAP))
        .pb(px(PAGE_GAP))
        .flex()
        .flex_row()
        .justify_center()
        .child(
            div()
                .id(("page", index))
                .w(px(width))
                .h(px(height))
                .flex_none()
                .bg(cx.theme().popover)
                .shadow_md()
                .on_prepaint(move |bounds, _, _| record.set(bounds.origin.y.as_f32()))
                .on_click(move |event, window, cx| {
                    let y_px = (event.position().y.as_f32() - top.get()).max(0.0);
                    let Some(y_pt) = preview.read(cx).y_pt(index, y_px) else {
                        return;
                    };
                    let Some(line) = preview.read(cx).line_at(index, y_pt) else {
                        return;
                    };
                    document.update(cx, |document, cx| {
                        document.set_cursor(line, 1, window, cx);
                    });
                })
                .child(match image {
                    Some(image) => img(ImageSource::Render(image))
                        .w(px(width))
                        .h(px(height))
                        .into_any_element(),
                    None => div().size_full().into_any_element(),
                }),
        )
        .into_any_element()
}

fn visible_pages(handle: &UniformListScrollHandle, count: usize) -> Option<(usize, usize)> {
    if count == 0 {
        return None;
    }
    let state = handle.0.borrow();
    let size = state.last_item_size?;
    let item_h = size.contents.height.as_f32() / count as f32;
    let viewport = size.item.height.as_f32();
    let offset = -state.base_handle.offset().y.as_f32();
    Some(visible_span(offset, viewport, item_h, count))
}
