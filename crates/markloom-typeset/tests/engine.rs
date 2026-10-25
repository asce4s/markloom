use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use markloom_core::parse;
use markloom_mermaid::DiagramService;
use markloom_style::PresetRegistry;
use markloom_typeset::{Compiled, Engine, EngineOptions, PdfMeta, Severity};

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

fn compile(markdown: &str) -> Compiled {
    compile_in(markdown, Path::new("."))
}

fn compile_in(markdown: &str, base_dir: &Path) -> Compiled {
    engine()
        .compile(&parse(markdown), &github(), base_dir)
        .unwrap_or_else(|failure| panic!("compile failed: {failure:?}"))
}

#[test]
fn compiles_simple_doc() {
    let compiled = compile("# Hello\n\nA paragraph.\n");
    assert_eq!(compiled.page_count(), 1);
    assert!(
        compiled
            .warnings
            .iter()
            .all(|diag| diag.severity != Severity::Error)
    );
}

#[test]
fn pdf_starts_with_magic() {
    let compiled = compile("# Hello\n\nA paragraph.\n");
    let bytes = compiled
        .pdf_bytes(&PdfMeta {
            title: Some("Hello".into()),
            author: Some("Markloom".into()),
        })
        .expect("pdf");
    assert!(bytes.starts_with(b"%PDF-"), "pdf magic missing");
}

#[test]
fn landscape_page_is_wide() {
    let mut style = github();
    style.page.orientation = markloom_style::Orientation::Landscape;
    let compiled = engine()
        .compile(&parse("# Hello\n"), &style, Path::new("."))
        .expect("landscape");
    let (width, height) = compiled.page_size_pt(0);
    assert!(width > height, "{width} x {height}");
}

#[test]
fn custom_typst_error_is_prefixed() {
    let mut style = github();
    style.custom.typst = "#let x = ".into();
    let failure = engine()
        .compile(&parse("# Hello\n"), &style, Path::new("."))
        .expect_err("broken custom typst");
    let message = &failure.diagnostics[0].message;
    assert!(message.starts_with("Custom Typst: "), "{message}");
    assert_eq!(failure.diagnostics[0].md_line, None);
}

#[test]
fn custom_typst_infinite_loop_is_an_error() {
    let mut style = github();
    style.custom.typst = "#while true {}".into();
    let started = Instant::now();
    let failure = engine()
        .compile(&parse("Hi\n"), &style, Path::new("."))
        .expect_err("loop must fail");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "elapsed {:?}",
        started.elapsed()
    );
    let message = &failure.diagnostics[0].message;
    assert!(message.starts_with("Custom Typst: "), "{message}");
}

#[test]
fn diagram_count_includes_diagram_blocks() {
    let compiled = compile("```mermaid\ngraph TD; A-->B\n```\n");
    assert_eq!(compiled.diagram_count, 1);
    let plain = compile("# Hello\n");
    assert_eq!(plain.diagram_count, 0);
}

#[test]
fn premultiplied_render_keeps_byte_length() {
    let compiled = compile("# Hello\n");
    let page = compiled.render_page_premultiplied(0, 1.0);
    assert_eq!(
        page.rgba.len(),
        page.width as usize * page.height as usize * 4
    );
    assert!(page.width > page.height / 2);
}

#[test]
fn render_page_dimensions() {
    let compiled = compile("# Hello\n\nA paragraph.\n");
    let bitmap = compiled.render_page(0, 2.0);
    assert!(
        (i64::from(bitmap.width) - 1191).abs() <= 1,
        "width {}",
        bitmap.width
    );
    assert!(
        (i64::from(bitmap.height) - 1684).abs() <= 1,
        "height {}",
        bitmap.height
    );
    assert_eq!(
        bitmap.rgba.len(),
        bitmap.width as usize * bitmap.height as usize * 4
    );
    let (width_pt, height_pt) = compiled.page_size_pt(0);
    assert!((width_pt - 595.28).abs() < 1.0, "{width_pt}");
    assert!((height_pt - 841.89).abs() < 1.0, "{height_pt}");
    assert_ne!(compiled.page_hash(0), 0);
}

#[test]
fn preset_changes_layout() {
    let markdown = long_prose(80);
    let doc = parse(&markdown);
    let github_pages = engine()
        .compile(&doc, &github(), Path::new("."))
        .expect("github")
        .page_count();
    let compact = PresetRegistry::builtin()
        .resolve("compact", &[])
        .expect("compact");
    let compact_pages = engine()
        .compile(&doc, &compact, Path::new("."))
        .expect("compact")
        .page_count();
    assert!(
        github_pages >= 3,
        "github fixture should fill at least 3 pages, got {github_pages}"
    );
    assert!(
        compact_pages < github_pages,
        "compact {compact_pages} should be fewer pages than github {github_pages}"
    );
}

#[test]
fn hostile_markdown_compiles() {
    let markdown = "\
# #set page(width: 1pt) $x$ @ref <lbl> ] [ \\\\ \\\" `

A cell with $ # @x <y> ] [ ` and an unbalanced [

| $ | # @x | <y> ` |
| --- | --- | --- |
| ] | [ | ` |

[link](https://example.com/$/#/@x/<y>/]/[/`)

See[^note]

[^note]: footnote with $ # @x <y> ] [ `

```
`unbalanced
```
";
    let compiled = compile(markdown);
    assert!(compiled.page_count() >= 1);
    assert!(
        compiled
            .warnings
            .iter()
            .all(|diag| diag.severity != Severity::Error)
    );
}

#[test]
fn invalid_mermaid_renders_error_box() {
    let markdown = "\
```mermaid
graph TD; A-->
```

Still a paragraph.
";
    let compiled = compile(markdown);
    assert_eq!(compiled.page_count(), 1);
    assert!(
        compiled
            .warnings
            .iter()
            .all(|diag| diag.severity != Severity::Error)
    );
}

#[test]
fn hostile_image_alt_compiles() {
    let markdown = "![#set page(width: 1pt) $x$ @ref](missing.png)\n";
    let compiled = compile(markdown);
    assert!(compiled.page_count() >= 1);
    assert!(
        compiled
            .warnings
            .iter()
            .all(|diag| diag.severity != Severity::Error)
    );
}

#[test]
fn bad_images_become_placeholders() {
    let dir = std::env::temp_dir().join(format!("markloom-engine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(dir.join("notes.txt"), "notes").expect("notes");
    let compiled = compile_in(
        "![a](missing.png) ![b](https://x/y.png) ![c](notes.txt)\n",
        &dir,
    );
    assert!(compiled.page_count() >= 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn write_pdf_replaces_via_tmp() {
    let compiled = compile("# Hello\n");
    let dir = std::env::temp_dir().join(format!("markloom-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("out.pdf");
    compiled
        .write_pdf(
            &path,
            &PdfMeta {
                title: None,
                author: None,
            },
        )
        .expect("write pdf");
    let bytes = std::fs::read(&path).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(!dir.join("out.pdf.tmp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn write_pdf_removes_tmp_when_rename_fails() {
    let compiled = compile("# Hello\n");
    let dir = std::env::temp_dir().join(format!("markloom-pdf-rename-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("out.pdf");
    std::fs::create_dir(&path).expect("dest dir");
    let error = compiled
        .write_pdf(
            &path,
            &PdfMeta {
                title: None,
                author: None,
            },
        )
        .expect_err("rename onto a directory");
    assert!(!error.to_string().is_empty());
    assert!(!dir.join("out.pdf.tmp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_document_compiles_to_one_page() {
    assert_eq!(compile("").page_count(), 1);
    assert_eq!(compile("---\ntitle: x\n---\n").page_count(), 1);
}

#[test]
#[cfg_attr(debug_assertions, ignore)]
fn incremental_recompile_is_fast() {
    let mut markdown = twenty_page_markdown();
    let mut engine = engine();
    let style = github();
    let base = Path::new(".");
    let first = engine
        .compile(&parse(&markdown), &style, base)
        .expect("warmup");
    assert!(
        first.page_count() >= 20,
        "fixture has {} pages",
        first.page_count()
    );
    markdown = markdown.replacen("EDITME", "EDITED", 1);
    let started = Instant::now();
    let second = engine
        .compile(&parse(&markdown), &style, base)
        .expect("recompile");
    let elapsed = started.elapsed();
    assert!(second.page_count() >= 20);
    assert!(
        elapsed.as_secs_f64() * 1000.0 < 150.0,
        "second compile took {elapsed:?}"
    );
}

fn long_prose(paragraphs: usize) -> String {
    let sentence = "The quick brown fox jumps over the lazy dog while the preview recompiles. ";
    let mut markdown = String::from("# Layout\n\n");
    for index in 0..paragraphs {
        markdown.push_str(&format!("Paragraph {index}. "));
        for _ in 0..12 {
            markdown.push_str(sentence);
        }
        markdown.push_str("\n\n");
    }
    markdown
}

fn twenty_page_markdown() -> String {
    let mut markdown = String::from(
        "# Latency\n\nEDITME sits in the first paragraph so a one-word edit is local.\n\n",
    );
    markdown.push_str("| Column | Kind | Notes |\n| --- | --- | --- |\n| alpha | text | row |\n| beta | code | row |\n| gamma | table | row |\n\n");
    markdown.push_str("```rust\nfn main() {\n    println!(\"markloom\");\n}\n```\n\n");
    let sentence = "The quick brown fox jumps over the lazy dog while the preview recompiles a long document. ";
    for section in 1..=32 {
        markdown.push_str(&format!("## Section {section}\n\n"));
        for paragraph in 0..6 {
            markdown.push_str(&format!("Paragraph {section}.{paragraph}. "));
            for _ in 0..8 {
                markdown.push_str(sentence);
            }
            markdown.push_str("\n\n");
        }
    }
    markdown
}
