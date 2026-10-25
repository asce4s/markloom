//! Markdown to Typst, then to pages and PDF.

pub mod codegen;
mod engine;
mod output;
mod world;

pub use codegen::{Assets, CodegenOutput, DiagramRef, generate, str_lit};
pub use engine::{CompileFailure, Compiled, Diag, Engine, EngineOptions, Severity, SourceMap};
pub use output::{ExportError, PageBitmap, PdfMeta};
