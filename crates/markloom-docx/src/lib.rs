//! DOCX export from the same document tree the Typst codegen walks.
//!
//! Word lays the text out again. Fonts, sizes, colours, tables, and diagrams
//! follow the stylesheet. Page breaks do not match the preview.

mod export;

pub use export::{DocxError, export_docx};
