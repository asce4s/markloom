//! Markdown to Typst, then to pages and PDF.

pub mod codegen;

pub use codegen::{Assets, CodegenOutput, DiagramRef, generate, str_lit};
