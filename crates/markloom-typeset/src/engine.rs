//! Compile a Markdown document to pages.

use std::path::Path;
use std::sync::Arc;

use markloom_core::Doc;
use markloom_mermaid::DiagramService;
use markloom_style::StyleSheet;
use typst_layout::PagedDocument;

use crate::codegen::generate;
use crate::output::{self, ExportError, PageBitmap, PdfMeta};
use crate::source_map::{self, SourceMap, md_line_for_typst_line};
use crate::world::MarkloomWorld;

/// Whether the world also loads fonts installed on the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineOptions {
    pub system_fonts: bool,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self { system_fonts: true }
    }
}

/// Markdown line a diagnostic points at. Filled in by the source map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// One Typst diagnostic, not yet mapped back to a Markdown line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    pub message: String,
    pub md_line: Option<u32>,
    pub severity: Severity,
}

/// Typst rejected the generated source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileFailure {
    pub diagnostics: Vec<Diag>,
}

impl std::fmt::Display for CompileFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for diagnostic in &self.diagnostics {
            writeln!(f, "{}", diagnostic.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for CompileFailure {}

/// A successful compile.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub document: Arc<PagedDocument>,
    pub source_map: SourceMap,
    pub warnings: Vec<Diag>,
}

impl Compiled {
    pub fn page_count(&self) -> usize {
        self.document.pages().len()
    }

    pub fn page_size_pt(&self, index: usize) -> (f32, f32) {
        output::page_size_pt(&self.document, index)
    }

    pub fn page_hash(&self, index: usize) -> u128 {
        output::page_hash(&self.document, index)
    }

    pub fn render_page(&self, index: usize, pixels_per_pt: f32) -> PageBitmap {
        output::render_page(&self.document, index, pixels_per_pt)
    }

    pub fn pdf_bytes(&self, meta: &PdfMeta) -> Result<Vec<u8>, ExportError> {
        output::pdf_bytes(&self.document, meta)
    }

    pub fn write_pdf(&self, path: &Path, meta: &PdfMeta) -> Result<(), ExportError> {
        output::write_pdf(&self.document, path, meta)
    }
}

/// Keeps one Typst world and recompiles into it.
pub struct Engine {
    world: MarkloomWorld,
    diagrams: Arc<DiagramService>,
}

impl Engine {
    pub fn new(options: EngineOptions, diagrams: Arc<DiagramService>) -> Self {
        Self {
            world: MarkloomWorld::new(options.system_fonts),
            diagrams,
        }
    }

    pub fn font_families(&self) -> Vec<String> {
        self.world.font_families()
    }

    pub fn compile(
        &mut self,
        doc: &Doc,
        style: &StyleSheet,
        base_dir: &Path,
    ) -> Result<Compiled, CompileFailure> {
        let generated = generate(doc, style, base_dir, &self.diagrams);
        self.world
            .replace_document(&generated.source, &generated.assets);
        let warned = typst::compile::<PagedDocument>(&self.world);
        typst::comemo::evict(30);
        let diagnostics = |diagnostic: &typst::diag::SourceDiagnostic| {
            diag_from(diagnostic, &self.world, &generated)
        };
        match warned.output {
            Ok(document) => {
                let source_map = source_map::from_document(&document, &generated.blocks);
                Ok(Compiled {
                    document: Arc::new(document),
                    source_map,
                    warnings: warned.warnings.iter().map(diagnostics).collect(),
                })
            }
            Err(errors) => Err(CompileFailure {
                diagnostics: errors.iter().map(diagnostics).collect(),
            }),
        }
    }
}

fn diag_from(
    diagnostic: &typst::diag::SourceDiagnostic,
    world: &MarkloomWorld,
    generated: &crate::codegen::CodegenOutput,
) -> Diag {
    let severity = match diagnostic.severity {
        typst::diag::Severity::Error => Severity::Error,
        typst::diag::Severity::Warning => Severity::Warning,
    };
    let md_line = world
        .typst_line(diagnostic.span)
        .and_then(|line| md_line_for_typst_line(generated, line));
    Diag {
        message: diagnostic.message.to_string(),
        md_line,
        severity,
    }
}
