//! Markdown to [`Doc`](crate::Doc), via comrak.

use comrak::nodes::{AstNode, ListType, NodeValue, Sourcepos, TableAlignment};
use comrak::{Arena, Options, parse_document};

use crate::ast::{Align, Block, Doc, ListItem, SourceRange};
use crate::front_matter::{parse_front_matter, yaml_body};
use crate::inline;

/// Parse `markdown` into an owned document.
///
/// Unrecognised nodes become paragraphs of their plain text. Raw HTML blocks
/// are dropped; Task 2 decides what to keep of inline HTML.
pub fn parse(markdown: &str) -> Doc {
    let arena = Arena::new();
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.tasklist = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    options.extension.front_matter_delimiter = Some("---".to_string());

    let normalized = markdown.replace("\r\n", "\n");
    let root = parse_document(&arena, &normalized, &options);
    let mut doc = Doc::default();
    for child in root.children() {
        match child.data().value.clone() {
            NodeValue::FootnoteDefinition(def) => {
                doc.footnotes.insert(def.name, child_blocks(child));
            }
            NodeValue::FrontMatter(raw) => {
                doc.front_matter = parse_front_matter(&yaml_body(&raw)).unwrap_or_default();
            }
            _ => {
                if let Some(block) = convert_block(child) {
                    doc.blocks.push(block);
                }
            }
        }
    }
    doc
}

fn child_blocks<'a>(node: &'a AstNode<'a>) -> Vec<Block> {
    node.children().filter_map(convert_block).collect()
}

fn convert_block<'a>(node: &'a AstNode<'a>) -> Option<Block> {
    let (value, src) = {
        let ast = node.data();
        (ast.value.clone(), range_of(&ast.sourcepos))
    };
    match value {
        NodeValue::Heading(heading) => Some(Block::Heading {
            level: heading.level,
            content: inline::inlines(node),
            src,
        }),
        NodeValue::Paragraph => Some(Block::Paragraph {
            content: inline::inlines(node),
            src,
        }),
        NodeValue::CodeBlock(code) => {
            let lang = lang_of(&code.info);
            if lang
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case("mermaid"))
            {
                Some(Block::Diagram {
                    source: code.literal,
                    src,
                })
            } else {
                Some(Block::Code {
                    lang,
                    code: code.literal,
                    src,
                })
            }
        }
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => Some(Block::Quote {
            blocks: child_blocks(node),
            src,
        }),
        NodeValue::List(list) => Some(Block::List {
            ordered: list.list_type == ListType::Ordered,
            start: u32::try_from(list.start).unwrap_or(u32::MAX),
            items: node.children().filter_map(convert_item).collect(),
            src,
        }),
        NodeValue::Table(table) => {
            let align = table.alignments.iter().copied().map(map_align).collect();
            let mut header = Vec::new();
            let mut rows = Vec::new();
            for row in node.children() {
                let is_header = matches!(row.data().value, NodeValue::TableRow(true));
                let cells = row.children().map(inline::inlines).collect();
                if is_header {
                    header = cells;
                } else {
                    rows.push(cells);
                }
            }
            Some(Block::Table {
                align,
                header,
                rows,
                src,
            })
        }
        NodeValue::ThematicBreak => Some(Block::Rule { src }),
        NodeValue::HtmlBlock(_)
        | NodeValue::FrontMatter(_)
        | NodeValue::FootnoteDefinition(_)
        | NodeValue::Document => None,
        _ => fallback_block(node, src),
    }
}

fn convert_item<'a>(node: &'a AstNode<'a>) -> Option<ListItem> {
    let checked = match node.data().value.clone() {
        NodeValue::Item(_) => None,
        NodeValue::TaskItem(task) => Some(task.symbol.is_some()),
        _ => return None,
    };
    Some(ListItem {
        checked,
        blocks: child_blocks(node),
    })
}

fn fallback_block<'a>(node: &'a AstNode<'a>, src: SourceRange) -> Option<Block> {
    let content = inline::inlines(node);
    if content.is_empty() {
        return None;
    }
    Some(Block::Paragraph { content, src })
}

fn lang_of(info: &str) -> Option<String> {
    let token = info.split_whitespace().next()?;
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn map_align(align: TableAlignment) -> Align {
    match align {
        TableAlignment::None => Align::None,
        TableAlignment::Left => Align::Left,
        TableAlignment::Center => Align::Center,
        TableAlignment::Right => Align::Right,
    }
}

fn range_of(pos: &Sourcepos) -> SourceRange {
    SourceRange {
        start_line: line_as_u32(pos.start.line),
        end_line: line_as_u32(pos.end.line),
    }
}

fn line_as_u32(line: usize) -> u32 {
    u32::try_from(line).unwrap_or(u32::MAX)
}
