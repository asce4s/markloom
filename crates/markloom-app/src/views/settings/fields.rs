//! Style paths, resolved display text, and the select rows.

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::select::SelectItem;
use gpui_kit::{
    App, InteractiveElement as _, IntoElement, ParentElement, SharedString, Styled, Window, div,
};

use markloom_style::{Length, StyleSheet};

use crate::probe;

#[derive(Clone, Copy)]
pub(super) enum NumberKind {
    Mm,
    Pt,
    Em,
    Float,
    Int,
    Text,
    Scale(usize),
}

pub(super) struct NumberSpec {
    pub id: &'static str,
    pub path: &'static str,
    pub label: &'static str,
    pub kind: NumberKind,
}

pub(super) const NUMBERS: &[NumberSpec] = &[
    NumberSpec {
        id: "margin-top",
        path: "page.margin.top",
        label: "Margin top",
        kind: NumberKind::Mm,
    },
    NumberSpec {
        id: "margin-right",
        path: "page.margin.right",
        label: "Margin right",
        kind: NumberKind::Mm,
    },
    NumberSpec {
        id: "margin-bottom",
        path: "page.margin.bottom",
        label: "Margin bottom",
        kind: NumberKind::Mm,
    },
    NumberSpec {
        id: "margin-left",
        path: "page.margin.left",
        label: "Margin left",
        kind: NumberKind::Mm,
    },
    NumberSpec {
        id: "toc-depth",
        path: "page.toc_depth",
        label: "TOC depth",
        kind: NumberKind::Int,
    },
    NumberSpec {
        id: "base-size",
        path: "typography.base_size",
        label: "Size",
        kind: NumberKind::Pt,
    },
    NumberSpec {
        id: "line-height",
        path: "typography.line_height",
        label: "Line height",
        kind: NumberKind::Float,
    },
    NumberSpec {
        id: "paragraph-spacing",
        path: "typography.paragraph_spacing",
        label: "Paragraph spacing",
        kind: NumberKind::Pt,
    },
    NumberSpec {
        id: "h1-scale",
        path: "typography.heading_scale",
        label: "H1",
        kind: NumberKind::Scale(0),
    },
    NumberSpec {
        id: "h2-scale",
        path: "typography.heading_scale",
        label: "H2",
        kind: NumberKind::Scale(1),
    },
    NumberSpec {
        id: "h3-scale",
        path: "typography.heading_scale",
        label: "H3",
        kind: NumberKind::Scale(2),
    },
    NumberSpec {
        id: "cell-padding",
        path: "table.cell_padding",
        label: "Cell padding",
        kind: NumberKind::Pt,
    },
    NumberSpec {
        id: "code-size",
        path: "code.font_size",
        label: "Font size",
        kind: NumberKind::Em,
    },
    NumberSpec {
        id: "diagram-scale",
        path: "diagram.scale",
        label: "Scale",
        kind: NumberKind::Float,
    },
    NumberSpec {
        id: "diagram-width",
        path: "diagram.max_width",
        label: "Max width",
        kind: NumberKind::Float,
    },
    NumberSpec {
        id: "image-width",
        path: "images.max_width",
        label: "Max width",
        kind: NumberKind::Float,
    },
    NumberSpec {
        id: "page-header",
        path: "page.header",
        label: "Header",
        kind: NumberKind::Text,
    },
    NumberSpec {
        id: "page-footer",
        path: "page.footer",
        label: "Footer",
        kind: NumberKind::Text,
    },
];

pub(super) const PAGE_PREFIXES: &[&str] = &[
    "page.size",
    "page.orientation",
    "page.margin",
    "colors.page_bg",
    "page.toc",
    "page.toc_depth",
    "page.break_before_h1",
    "typography.number_headings",
];

pub(super) const TYPE_PREFIXES: &[&str] = &[
    "typography.body_font",
    "typography.base_size",
    "typography.line_height",
    "typography.paragraph_spacing",
    "typography.justify",
    "colors.text",
    "colors.link",
];

pub(super) const HEADING_PREFIXES: &[&str] = &[
    "typography.heading_font",
    "typography.heading_weight",
    "typography.heading_scale",
    "colors.heading",
];

pub(super) const CODE_PREFIXES: &[&str] = &["code", "typography.code_font"];

pub(super) const HEADER_PREFIXES: &[&str] = &["page.header", "page.footer", "page.page_numbers"];

#[derive(Clone)]
pub(super) struct Choice {
    pub label: &'static str,
    pub value: &'static str,
    pub probe: &'static str,
}

impl SelectItem for Choice {
    type Value = &'static str;

    fn title(&self) -> SharedString {
        self.label.into()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let probe = self.probe;
        let label = self.label;
        div()
            .id(probe)
            .w_full()
            .on_prepaint(move |bounds, _, _| probe::record(probe, bounds))
            .child(label)
    }
}

pub(super) fn choices_for(id: &str) -> Vec<Choice> {
    match id {
        "page-size" => choices(
            "page-size",
            &[
                ("A4", "A4"),
                ("Letter", "Letter"),
                ("A5", "A5"),
                ("Legal", "Legal"),
            ],
        ),
        "orientation" => vec![
            Choice {
                label: "Portrait",
                value: "portrait",
                probe: "orientation-portrait",
            },
            Choice {
                label: "Landscape",
                value: "landscape",
                probe: "orientation-landscape",
            },
        ],
        "heading-weight" => choices(
            "heading-weight",
            &[
                ("Regular", "Regular"),
                ("Medium", "Medium"),
                ("Semibold", "Semibold"),
                ("Bold", "Bold"),
            ],
        ),
        "table-borders" => choices(
            "table-borders",
            &[
                ("Grid", "Grid"),
                ("Horizontal", "Horizontal"),
                ("Header only", "HeaderOnly"),
                ("None", "None"),
            ],
        ),
        "diagram-theme" => choices(
            "diagram-theme",
            &[
                ("Default", "Default"),
                ("Neutral", "Neutral"),
                ("Dark", "Dark"),
                ("Forest", "Forest"),
            ],
        ),
        "diagram-align" | "image-align" => choices(
            id,
            &[("Left", "Left"), ("Center", "Center"), ("Right", "Right")],
        ),
        "page-numbers" => choices(
            "page-numbers",
            &[
                ("None", "None"),
                ("Bottom center", "BottomCenter"),
                ("Bottom right", "BottomRight"),
            ],
        ),
        _ => Vec::new(),
    }
}

fn choices(_id: &str, pairs: &[(&'static str, &'static str)]) -> Vec<Choice> {
    pairs
        .iter()
        .map(|(label, value)| Choice {
            label,
            value,
            probe: value,
        })
        .collect()
}

pub(super) fn choice_value(sheet: &StyleSheet, id: &str) -> &'static str {
    match id {
        "page-size" => match sheet.page.size {
            markloom_style::PaperSize::A4 => "A4",
            markloom_style::PaperSize::Letter => "Letter",
            markloom_style::PaperSize::A5 => "A5",
            markloom_style::PaperSize::Legal => "Legal",
        },
        "orientation" => match sheet.page.orientation {
            markloom_style::Orientation::Portrait => "portrait",
            markloom_style::Orientation::Landscape => "landscape",
        },
        "heading-weight" => match sheet.typography.heading_weight {
            markloom_style::Weight::Regular => "Regular",
            markloom_style::Weight::Medium => "Medium",
            markloom_style::Weight::Semibold => "Semibold",
            markloom_style::Weight::Bold => "Bold",
        },
        "table-borders" => match sheet.table.borders {
            markloom_style::TableBorders::Grid => "Grid",
            markloom_style::TableBorders::Horizontal => "Horizontal",
            markloom_style::TableBorders::HeaderOnly => "HeaderOnly",
            markloom_style::TableBorders::None => "None",
        },
        "diagram-theme" => match sheet.diagram.theme {
            markloom_style::MermaidTheme::Default => "Default",
            markloom_style::MermaidTheme::Neutral => "Neutral",
            markloom_style::MermaidTheme::Dark => "Dark",
            markloom_style::MermaidTheme::Forest => "Forest",
        },
        "diagram-align" => align_name(sheet.diagram.align),
        "image-align" => align_name(sheet.images.align),
        "page-numbers" => match sheet.page.page_numbers {
            markloom_style::PageNumbers::None => "None",
            markloom_style::PageNumbers::BottomCenter => "BottomCenter",
            markloom_style::PageNumbers::BottomRight => "BottomRight",
        },
        _ => "",
    }
}

fn align_name(align: markloom_style::HAlign) -> &'static str {
    match align {
        markloom_style::HAlign::Left => "Left",
        markloom_style::HAlign::Center => "Center",
        markloom_style::HAlign::Right => "Right",
    }
}

pub(super) fn number_text(sheet: &StyleSheet, id: &str) -> String {
    match id {
        "margin-top" => trim(millimetres(sheet.page.margin.top)),
        "margin-right" => trim(millimetres(sheet.page.margin.right)),
        "margin-bottom" => trim(millimetres(sheet.page.margin.bottom)),
        "margin-left" => trim(millimetres(sheet.page.margin.left)),
        "toc-depth" => sheet.page.toc_depth.to_string(),
        "base-size" => trim(points(sheet.typography.base_size)),
        "line-height" => trim(sheet.typography.line_height),
        "paragraph-spacing" => trim(points(sheet.typography.paragraph_spacing)),
        "h1-scale" => trim(sheet.typography.heading_scale[0]),
        "h2-scale" => trim(sheet.typography.heading_scale[1]),
        "h3-scale" => trim(sheet.typography.heading_scale[2]),
        "cell-padding" => trim(points(sheet.table.cell_padding)),
        "code-size" => trim(length_number(sheet.code.font_size)),
        "diagram-scale" => trim(sheet.diagram.scale),
        "diagram-width" => trim(sheet.diagram.max_width),
        "image-width" => trim(sheet.images.max_width),
        "page-header" => sheet.page.header.clone(),
        "page-footer" => sheet.page.footer.clone(),
        _ => String::new(),
    }
}

pub(super) fn code_size_kind(sheet: &StyleSheet) -> NumberKind {
    match sheet.code.font_size {
        Length::Em(_) => NumberKind::Em,
        Length::Pt(_) => NumberKind::Pt,
    }
}

fn length_number(length: Length) -> f32 {
    match length {
        Length::Pt(points) => points,
        Length::Em(ems) => ems,
    }
}

fn points(length: Length) -> f32 {
    match length {
        Length::Pt(points) => points,
        Length::Em(ems) => ems,
    }
}

fn millimetres(length: Length) -> f32 {
    match length {
        Length::Pt(points) => points * 25.4 / 72.0,
        Length::Em(ems) => ems,
    }
}

pub(super) fn trim(value: f32) -> String {
    let value = (value * 100.0).round() / 100.0;
    if (value - value.round()).abs() < 0.001 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

pub(super) fn hex_color(color: markloom_style::Color) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

pub(super) fn color_hex(sheet: &StyleSheet, path: &str) -> Option<String> {
    let color = match path {
        "colors.page_bg" => Some(sheet.colors.page_bg),
        "colors.text" => Some(sheet.colors.text),
        "colors.link" => Some(sheet.colors.link),
        "colors.heading" => Some(sheet.colors.heading),
        "table.border_color" => Some(sheet.table.border_color),
        "table.header_bg" => sheet.table.header_bg,
        "table.header_text" => sheet.table.header_text,
        "table.zebra" => sheet.table.zebra,
        "code.background" => Some(sheet.code.background),
        "code.border" => sheet.code.border,
        _ => None,
    };
    color.map(hex_color)
}

pub(super) fn font_list(mut families: Vec<String>, sheet: &StyleSheet) -> Vec<String> {
    for font in sheet
        .typography
        .body_font
        .iter()
        .chain(&sheet.typography.heading_font)
        .chain(&sheet.typography.code_font)
    {
        if !families.iter().any(|name| name == font) {
            families.insert(0, font.clone());
        }
    }
    if families.is_empty() {
        families.push("Inter".into());
        families.push("JetBrains Mono".into());
    }
    families
}

pub(super) fn current_font(sheet: &StyleSheet, path: &str) -> String {
    let fonts = match path {
        "typography.heading_font" => &sheet.typography.heading_font,
        "typography.code_font" => &sheet.typography.code_font,
        _ => &sheet.typography.body_font,
    };
    fonts.first().cloned().unwrap_or_else(|| "Inter".into())
}
