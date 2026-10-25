//! Markdown parsing for Markloom.
//!
//! [`parse`] never fails. Anything the walker does not recognise becomes text,
//! and raw HTML blocks are dropped until a later task.

mod ast;
mod front_matter;
mod inline;
mod parse;

pub use ast::{Align, Block, Doc, FrontMatter, FrontMatterError, Inline, ListItem, SourceRange};
pub use front_matter::parse_front_matter;
pub use parse::parse;
