//! Preset style sheets and the values they parse.

mod merge;
mod registry;
mod sheet;
mod values;

pub use merge::deep_merge;
pub use registry::{PresetLoadError, PresetRegistry};
pub use sheet::{
    CodeStyle, Colors, CustomStyle, DiagramStyle, HAlign, ImageStyle, ListStyle, Margins,
    MermaidTheme, Orientation, PageNumbers, PageStyle, PaperSize, QuoteStyle, StyleError,
    StyleSheet, TableBorders, TableStyle, Typography, Weight,
};
pub use values::{Color, Length};
