//! Github-default style sheet. Missing TOML keys keep these defaults.

use serde::Deserializer;
use serde::{Deserialize, Serialize};

use crate::values::{Color, Length, deserialize_optional_color};

/// A resolved style. [`Default`] is the github look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct StyleSheet {
    pub page: PageStyle,
    pub typography: Typography,
    pub colors: Colors,
    pub code: CodeStyle,
    pub table: TableStyle,
    pub blockquote: QuoteStyle,
    pub lists: ListStyle,
    pub diagram: DiagramStyle,
}

impl StyleSheet {
    /// Check the numeric ranges the spec requires.
    pub fn validate(&self) -> Result<(), StyleError> {
        if !(1.0..=3.0).contains(&self.typography.line_height) {
            return Err(StyleError::Range("line_height must be 1.0–3.0".into()));
        }
        if !(0.1..=1.0).contains(&self.diagram.max_width) {
            return Err(StyleError::Range(
                "diagram.max_width must be 0.1–1.0".into(),
            ));
        }
        if !(0.25..=4.0).contains(&self.diagram.scale) {
            return Err(StyleError::Range("diagram.scale must be 0.25–4.0".into()));
        }
        match self.typography.base_size {
            Length::Pt(points) if (6.0..=32.0).contains(&points) => Ok(()),
            _ => Err(StyleError::Range("base_size must be 6–32pt".into())),
        }
    }
}

/// A style value failed to parse or failed a range check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleError {
    Range(String),
    Message(String),
}

impl std::fmt::Display for StyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StyleError::Range(message) | StyleError::Message(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StyleError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PaperSize {
    #[default]
    A4,
    Letter,
    A5,
    Legal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PageNumbers {
    None,
    #[default]
    BottomCenter,
    BottomRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct PageStyle {
    pub size: PaperSize,
    #[serde(default, deserialize_with = "deserialize_margin")]
    pub margin: Margins,
    pub header: String,
    pub footer: String,
    pub page_numbers: PageNumbers,
    pub toc: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Margins {
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
}

impl Default for Margins {
    fn default() -> Self {
        let side = two_cm();
        Self {
            top: side,
            right: side,
            bottom: side,
            left: side,
        }
    }
}

fn deserialize_margin<'de, D>(deserializer: D) -> Result<Margins, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Input {
        All(Length),
        Sides(MarginSides),
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct MarginSides {
        #[serde(default = "two_cm")]
        top: Length,
        #[serde(default = "two_cm")]
        right: Length,
        #[serde(default = "two_cm")]
        bottom: Length,
        #[serde(default = "two_cm")]
        left: Length,
    }
    match Input::deserialize(deserializer)? {
        Input::All(length) => Ok(Margins {
            top: length,
            right: length,
            bottom: length,
            left: length,
        }),
        Input::Sides(sides) => Ok(Margins {
            top: sides.top,
            right: sides.right,
            bottom: sides.bottom,
            left: sides.left,
        }),
    }
}

fn two_cm() -> Length {
    Length::Pt(2.0 * 72.0 / 2.54)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Weight {
    Regular,
    Medium,
    #[default]
    Semibold,
    Bold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Typography {
    #[serde(default = "default_body_font")]
    pub body_font: Vec<String>,
    #[serde(default = "default_body_font")]
    pub heading_font: Vec<String>,
    #[serde(default = "default_code_font")]
    pub code_font: Vec<String>,
    #[serde(default = "default_base_size")]
    pub base_size: Length,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    #[serde(default = "default_paragraph_spacing")]
    pub paragraph_spacing: Length,
    #[serde(default = "default_heading_scale")]
    pub heading_scale: [f32; 6],
    pub heading_weight: Weight,
    pub justify: bool,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            body_font: default_body_font(),
            heading_font: default_body_font(),
            code_font: default_code_font(),
            base_size: default_base_size(),
            line_height: default_line_height(),
            paragraph_spacing: default_paragraph_spacing(),
            heading_scale: default_heading_scale(),
            heading_weight: Weight::Semibold,
            justify: false,
        }
    }
}

fn default_body_font() -> Vec<String> {
    vec![
        "Inter".into(),
        "Liberation Sans".into(),
        "DejaVu Sans".into(),
    ]
}

fn default_code_font() -> Vec<String> {
    vec!["JetBrains Mono".into(), "DejaVu Sans Mono".into()]
}

fn default_base_size() -> Length {
    Length::Pt(11.0)
}

fn default_line_height() -> f32 {
    1.6
}

fn default_paragraph_spacing() -> Length {
    Length::Pt(10.0)
}

fn default_heading_scale() -> [f32; 6] {
    [2.0, 1.5, 1.25, 1.0, 0.875, 0.85]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    #[serde(default = "default_text")]
    pub text: Color,
    #[serde(default = "default_text")]
    pub heading: Color,
    #[serde(default = "default_link")]
    pub link: Color,
    #[serde(default = "default_muted")]
    pub muted: Color,
    #[serde(default = "default_white")]
    pub page_bg: Color,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            text: default_text(),
            heading: default_text(),
            link: default_link(),
            muted: default_muted(),
            page_bg: default_white(),
        }
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b, a: 255 }
}

fn default_text() -> Color {
    rgb(0x1f, 0x23, 0x28)
}

fn default_link() -> Color {
    rgb(0x09, 0x69, 0xda)
}

fn default_muted() -> Color {
    rgb(0x59, 0x63, 0x6e)
}

fn default_white() -> Color {
    rgb(0xff, 0xff, 0xff)
}

fn default_code_bg() -> Color {
    rgb(0xf6, 0xf8, 0xfa)
}

fn default_border() -> Color {
    rgb(0xd1, 0xd9, 0xe0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CodeStyle {
    pub highlight: bool,
    #[serde(default = "default_code_bg")]
    pub background: Color,
    #[serde(default, deserialize_with = "deserialize_optional_color")]
    pub border: Option<Color>,
    #[serde(default = "default_code_font_size")]
    pub font_size: Length,
    #[serde(default = "default_code_radius")]
    pub radius: Length,
    #[serde(default = "default_code_inset")]
    pub inset: Length,
}

impl Default for CodeStyle {
    fn default() -> Self {
        Self {
            highlight: true,
            background: default_code_bg(),
            border: None,
            font_size: default_code_font_size(),
            radius: default_code_radius(),
            inset: default_code_inset(),
        }
    }
}

fn default_code_font_size() -> Length {
    Length::Em(0.85)
}

fn default_code_radius() -> Length {
    Length::Pt(6.0)
}

fn default_code_inset() -> Length {
    Length::Pt(10.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TableBorders {
    #[default]
    Grid,
    Horizontal,
    HeaderOnly,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TableStyle {
    pub borders: TableBorders,
    #[serde(default = "default_border")]
    pub border_color: Color,
    #[serde(default = "default_border_width")]
    pub border_width: Length,
    #[serde(
        default = "default_header_bg",
        deserialize_with = "deserialize_optional_color"
    )]
    pub header_bg: Option<Color>,
    pub header_bold: bool,
    #[serde(
        default = "default_header_bg",
        deserialize_with = "deserialize_optional_color"
    )]
    pub zebra: Option<Color>,
    #[serde(default = "default_cell_padding")]
    pub cell_padding: Length,
    #[serde(default = "default_table_font_size")]
    pub font_size: Length,
    pub full_width: bool,
}

impl Default for TableStyle {
    fn default() -> Self {
        Self {
            borders: TableBorders::Grid,
            border_color: default_border(),
            border_width: default_border_width(),
            header_bg: default_header_bg(),
            header_bold: true,
            zebra: default_header_bg(),
            cell_padding: default_cell_padding(),
            font_size: default_table_font_size(),
            full_width: false,
        }
    }
}

fn default_border_width() -> Length {
    Length::Pt(0.75)
}

fn default_header_bg() -> Option<Color> {
    Some(default_code_bg())
}

fn default_cell_padding() -> Length {
    Length::Pt(6.0)
}

fn default_table_font_size() -> Length {
    Length::Em(1.0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct QuoteStyle {
    #[serde(default = "default_border")]
    pub bar_color: Color,
    #[serde(default = "default_bar_width")]
    pub bar_width: Length,
    #[serde(default = "default_muted")]
    pub text_color: Color,
    pub italic: bool,
}

impl Default for QuoteStyle {
    fn default() -> Self {
        Self {
            bar_color: default_border(),
            bar_width: default_bar_width(),
            text_color: default_muted(),
            italic: false,
        }
    }
}

fn default_bar_width() -> Length {
    Length::Pt(3.0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ListStyle {
    #[serde(default = "default_indent")]
    pub indent: Length,
    #[serde(default = "default_bullet")]
    pub bullet: String,
}

impl Default for ListStyle {
    fn default() -> Self {
        Self {
            indent: default_indent(),
            bullet: default_bullet(),
        }
    }
}

fn default_indent() -> Length {
    Length::Em(1.2)
}

fn default_bullet() -> String {
    "•".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MermaidTheme {
    #[default]
    Default,
    Neutral,
    Dark,
    Forest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DiagramStyle {
    pub theme: MermaidTheme,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default = "default_scale")]
    pub max_width: f32,
    pub align: HAlign,
    #[serde(default, deserialize_with = "deserialize_optional_color")]
    pub background: Option<Color>,
    #[serde(default, deserialize_with = "deserialize_optional_color")]
    pub border: Option<Color>,
}

impl Default for DiagramStyle {
    fn default() -> Self {
        Self {
            theme: MermaidTheme::Default,
            scale: default_scale(),
            max_width: default_scale(),
            align: HAlign::Center,
            background: None,
            border: None,
        }
    }
}

fn default_scale() -> f32 {
    1.0
}
