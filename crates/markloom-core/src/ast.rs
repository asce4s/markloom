//! Owned Markdown document tree.
//!
//! Line numbers on [`SourceRange`] are 1-based and inclusive.

use std::collections::BTreeMap;

/// YAML front matter. Task 2 fills the fields; this task keeps the struct empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrontMatter;

/// A 1-based inclusive span of source lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    pub start_line: u32,
    pub end_line: u32,
}

/// Inline content. This task stores only the plain text of a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
}

/// One item of a list, including an optional GFM task checkbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

/// Column alignment from a GFM table delimiter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

/// A block in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        content: Vec<Inline>,
        src: SourceRange,
    },
    Paragraph {
        content: Vec<Inline>,
        src: SourceRange,
    },
    Code {
        lang: Option<String>,
        code: String,
        src: SourceRange,
    },
    Diagram {
        source: String,
        src: SourceRange,
    },
    Quote {
        blocks: Vec<Block>,
        src: SourceRange,
    },
    List {
        ordered: bool,
        start: u32,
        items: Vec<ListItem>,
        src: SourceRange,
    },
    Table {
        align: Vec<Align>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        src: SourceRange,
    },
    Rule {
        src: SourceRange,
    },
}

impl Block {
    /// Source span of this block.
    pub fn src(&self) -> SourceRange {
        match self {
            Block::Heading { src, .. }
            | Block::Paragraph { src, .. }
            | Block::Code { src, .. }
            | Block::Diagram { src, .. }
            | Block::Quote { src, .. }
            | Block::List { src, .. }
            | Block::Table { src, .. }
            | Block::Rule { src } => *src,
        }
    }
}

/// A parsed document. Parsing never fails: unrecognised input becomes text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Doc {
    pub front_matter: FrontMatter,
    pub blocks: Vec<Block>,
    pub footnotes: BTreeMap<String, Vec<Block>>,
}
