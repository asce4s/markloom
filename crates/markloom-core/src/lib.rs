//! Markdown parsing for Markloom.
//!
//! [`parse`] never fails. Anything the walker does not recognise becomes text,
//! and raw HTML blocks are dropped until a later task.

mod ast;
mod parse;

pub use ast::{Align, Block, Doc, FrontMatter, Inline, ListItem, SourceRange};
pub use parse::parse;
