use std::path::Path;
use std::sync::Arc;

use markloom_core::{Block, parse};
use markloom_mermaid::DiagramService;
use markloom_style::PresetRegistry;
use markloom_typeset::{Engine, EngineOptions, generate, md_line_for_typst_line};

fn engine() -> Engine {
    Engine::new(
        EngineOptions {
            system_fonts: false,
        },
        Arc::new(DiagramService::with_defaults()),
    )
}

fn github() -> markloom_style::StyleSheet {
    PresetRegistry::builtin()
        .resolve("github", &[])
        .expect("github preset")
}

#[test]
fn headings_map_to_pages() {
    let mut markdown = String::from("# One\n\n");
    let sentence = "The quick brown fox jumps over the lazy dog while pages fill. ";
    for index in 0..120 {
        markdown.push_str(&format!("Paragraph {index}. "));
        for _ in 0..14 {
            markdown.push_str(sentence);
        }
        markdown.push_str("\n\n");
    }
    markdown.push_str("# Two\n");
    let two_line = line_of(&markdown, "# Two");
    let doc = parse(&markdown);
    let compiled = engine()
        .compile(&doc, &github(), Path::new("."))
        .expect("compile");
    let first = compiled.source_map.locate_line(1).expect("line 1");
    assert_eq!(first.page, 0, "first heading page");
    let second = compiled.source_map.locate_line(two_line).expect("# Two");
    assert!(
        second.page >= 1,
        "# Two on page {}, want >= 1 (pages {})",
        second.page,
        compiled.page_count()
    );

    let mut previous: Option<markloom_typeset::Location> = None;
    for block in &doc.blocks {
        let location = compiled
            .source_map
            .locate_line(block.src().start_line)
            .expect("block");
        if let Some(previous) = previous
            && location.page == previous.page
        {
            assert!(
                location.y_pt + 0.01 >= previous.y_pt,
                "y went backwards: {previous:?} then {location:?}"
            );
        }
        previous = Some(location);
    }
}

#[test]
fn line_at_roundtrip() {
    let markdown = "# One\n\nA paragraph.\n\n## Two\n\nMore.\n";
    let doc = parse(markdown);
    let compiled = engine()
        .compile(&doc, &github(), Path::new("."))
        .expect("compile");
    for block in &doc.blocks {
        let line = block.src().start_line;
        let location = compiled.source_map.locate_line(line).expect("locate");
        let found = compiled
            .source_map
            .line_at(location.page, location.y_pt)
            .expect("line_at");
        assert_eq!(found, line, "roundtrip for line {line}");
    }
}

#[test]
fn line_inside_code_block_maps_to_block() {
    let markdown = "# Title\n\n```\nalpha\nbeta\ngamma\n```\n";
    let doc = parse(markdown);
    let code = doc
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Code { src, .. } => Some(*src),
            _ => None,
        })
        .expect("code block");
    assert!(code.end_line > code.start_line);
    let compiled = engine()
        .compile(&doc, &github(), Path::new("."))
        .expect("compile");
    let start = compiled
        .source_map
        .locate_line(code.start_line)
        .expect("code start");
    let inside = compiled
        .source_map
        .locate_line(code.start_line + 1)
        .expect("inside code");
    assert_eq!(inside.page, start.page);
    assert!((inside.y_pt - start.y_pt).abs() < 0.01);
    assert_eq!(
        compiled.source_map.line_at(inside.page, inside.y_pt),
        Some(code.start_line)
    );
}

#[test]
fn diag_maps_to_markdown_line() {
    let markdown = "# Title\n\nParagraph one.\n\nParagraph two.\n";
    let doc = parse(markdown);
    let output = generate(
        &doc,
        &github(),
        Path::new("."),
        &DiagramService::with_defaults(),
    );
    assert!(output.line_to_block.len() >= 2);
    for (index, (typst_line, block_index)) in output.line_to_block.iter().enumerate() {
        let start = output.blocks[*block_index].start_line;
        assert_eq!(
            md_line_for_typst_line(&output, *typst_line),
            Some(start),
            "marker line {typst_line}"
        );
        if let Some((next, _)) = output.line_to_block.get(index + 1)
            && *next > *typst_line
        {
            assert_eq!(md_line_for_typst_line(&output, next - 1), Some(start));
        }
    }
    assert_eq!(md_line_for_typst_line(&output, 0), None);
}

fn line_of(markdown: &str, needle: &str) -> u32 {
    let byte = markdown.find(needle).expect(needle);
    u32::try_from(
        markdown[..byte]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count(),
    )
    .expect("line")
    .saturating_add(1)
}
