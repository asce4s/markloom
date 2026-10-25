//! comrak inline nodes to [`Inline`](crate::Inline).

use comrak::nodes::{AstNode, NodeValue};

use crate::ast::Inline;

pub(crate) fn inlines<'a>(node: &'a AstNode<'a>) -> Vec<Inline> {
    node.children().flat_map(convert_inline).collect()
}

fn convert_inline<'a>(node: &'a AstNode<'a>) -> Vec<Inline> {
    match node.data().value.clone() {
        NodeValue::Text(text) => vec![Inline::Text(text.into_owned())],
        NodeValue::Code(code) => vec![Inline::Code(code.literal)],
        NodeValue::Emph => vec![Inline::Emph(inlines(node))],
        NodeValue::Strong => vec![Inline::Strong(inlines(node))],
        NodeValue::Strikethrough => vec![Inline::Strike(inlines(node))],
        NodeValue::Link(link) => vec![Inline::Link {
            url: link.url,
            content: inlines(node),
        }],
        NodeValue::Image(image) => vec![Inline::Image {
            url: image.url,
            alt: alt_text(node),
        }],
        NodeValue::SoftBreak => vec![Inline::SoftBreak],
        NodeValue::LineBreak => vec![Inline::HardBreak],
        NodeValue::FootnoteReference(reference) => vec![Inline::FootnoteRef(reference.name)],
        NodeValue::HtmlInline(html) => {
            if is_br(&html) {
                vec![Inline::HardBreak]
            } else {
                Vec::new()
            }
        }
        _ => inlines(node),
    }
}

fn alt_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    collect_alt(node, &mut out);
    out
}

fn collect_alt<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        match child.data().value.clone() {
            NodeValue::Text(text) => out.push_str(&text),
            NodeValue::Code(code) => out.push_str(&code.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => collect_alt(child, out),
        }
    }
}

fn is_br(html: &str) -> bool {
    let Some(rest) = html.trim().strip_prefix('<') else {
        return false;
    };
    let rest = rest.trim_start();
    let name_end = rest
        .find(|ch: char| ch.is_whitespace() || ch == '/' || ch == '>')
        .unwrap_or(rest.len());
    rest[..name_end].eq_ignore_ascii_case("br")
}
