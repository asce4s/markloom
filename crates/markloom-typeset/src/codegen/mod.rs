//! Markdown and a style sheet to Typst source.

mod blocks;
mod escape;
mod preamble;

use std::path::Path;
use std::sync::Arc;

use markloom_core::{Doc, SourceRange};
use markloom_mermaid::DiagramService;
use markloom_style::StyleSheet;

pub use escape::str_lit;

/// Files the Typst world must serve for this document.
#[derive(Debug, Clone, Default)]
pub struct Assets {
    pub diagrams: Vec<(String, DiagramRef)>,
    pub images: Vec<(String, std::path::PathBuf)>,
}

/// A rendered diagram, or the error text shown in its place.
#[derive(Debug, Clone)]
pub enum DiagramRef {
    Svg(Arc<str>),
    Error { message: String },
}

/// Generated Typst source plus the map back to Markdown blocks.
#[derive(Debug, Clone)]
pub struct CodegenOutput {
    pub source: String,
    pub blocks: Vec<SourceRange>,
    pub line_to_block: Vec<(u32, usize)>,
    pub assets: Assets,
    /// 1-based Typst lines of the custom block, exclusive of the marker lines.
    pub custom_lines: Option<std::ops::Range<u32>>,
}

/// Generate Typst. Markdown text is emitted only as string literals.
pub fn generate(
    doc: &Doc,
    style: &StyleSheet,
    base_dir: &Path,
    diagrams: &DiagramService,
) -> CodegenOutput {
    let preamble = preamble::render(style);
    blocks::emit(doc, style, base_dir, diagrams, preamble)
}
