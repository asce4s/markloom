//! Document settings. Each control writes one style override.

mod fields;
mod rows;

use std::time::Duration;

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::input::{EditorState, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme as _, IndexPath, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement, Render, StatefulInteractiveElement as _, Styled, Subscription, Window, div, px,
};
use markloom_style::StyleSheet;
use toml::Value;

use crate::preview_state::PreviewState;
use crate::probe;
use crate::style_state::StyleState;

use fields::{
    CODE_PREFIXES, Choice, HEADER_PREFIXES, HEADING_PREFIXES, NUMBERS, NumberKind, PAGE_PREFIXES,
    TYPE_PREFIXES, choice_value, choices_for, code_size_kind, color_hex, current_font, font_list,
    number_text,
};
use rows::{
    choice_row, color_row, column_header, custom_editor, font_row, group, number_row, reset_row,
    switch_row,
};

struct NumberField {
    id: &'static str,
    path: &'static str,
    label: &'static str,
    kind: NumberKind,
    input: Entity<InputState>,
}

struct ChoiceField {
    id: &'static str,
    path: &'static str,
    label: &'static str,
    state: Entity<SelectState<Vec<Choice>>>,
}

struct FontField {
    id: &'static str,
    path: &'static str,
    label: &'static str,
    state: Entity<SelectState<Vec<String>>>,
}

struct ColorField {
    path: &'static str,
    label: &'static str,
    state: Entity<ColorPickerState>,
}

pub struct SettingsColumn {
    style: Entity<StyleState>,
    preview: Entity<PreviewState>,
    numbers: Vec<NumberField>,
    choices: Vec<ChoiceField>,
    fonts: Vec<FontField>,
    colors: Vec<ColorField>,
    font_names: Vec<String>,
    custom: Entity<EditorState>,
    custom_gen: u64,
    /// Page starts open. The rest stay closed until their title is clicked.
    open: [bool; 9],
    _subs: Vec<Subscription>,
}

impl SettingsColumn {
    pub fn new(
        style: Entity<StyleState>,
        preview: Entity<PreviewState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let sheet = style.read(cx).sheet().clone();
        let names = font_list(preview.read(cx).font_families(), &sheet);
        let mut subs = Vec::new();

        let numbers = NUMBERS
            .iter()
            .map(|spec| {
                let kind = if spec.id == "code-size" {
                    code_size_kind(&sheet)
                } else {
                    spec.kind
                };
                let input = cx.new(|cx| {
                    InputState::new(window, cx).default_value(number_text(&sheet, spec.id))
                });
                let id = spec.id;
                let path = spec.path;
                subs.push(cx.observe(&input, move |this, input, cx| {
                    let text = input.read(cx).value().to_string();
                    if text != number_text(this.style.read(cx).sheet(), id) {
                        this.apply_number(path, kind, &text, cx);
                    }
                }));
                NumberField {
                    id,
                    path,
                    label: spec.label,
                    kind,
                    input,
                }
            })
            .collect();

        let choices = choice_specs()
            .into_iter()
            .map(|(id, path, label)| {
                let items = choices_for(id);
                let selected = items
                    .iter()
                    .position(|item| item.value == choice_value(&sheet, id))
                    .map(IndexPath::new);
                let state = cx.new(|cx| SelectState::new(items, selected, window, cx));
                let stored = path;
                subs.push(cx.subscribe(
                    &state,
                    move |this, _, event: &SelectEvent<Vec<Choice>>, cx| {
                        let SelectEvent::Confirm(Some(value)) = event else {
                            return;
                        };
                        let value = (*value).to_string();
                        this.style.update(cx, |style, cx| {
                            style.set(stored, Value::String(value), cx);
                        });
                    },
                ));
                ChoiceField {
                    id,
                    path,
                    label,
                    state,
                }
            })
            .collect();

        let fonts = font_specs()
            .into_iter()
            .map(|(id, path, label)| {
                let current = current_font(&sheet, path);
                let selected = names
                    .iter()
                    .position(|name| name == &current)
                    .map(IndexPath::new);
                let state = cx.new(|cx| SelectState::new(names.clone(), selected, window, cx));
                subs.push(cx.subscribe(
                    &state,
                    move |this, _, event: &SelectEvent<Vec<String>>, cx| {
                        let SelectEvent::Confirm(Some(value)) = event else {
                            return;
                        };
                        let value = Value::Array(vec![Value::String(value.clone())]);
                        this.style
                            .update(cx, |style, cx| style.set(path, value, cx));
                    },
                ));
                FontField {
                    id,
                    path,
                    label,
                    state,
                }
            })
            .collect();

        let colors = color_specs()
            .into_iter()
            .map(|(path, label)| {
                let state = cx.new(|cx| {
                    let mut picker = ColorPickerState::new(window, cx);
                    if let Some(hex) = color_hex(&sheet, path)
                        && let Some(color) = hsla_from_hex(&hex)
                    {
                        picker = picker.default_value(color);
                    }
                    picker
                });
                subs.push(
                    cx.subscribe(&state, move |this, _, event: &ColorPickerEvent, cx| {
                        let ColorPickerEvent::Change(Some(color)) = event else {
                            return;
                        };
                        let hex = hex_of(color);
                        this.style.update(cx, |style, cx| {
                            style.set(path, Value::String(hex), cx);
                        });
                    }),
                );
                ColorField { path, label, state }
            })
            .collect();

        let custom =
            cx.new(|cx| EditorState::new(window, cx).default_value(sheet.custom.typst.clone()));
        subs.push(cx.subscribe(&custom, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.schedule_custom(cx);
            }
        }));
        subs.push(cx.observe(&style, |_, _, cx| cx.notify()));
        subs.push(cx.observe(&preview, |this, preview, cx| {
            let names = font_list(
                preview.read(cx).font_families(),
                this.style.read(cx).sheet(),
            );
            if names != this.font_names {
                cx.notify();
            }
        }));

        Self {
            style,
            preview,
            numbers,
            choices,
            fonts,
            colors,
            font_names: names,
            custom,
            custom_gen: 0,
            open: [true, false, false, false, false, false, false, false, false],
            _subs: subs,
        }
    }

    fn apply_number(&mut self, path: &str, kind: NumberKind, text: &str, cx: &mut Context<Self>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let value = match kind {
            NumberKind::Text => Value::String(text.to_string()),
            NumberKind::Mm => {
                if text.parse::<f32>().is_err() {
                    return;
                }
                Value::String(format!("{text}mm"))
            }
            NumberKind::Pt => {
                if text.parse::<f32>().is_err() {
                    return;
                }
                Value::String(format!("{text}pt"))
            }
            NumberKind::Em => {
                if text.parse::<f32>().is_err() {
                    return;
                }
                Value::String(format!("{text}em"))
            }
            NumberKind::Float => {
                let Ok(number) = text.parse::<f64>() else {
                    return;
                };
                Value::Float(number)
            }
            NumberKind::Int => {
                let Ok(number) = text.parse::<i64>() else {
                    return;
                };
                Value::Integer(number)
            }
            NumberKind::Scale(index) => {
                let Ok(number) = text.parse::<f32>() else {
                    return;
                };
                let mut scale = self.style.read(cx).sheet().typography.heading_scale;
                if index >= scale.len() {
                    return;
                }
                scale[index] = number;
                Value::Array(
                    scale
                        .into_iter()
                        .map(|n| Value::Float(f64::from(n)))
                        .collect(),
                )
            }
        };
        self.style
            .update(cx, |style, cx| style.set(path, value, cx));
    }

    fn schedule_custom(&mut self, cx: &mut Context<Self>) {
        self.custom_gen = self.custom_gen.wrapping_add(1);
        let generation = self.custom_gen;
        let editor = self.custom.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.custom_gen != generation {
                    return;
                }
                let text = editor.read(cx).value().to_string();
                this.style.update(cx, |style, cx| {
                    style.set("custom.typst", Value::String(text), cx);
                });
            });
        })
        .detach();
    }

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sheet = self.style.read(cx).sheet().clone();
        for field in &self.numbers {
            if field.input.read(cx).focus_handle(cx).is_focused(window) {
                continue;
            }
            let next = number_text(&sheet, field.id);
            if field.input.read(cx).value().as_ref() != next {
                field.input.update(cx, |input, cx| {
                    input.set_value(next, window, cx);
                });
            }
        }
        for field in &self.choices {
            let want = choice_value(&sheet, field.id);
            if field.state.read(cx).selected_value().copied() == Some(want) {
                continue;
            }
            field.state.update(cx, |state, cx| {
                state.set_selected_value(&want, window, cx);
            });
        }
        let names = font_list(self.preview.read(cx).font_families(), &sheet);
        if names != self.font_names {
            self.font_names = names.clone();
            for field in &self.fonts {
                field.state.update(cx, |state, cx| {
                    state.set_items(names.clone(), window, cx);
                });
            }
        }
        for field in &self.fonts {
            let want = current_font(&sheet, field.path);
            if field.state.read(cx).selected_value().map(String::as_str) == Some(want.as_str()) {
                continue;
            }
            field.state.update(cx, |state, cx| {
                state.set_selected_value(&want, window, cx);
            });
        }
        for field in &self.colors {
            let hex_input = field.state.read(cx).hex_input().clone();
            if hex_input.read(cx).focus_handle(cx).is_focused(window) {
                continue;
            }
            let want = color_hex(&sheet, field.path);
            let have = field.state.read(cx).value().as_ref().map(hex_of);
            if want == have {
                continue;
            }
            field.state.update(cx, |state, cx| match want.clone() {
                Some(hex) => {
                    if let Some(color) = hsla_from_hex(&hex) {
                        state.set_value(color, window, cx);
                    }
                }
                None => state.clear_value(window, cx),
            });
        }
    }
}

impl Render for SettingsColumn {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync(window, cx);
        let sheet = self.style.read(cx).sheet().clone();
        let style = self.style.clone();
        let open = self.open;
        v_flex()
            .id("settings")
            .size_full()
            .bg(cx.theme().background)
            .on_prepaint(|bounds, _, _| probe::record("settings", bounds))
            .child(column_header(style.clone(), cx))
            .child(
                v_flex()
                    .id("settings-body")
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_y_scrollbar()
                    .on_prepaint(|bounds, _, _| probe::record("settings-body", bounds))
                    .child(fold(
                        open[0],
                        "section-page",
                        "Page",
                        page_section(
                            &self.numbers,
                            &self.choices,
                            &self.colors,
                            &sheet,
                            style.clone(),
                            cx,
                        ),
                        cx.listener(|this, _, _, cx| this.toggle(0, cx)),
                        cx,
                    ))
                    .child(fold(
                        open[1],
                        "section-type",
                        "Typography",
                        type_section(
                            &self.numbers,
                            &self.fonts,
                            &self.colors,
                            &sheet,
                            style.clone(),
                            cx,
                        ),
                        cx.listener(|this, _, _, cx| this.toggle(1, cx)),
                        cx,
                    ))
                    .child(fold(
                        open[2],
                        "section-headings",
                        "Headings",
                        heading_section(
                            &self.numbers,
                            &self.choices,
                            &self.fonts,
                            &self.colors,
                            &sheet,
                            style.clone(),
                            cx,
                        ),
                        cx.listener(|this, _, _, cx| this.toggle(2, cx)),
                        cx,
                    ))
                    .child(fold(
                        open[3],
                        "section-tables",
                        "Tables",
                        table_section(
                            &self.numbers,
                            &self.choices,
                            &self.colors,
                            &sheet,
                            style.clone(),
                            cx,
                        ),
                        cx.listener(|this, _, _, cx| this.toggle(3, cx)),
                        cx,
                    ))
                    .child(fold(
                        open[4],
                        "section-code",
                        "Code blocks",
                        code_section(
                            &self.numbers,
                            &self.fonts,
                            &self.colors,
                            &sheet,
                            style.clone(),
                            cx,
                        ),
                        cx.listener(|this, _, _, cx| this.toggle(4, cx)),
                        cx,
                    ))
                    .child(fold(
                        open[5],
                        "section-diagrams",
                        "Diagrams",
                        diagram_section(&self.numbers, &self.choices, &sheet, style.clone(), cx),
                        cx.listener(|this, _, _, cx| this.toggle(5, cx)),
                        cx,
                    ))
                    .child(fold(
                        open[6],
                        "section-images",
                        "Images",
                        image_section(&self.numbers, &self.choices, &sheet, style.clone(), cx),
                        cx.listener(|this, _, _, cx| this.toggle(6, cx)),
                        cx,
                    ))
                    .child(fold(
                        open[7],
                        "section-header",
                        "Header & footer",
                        header_section(&self.numbers, &self.choices, &sheet, style.clone(), cx),
                        cx.listener(|this, _, _, cx| this.toggle(7, cx)),
                        cx,
                    ))
                    .child(fold(
                        open[8],
                        "section-custom",
                        "Custom Typst",
                        group([
                            custom_editor(self.custom.clone()).into_any_element(),
                            reset_row("reset-custom", style.clone(), &["custom"])
                                .into_any_element(),
                        ]),
                        cx.listener(|this, _, _, cx| this.toggle(8, cx)),
                        cx,
                    )),
            )
    }
}

impl SettingsColumn {
    fn toggle(&mut self, index: usize, cx: &mut Context<Self>) {
        self.open[index] = !self.open[index];
        cx.notify();
    }
}

fn fold(
    open: bool,
    id: &'static str,
    title: &'static str,
    body: impl IntoElement,
    toggle: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> gpui_kit::AnyElement {
    v_flex()
        .w_full()
        .child(
            div()
                .id(id)
                .w_full()
                .py_1()
                .px_2()
                .text_sm()
                .text_color(cx.theme().foreground)
                .on_prepaint(move |bounds, _, _| probe::record(id, bounds))
                .on_click(toggle)
                .child(title),
        )
        .when(open, |row| row.child(body))
        .into_any_element()
}

fn page_section(
    numbers: &[NumberField],
    choices: &[ChoiceField],
    colors: &[ColorField],
    sheet: &StyleSheet,
    style: Entity<StyleState>,
    cx: &App,
) -> impl IntoElement {
    group([
        choice(choices, "page-size", &style, cx).into_any_element(),
        choice(choices, "orientation", &style, cx).into_any_element(),
        number(numbers, "margin-top", &style, cx).into_any_element(),
        number(numbers, "margin-right", &style, cx).into_any_element(),
        number(numbers, "margin-bottom", &style, cx).into_any_element(),
        number(numbers, "margin-left", &style, cx).into_any_element(),
        color(colors, "colors.page_bg", &style, cx).into_any_element(),
        flag(
            style.clone(),
            "page.toc",
            "Table of contents",
            sheet.page.toc,
            cx,
        )
        .into_any_element(),
        number(numbers, "toc-depth", &style, cx).into_any_element(),
        flag(
            style.clone(),
            "typography.number_headings",
            "Number headings",
            sheet.typography.number_headings,
            cx,
        )
        .into_any_element(),
        flag(
            style.clone(),
            "page.break_before_h1",
            "New page before H1",
            sheet.page.break_before_h1,
            cx,
        )
        .into_any_element(),
        reset_row("reset-page", style, PAGE_PREFIXES).into_any_element(),
    ])
}

fn type_section(
    numbers: &[NumberField],
    fonts: &[FontField],
    colors: &[ColorField],
    sheet: &StyleSheet,
    style: Entity<StyleState>,
    cx: &App,
) -> impl IntoElement {
    group([
        font(fonts, "body-font", &style, cx).into_any_element(),
        number(numbers, "base-size", &style, cx).into_any_element(),
        number(numbers, "line-height", &style, cx).into_any_element(),
        number(numbers, "paragraph-spacing", &style, cx).into_any_element(),
        flag(
            style.clone(),
            "typography.justify",
            "Justify",
            sheet.typography.justify,
            cx,
        )
        .into_any_element(),
        color(colors, "colors.text", &style, cx).into_any_element(),
        color(colors, "colors.link", &style, cx).into_any_element(),
        reset_row("reset-type", style, TYPE_PREFIXES).into_any_element(),
    ])
}

fn heading_section(
    numbers: &[NumberField],
    choices: &[ChoiceField],
    fonts: &[FontField],
    colors: &[ColorField],
    _sheet: &StyleSheet,
    style: Entity<StyleState>,
    cx: &App,
) -> impl IntoElement {
    group([
        font(fonts, "heading-font", &style, cx).into_any_element(),
        choice(choices, "heading-weight", &style, cx).into_any_element(),
        number(numbers, "h1-scale", &style, cx).into_any_element(),
        number(numbers, "h2-scale", &style, cx).into_any_element(),
        number(numbers, "h3-scale", &style, cx).into_any_element(),
        color(colors, "colors.heading", &style, cx).into_any_element(),
        reset_row("reset-headings", style, HEADING_PREFIXES).into_any_element(),
    ])
}

fn table_section(
    numbers: &[NumberField],
    choices: &[ChoiceField],
    colors: &[ColorField],
    sheet: &StyleSheet,
    style: Entity<StyleState>,
    cx: &App,
) -> impl IntoElement {
    let zebra_on = sheet.table.zebra.is_some();
    group([
        choice(choices, "table-borders", &style, cx).into_any_element(),
        color(colors, "table.border_color", &style, cx).into_any_element(),
        color(colors, "table.header_bg", &style, cx).into_any_element(),
        color(colors, "table.header_text", &style, cx).into_any_element(),
        flag(
            style.clone(),
            "table.header_bold",
            "Header bold",
            sheet.table.header_bold,
            cx,
        )
        .into_any_element(),
        zebra(style.clone(), zebra_on, cx).into_any_element(),
        color(colors, "table.zebra", &style, cx).into_any_element(),
        number(numbers, "cell-padding", &style, cx).into_any_element(),
        reset_row("reset-tables", style, &["table"]).into_any_element(),
    ])
}

fn code_section(
    numbers: &[NumberField],
    fonts: &[FontField],
    colors: &[ColorField],
    sheet: &StyleSheet,
    style: Entity<StyleState>,
    cx: &App,
) -> impl IntoElement {
    group([
        flag(
            style.clone(),
            "code.highlight",
            "Highlight",
            sheet.code.highlight,
            cx,
        )
        .into_any_element(),
        color(colors, "code.background", &style, cx).into_any_element(),
        color(colors, "code.border", &style, cx).into_any_element(),
        number(numbers, "code-size", &style, cx).into_any_element(),
        font(fonts, "code-font", &style, cx).into_any_element(),
        reset_row("reset-code", style, CODE_PREFIXES).into_any_element(),
    ])
}

fn diagram_section(
    numbers: &[NumberField],
    choices: &[ChoiceField],
    _sheet: &StyleSheet,
    style: Entity<StyleState>,
    cx: &App,
) -> impl IntoElement {
    group([
        choice(choices, "diagram-theme", &style, cx).into_any_element(),
        number(numbers, "diagram-scale", &style, cx).into_any_element(),
        number(numbers, "diagram-width", &style, cx).into_any_element(),
        choice(choices, "diagram-align", &style, cx).into_any_element(),
        reset_row("reset-diagrams", style, &["diagram"]).into_any_element(),
    ])
}

fn image_section(
    numbers: &[NumberField],
    choices: &[ChoiceField],
    _sheet: &StyleSheet,
    style: Entity<StyleState>,
    cx: &App,
) -> impl IntoElement {
    group([
        number(numbers, "image-width", &style, cx).into_any_element(),
        choice(choices, "image-align", &style, cx).into_any_element(),
        reset_row("reset-images", style, &["images"]).into_any_element(),
    ])
}

fn header_section(
    numbers: &[NumberField],
    choices: &[ChoiceField],
    _sheet: &StyleSheet,
    style: Entity<StyleState>,
    cx: &App,
) -> impl IntoElement {
    group([
        number(numbers, "page-header", &style, cx).into_any_element(),
        number(numbers, "page-footer", &style, cx).into_any_element(),
        choice(choices, "page-numbers", &style, cx).into_any_element(),
        reset_row("reset-header", style, HEADER_PREFIXES).into_any_element(),
    ])
}

fn number(
    fields: &[NumberField],
    id: &str,
    style: &Entity<StyleState>,
    cx: &App,
) -> gpui_kit::AnyElement {
    let field = fields
        .iter()
        .find(|field| field.id == id)
        .expect("number field");
    let overridden = style.read(cx).is_overridden(field.path);
    number_row(
        field.id,
        field.label,
        field.kind,
        overridden,
        field.input.clone(),
        cx,
    )
    .into_any_element()
}

fn choice(
    fields: &[ChoiceField],
    id: &str,
    style: &Entity<StyleState>,
    cx: &App,
) -> gpui_kit::AnyElement {
    let field = fields
        .iter()
        .find(|field| field.id == id)
        .expect("choice field");
    let overridden = style.read(cx).is_overridden(field.path);
    choice_row(field.id, field.label, overridden, &field.state, cx).into_any_element()
}

fn font(
    fields: &[FontField],
    id: &str,
    style: &Entity<StyleState>,
    cx: &App,
) -> gpui_kit::AnyElement {
    let field = fields
        .iter()
        .find(|field| field.id == id)
        .expect("font field");
    let overridden = style.read(cx).is_overridden(field.path);
    font_row(field.id, field.label, overridden, &field.state, cx).into_any_element()
}

fn color(
    fields: &[ColorField],
    path: &str,
    style: &Entity<StyleState>,
    cx: &App,
) -> gpui_kit::AnyElement {
    let field = fields
        .iter()
        .find(|field| field.path == path)
        .expect("color field");
    let overridden = style.read(cx).is_overridden(field.path);
    color_row(field.label, overridden, &field.state, cx).into_any_element()
}

fn flag(
    style: Entity<StyleState>,
    path: &'static str,
    label: &'static str,
    checked: bool,
    cx: &App,
) -> impl IntoElement {
    let id = path;
    let overridden = style.read(cx).is_overridden(path);
    switch_row(
        id,
        label,
        checked,
        overridden,
        move |next, _, cx| {
            style.update(cx, |style, cx| {
                style.set(path, Value::Boolean(*next), cx);
            });
        },
        cx,
    )
}

fn zebra(style: Entity<StyleState>, checked: bool, cx: &App) -> impl IntoElement {
    let overridden = style.read(cx).is_overridden("table.zebra");
    switch_row(
        "table.zebra",
        "Zebra",
        checked,
        overridden,
        move |next, _, cx| {
            let value = if *next {
                Value::String("#f6f8fa".into())
            } else {
                Value::String("none".into())
            };
            style.update(cx, |style, cx| style.set("table.zebra", value, cx));
        },
        cx,
    )
}

fn choice_specs() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("page-size", "page.size", "Paper size"),
        ("orientation", "page.orientation", "Orientation"),
        ("heading-weight", "typography.heading_weight", "Weight"),
        ("table-borders", "table.borders", "Borders"),
        ("diagram-theme", "diagram.theme", "Theme"),
        ("diagram-align", "diagram.align", "Align"),
        ("image-align", "images.align", "Align"),
        ("page-numbers", "page.page_numbers", "Page numbers"),
    ]
}

fn font_specs() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("body-font", "typography.body_font", "Body font"),
        ("heading-font", "typography.heading_font", "Font"),
        ("code-font", "typography.code_font", "Font"),
    ]
}

fn color_specs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("colors.page_bg", "Background"),
        ("colors.text", "Text"),
        ("colors.link", "Link"),
        ("colors.heading", "Color"),
        ("table.border_color", "Border"),
        ("table.header_bg", "Header background"),
        ("table.header_text", "Header text"),
        ("table.zebra", "Zebra color"),
        ("code.background", "Background"),
        ("code.border", "Border"),
    ]
}

fn hsla_from_hex(hex: &str) -> Option<gpui_kit::Hsla> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some(gpui_kit::Hsla::from(gpui_kit::rgb(n)))
}

fn hex_of(color: &gpui_kit::Hsla) -> String {
    let rgb = color.to_rgb();
    format!(
        "#{:02x}{:02x}{:02x}",
        (rgb.r * 255.0).round() as u8,
        (rgb.g * 255.0).round() as u8,
        (rgb.b * 255.0).round() as u8
    )
}
