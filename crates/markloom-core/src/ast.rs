//! Owned Markdown document tree.
//!
//! Line numbers on [`SourceRange`] are 1-based and inclusive.

use std::collections::BTreeMap;

/// YAML front matter. Unknown or invalid YAML becomes [`FrontMatter::default`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub author: Option<String>,
    pub preset: Option<String>,
    pub style: Option<toml::Table>,
}

/// Why [`crate::parse_front_matter`] rejected a YAML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatterError {
    message: String,
}

impl FrontMatterError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for FrontMatterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FrontMatterError {}

/// A 1-based inclusive span of source lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    pub start_line: u32,
    pub end_line: u32,
}

/// Inline content. Markdown text is stored here and later emitted as Typst
/// string literals, never as spliced markup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Link { url: String, content: Vec<Inline> },
    Image { url: String, alt: String },
    SoftBreak,
    HardBreak,
    FootnoteRef(String),
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
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Doc {
    pub front_matter: FrontMatter,
    pub blocks: Vec<Block>,
    pub footnotes: BTreeMap<String, Vec<Block>>,
}
