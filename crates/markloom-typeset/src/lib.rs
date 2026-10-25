//! Markdown to Typst, then to pages and PDF.

pub mod codegen;
mod engine;
mod output;
mod source_map;
mod world;

pub use codegen::{Assets, CodegenOutput, DiagramRef, generate, str_lit};
pub use engine::{CompileFailure, Compiled, Diag, Engine, EngineOptions, Severity};
pub use output::{ExportError, PageBitmap, PdfMeta};
pub use source_map::{Location, SourceMap, md_line_for_typst_line};
