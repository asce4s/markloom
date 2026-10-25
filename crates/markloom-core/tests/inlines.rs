use markloom_core::{Block, Doc, Inline, parse, parse_front_matter};

fn paragraph(doc: &Doc) -> &Vec<Inline> {
    match &doc.blocks[0] {
        Block::Paragraph { content, .. } => content,
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn nested_emphasis_and_code() {
    let doc = parse("**a _b_ `c`**");
    let content = paragraph(&doc);
    assert_eq!(
        content,
        &vec![Inline::Strong(vec![
            Inline::Text("a ".to_string()),
            Inline::Emph(vec![Inline::Text("b".to_string())]),
            Inline::Text(" ".to_string()),
            Inline::Code("c".to_string()),
        ])]
    );
}

#[test]
fn link_and_image() {
    let doc = parse("[see](https://example.com/a) ![Alt](pic.png)");
    assert_eq!(
        paragraph(&doc),
        &vec![
            Inline::Link {
                url: "https://example.com/a".to_string(),
                content: vec![Inline::Text("see".to_string())],
            },
            Inline::Text(" ".to_string()),
            Inline::Image {
                url: "pic.png".to_string(),
                alt: "Alt".to_string(),
            },
        ]
    );
}

#[test]
fn hard_and_soft_breaks() {
    let soft = parse("foo\nbar\n");
    assert_eq!(
        paragraph(&soft),
        &vec![
            Inline::Text("foo".to_string()),
            Inline::SoftBreak,
            Inline::Text("bar".to_string()),
        ]
    );
    let hard = parse("foo\\\nbar\n");
    assert_eq!(
        paragraph(&hard),
        &vec![
            Inline::Text("foo".to_string()),
            Inline::HardBreak,
            Inline::Text("bar".to_string()),
        ]
    );
}

#[test]
fn footnote_ref_and_definition() {
    let doc = parse("x[^n]\n\n[^n]: note\n");
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(
        paragraph(&doc),
        &vec![
            Inline::Text("x".to_string()),
            Inline::FootnoteRef("n".to_string()),
        ]
    );
    let note = doc.footnotes.get("n").expect("footnote n");
    assert_eq!(note.len(), 1);
    match &note[0] {
        Block::Paragraph { content, .. } => {
            assert_eq!(content, &vec![Inline::Text("note".to_string())]);
        }
        other => panic!("expected note paragraph, got {other:?}"),
    }
}

#[test]
fn inline_br_html_is_hard_break() {
    let doc = parse("a<br>b");
    assert_eq!(
        paragraph(&doc),
        &vec![
            Inline::Text("a".to_string()),
            Inline::HardBreak,
            Inline::Text("b".to_string()),
        ]
    );
    let kept = parse("a<em>c</em>d");
    assert_eq!(
        paragraph(&kept),
        &vec![
            Inline::Text("a".to_string()),
            Inline::Text("c".to_string()),
            Inline::Text("d".to_string()),
        ]
    );
}

#[test]
fn html_block_dropped() {
    let doc = parse("<details>\n<summary>x</summary>\n</details>\n\npara");
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(paragraph(&doc), &vec![Inline::Text("para".to_string())]);
}

#[test]
fn front_matter_fields() {
    let markdown = "\
---
title: T
author: A
markloom:
  preset: compact
  style:
    typography:
      base_size: 10pt
---
body";
    let doc = parse(markdown);
    assert_eq!(doc.front_matter.title.as_deref(), Some("T"));
    assert_eq!(doc.front_matter.author.as_deref(), Some("A"));
    assert_eq!(doc.front_matter.preset.as_deref(), Some("compact"));
    let style = doc.front_matter.style.as_ref().expect("style table");
    assert_eq!(style["typography"]["base_size"].as_str(), Some("10pt"));
    match &doc.blocks[0] {
        Block::Paragraph { src, .. } => assert_eq!(src.start_line, 10),
        other => panic!("expected body paragraph, got {other:?}"),
    }

    let parsed = parse_front_matter(
        "title: T\nauthor: A\nmarkloom:\n  preset: compact\n  style:\n    typography:\n      base_size: 10pt\n",
    )
    .expect("yaml");
    assert_eq!(parsed.preset.as_deref(), Some("compact"));
}

#[test]
fn invalid_front_matter_is_ignored() {
    let doc = parse("---\n[\n---\nbody\n");
    assert_eq!(doc.front_matter, markloom_core::FrontMatter::default());
    assert_eq!(paragraph(&doc), &vec![Inline::Text("body".to_string())]);
    assert!(parse_front_matter("[").is_err());
}

#[test]
fn crlf_source_ranges_match_lf() {
    let lf = "# Title\n\npara with *em*\n";
    let crlf = lf.replace('\n', "\r\n");
    let left = parse(lf);
    let right = parse(&crlf);
    assert_eq!(left.blocks.len(), right.blocks.len());
    for (a, b) in left.blocks.iter().zip(right.blocks.iter()) {
        assert_eq!(a.src(), b.src());
    }
    assert_eq!(debug_text(&left), debug_text(&right));
    assert!(!debug_text(&right).contains('\r'));
}

fn debug_text(doc: &Doc) -> String {
    fn walk_inline(inline: &Inline, out: &mut String) {
        match inline {
            Inline::Text(text) | Inline::Code(text) => out.push_str(text),
            Inline::Emph(inner) | Inline::Strong(inner) | Inline::Strike(inner) => {
                for child in inner {
                    walk_inline(child, out);
                }
            }
            Inline::Link { content, .. } => {
                for child in content {
                    walk_inline(child, out);
                }
            }
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::FootnoteRef(name) => out.push_str(name),
            Inline::SoftBreak => out.push(' '),
            Inline::HardBreak => out.push('\n'),
        }
    }
    fn walk_block(block: &Block, out: &mut String) {
        match block {
            Block::Heading { content, .. } | Block::Paragraph { content, .. } => {
                for inline in content {
                    walk_inline(inline, out);
                }
            }
            Block::Code { code, .. } | Block::Diagram { source: code, .. } => out.push_str(code),
            Block::Quote { blocks, .. } => {
                for child in blocks {
                    walk_block(child, out);
                }
            }
            Block::List { items, .. } => {
                for item in items {
                    for child in &item.blocks {
                        walk_block(child, out);
                    }
                }
            }
            Block::Table { header, rows, .. } => {
                for cell in header.iter().chain(rows.iter().flatten()) {
                    for inline in cell {
                        walk_inline(inline, out);
                    }
                }
            }
            Block::Rule { .. } => {}
        }
    }
    let mut out = String::new();
    for block in &doc.blocks {
        walk_block(block, &mut out);
    }
    out
}
