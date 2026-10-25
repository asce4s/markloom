use markloom_core::{Align, Block, Doc, Inline, parse};

fn text_of(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
        }
    }
    out
}

fn parse_doc(markdown: &str) -> Doc {
    parse(markdown)
}

#[test]
fn heading_levels_and_ranges() {
    let doc = parse_doc("# A\n\n### B\n");
    assert_eq!(doc.blocks.len(), 2);
    match &doc.blocks[0] {
        Block::Heading {
            level,
            content,
            src,
        } => {
            assert_eq!(*level, 1);
            assert_eq!(text_of(content), "A");
            assert_eq!((src.start_line, src.end_line), (1, 1));
        }
        other => panic!("expected heading, got {other:?}"),
    }
    match &doc.blocks[1] {
        Block::Heading {
            level,
            content,
            src,
        } => {
            assert_eq!(*level, 3);
            assert_eq!(text_of(content), "B");
            assert_eq!((src.start_line, src.end_line), (3, 3));
        }
        other => panic!("expected heading, got {other:?}"),
    }
}

#[test]
fn mermaid_fence_becomes_diagram() {
    let doc = parse_doc("```mermaid\ngraph TD; A-->B\n```\n");
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Diagram { source, src } => {
            assert_eq!(source, "graph TD; A-->B\n");
            assert_eq!((src.start_line, src.end_line), (1, 3));
        }
        other => panic!("expected diagram, got {other:?}"),
    }

    let rust = parse_doc("```rust\nfn main() {}\n```\n");
    match &rust.blocks[0] {
        Block::Code { lang, code, src } => {
            assert_eq!(lang.as_deref(), Some("rust"));
            assert_eq!(code, "fn main() {}\n");
            assert_eq!((src.start_line, src.end_line), (1, 3));
        }
        other => panic!("expected code, got {other:?}"),
    }
}

#[test]
fn task_list_items() {
    let doc = parse_doc("- [ ] a\n- [x] b\n");
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::List {
            ordered,
            items,
            src,
            ..
        } => {
            assert!(!*ordered);
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].checked, Some(false));
            assert_eq!(items[1].checked, Some(true));
            assert_eq!(item_text(&items[0]), "a");
            assert_eq!(item_text(&items[1]), "b");
            assert_eq!(src.start_line, 1);
            assert_eq!(src.end_line, 2);
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn ordered_list_start() {
    let doc = parse_doc("3. x\n4. y\n");
    match &doc.blocks[0] {
        Block::List {
            ordered,
            start,
            items,
            ..
        } => {
            assert!(*ordered);
            assert_eq!(*start, 3);
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].checked, None);
            assert_eq!(item_text(&items[0]), "x");
            assert_eq!(item_text(&items[1]), "y");
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn table_alignment() {
    let doc = parse_doc("|a|b|c|\n|:-|:-:|-:|\n|1|2|3|\n");
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Table {
            align,
            header,
            rows,
            src,
        } => {
            assert_eq!(align, &[Align::Left, Align::Center, Align::Right]);
            assert_eq!(header.len(), 3);
            assert_eq!(text_of(&header[0]), "a");
            assert_eq!(text_of(&header[1]), "b");
            assert_eq!(text_of(&header[2]), "c");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].len(), 3);
            assert_eq!(text_of(&rows[0][0]), "1");
            assert_eq!(text_of(&rows[0][1]), "2");
            assert_eq!(text_of(&rows[0][2]), "3");
            assert_eq!(src.start_line, 1);
            assert!(src.end_line >= 3);
        }
        other => panic!("expected table, got {other:?}"),
    }
}

#[test]
fn nested_quote_contains_list() {
    let doc = parse_doc("> - nested\n");
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Quote { blocks, src } => {
            assert_eq!(src.start_line, 1);
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                Block::List { ordered, items, .. } => {
                    assert!(!*ordered);
                    assert_eq!(items.len(), 1);
                    assert_eq!(item_text(&items[0]), "nested");
                }
                other => panic!("expected list inside quote, got {other:?}"),
            }
        }
        other => panic!("expected quote, got {other:?}"),
    }
}

#[test]
fn thematic_break() {
    let doc = parse_doc("***\n");
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::Rule { src } => {
            assert_eq!((src.start_line, src.end_line), (1, 1));
        }
        other => panic!("expected rule, got {other:?}"),
    }
}

fn item_text(item: &markloom_core::ListItem) -> String {
    let mut out = String::new();
    for block in &item.blocks {
        if let Block::Paragraph { content, .. } = block {
            out.push_str(&text_of(content));
        }
    }
    out
}
