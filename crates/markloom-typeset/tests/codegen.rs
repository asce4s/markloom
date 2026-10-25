use std::path::Path;

use markloom_core::parse;
use markloom_mermaid::DiagramService;
use markloom_style::PresetRegistry;
use markloom_typeset::{generate, str_lit};

fn source_for(markdown: &str, preset: &str) -> markloom_typeset::CodegenOutput {
    let style = PresetRegistry::builtin()
        .resolve(preset, &[])
        .expect(preset);
    generate(
        &parse(markdown),
        &style,
        Path::new("."),
        &DiagramService::with_defaults(),
    )
}

fn preamble(preset: &str) -> String {
    source_for("", preset).source
}

#[test]
fn snapshot_preamble_github() {
    insta::assert_snapshot!(preamble("github"));
}

#[test]
fn snapshot_preamble_compact() {
    insta::assert_snapshot!(preamble("compact"));
}

#[test]
fn snapshot_preamble_academic() {
    insta::assert_snapshot!(preamble("academic"));
}

#[test]
fn snapshot_preamble_minimal() {
    insta::assert_snapshot!(preamble("minimal"));
}

#[test]
fn snapshot_preamble_print_friendly() {
    insta::assert_snapshot!(preamble("print-friendly"));
}

#[test]
fn snapshot_blocks_showcase() {
    let markdown = "\
# Title

A paragraph with **bold**, [a link](https://example.com), and `code`.

> A quote
>
> - nested

- [ ] todo
- [x] done

3. third

| a | b | c |
| --- | :-: | ---: |
| 1 | 2 | 3 |

---

```rust
fn main() {}
```

```mermaid
graph TD; A-->B
```

See[^n]

[^n]: note

![alt](nope.png)
";
    insta::assert_snapshot!(source_for(markdown, "github").source);
}

#[test]
fn hostile_text_is_literal() {
    let text = "#set page(width: 1pt) $x$ @ref <lbl> ] [ \\ \" `";
    let markdown = format!("{text}\n");
    let doc = parse(&markdown);
    let output = source_for(&markdown, "github");
    let mut outside = output.source.clone();
    let mut saw_directive = false;
    fn strip(inlines: &[markloom_core::Inline], outside: &mut String, saw: &mut bool) {
        for inline in inlines {
            match inline {
                markloom_core::Inline::Text(text) | markloom_core::Inline::Code(text) => {
                    let literal = str_lit(text);
                    assert!(outside.contains(&literal), "missing {literal} in {outside}");
                    if text.contains("#set page(width: 1pt)") {
                        *saw = true;
                    }
                    *outside = outside.replacen(&literal, "", 1);
                }
                markloom_core::Inline::Emph(inner)
                | markloom_core::Inline::Strong(inner)
                | markloom_core::Inline::Strike(inner)
                | markloom_core::Inline::Link { content: inner, .. } => {
                    strip(inner, outside, saw);
                }
                markloom_core::Inline::Image { alt, url } => {
                    *outside = outside.replacen(&str_lit(alt), "", 1);
                    *outside = outside.replacen(&str_lit(url), "", 1);
                }
                markloom_core::Inline::FootnoteRef(label) => {
                    *outside = outside.replacen(&str_lit(&format!("[^{label}]")), "", 1);
                }
                markloom_core::Inline::SoftBreak | markloom_core::Inline::HardBreak => {}
            }
        }
    }
    for block in &doc.blocks {
        if let markloom_core::Block::Paragraph { content, .. } = block {
            strip(content, &mut outside, &mut saw_directive);
        }
    }
    assert!(saw_directive, "parser dropped the hostile directive");
    assert_eq!(outside.matches("#set page(").count(), 1, "{outside}");
    assert!(!outside.contains("#set page(width: 1pt)"));
}

#[test]
fn block_markers_match_blocks() {
    let markdown = "# One\n\npara\n\n---\n";
    let doc = parse(markdown);
    let output = source_for(markdown, "github");
    let markers = output.source.matches("<ml-b>").count();
    assert_eq!(markers, output.blocks.len());
    assert_eq!(output.blocks.len(), doc.blocks.len());
    assert_eq!(output.line_to_block.len(), doc.blocks.len());
}

#[test]
fn missing_image_is_placeholder() {
    let output = source_for("![alt](nope.png)\n", "github");
    assert!(output.source.contains("Image: alt"), "{}", output.source);
    assert!(output.assets.images.is_empty());
}
