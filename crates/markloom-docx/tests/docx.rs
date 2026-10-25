use std::fs;
use std::io::Read;
use std::path::Path;

use markloom_core::parse;
use markloom_docx::export_docx;
use markloom_mermaid::DiagramService;
use markloom_style::PresetRegistry;

fn xml(path: &Path, name: &str) -> String {
    let file = fs::File::open(path).expect("docx");
    let mut archive = zip::ZipArchive::new(file).expect("zip");
    let mut entry = archive
        .by_name(name)
        .unwrap_or_else(|_| panic!("missing {name}"));
    let mut text = String::new();
    entry.read_to_string(&mut text).expect("read xml");
    text
}

fn export(markdown: &str, dir: &Path) -> std::path::PathBuf {
    let doc = parse(markdown);
    let style = PresetRegistry::builtin()
        .resolve("github", &[])
        .expect("github");
    let dest = dir.join("out.docx");
    export_docx(&doc, &style, dir, &DiagramService::with_defaults(), &dest).expect("export");
    dest
}

#[test]
fn styles_and_text_survive_hostile_literals() {
    let dir = tempfile_dir("styles");
    let dest = export(
        "# Title\n\nHello `# $ @ < ]` world.\n\n> quoted\n\n```\ncode\n```\n",
        &dir,
    );
    let document = xml(&dest, "word/document.xml");
    let styles = xml(&dest, "word/styles.xml");
    assert!(document.contains("Title"));
    assert!(document.contains("# $ @"));
    assert!(document.contains("&lt;"));
    assert!(document.contains("quoted"));
    assert!(document.contains("code"));
    assert!(styles.contains("Heading1"));
    assert!(styles.contains("Code"));
    assert!(styles.contains("Quote"));
}

#[test]
fn table_has_borders_and_header_shading() {
    let dir = tempfile_dir("table");
    let dest = export("| A | B |\n| --- | --- |\n| 1 | 2 |\n", &dir);
    let document = xml(&dest, "word/document.xml");
    assert!(document.contains("A"));
    assert!(document.contains("2"));
    assert!(document.contains("w:shd") || document.contains("w:tcBorders"));
}

#[test]
fn missing_image_is_placeholder_text() {
    let dir = tempfile_dir("image");
    let dest = export(
        "![chart](notes.txt)\n\n![gone](http://example.com/a.png)\n",
        &dir,
    );
    let document = xml(&dest, "word/document.xml");
    assert!(document.contains("chart"));
    assert!(document.contains("gone"));
    assert!(document.contains("placeholder"));
}

#[test]
fn a4_page_size_is_written() {
    let dir = tempfile_dir("page");
    let dest = export("Hi\n", &dir);
    let document = xml(&dest, "word/document.xml");
    assert!(document.contains("11906"));
    assert!(document.contains("16838"));
}

#[test]
fn diagram_is_embedded_as_png() {
    let dir = tempfile_dir("diagram");
    let dest = export("```mermaid\nflowchart LR\n  A-->B\n```\n", &dir);
    let file = fs::File::open(&dest).expect("docx");
    let mut archive = zip::ZipArchive::new(file).expect("zip");
    let mut found = false;
    for index in 0..archive.len() {
        let name = archive.by_index(index).expect("entry").name().to_string();
        if name.starts_with("word/media/") && name.ends_with(".png") {
            found = true;
        }
    }
    assert!(found, "diagram png missing");
}

#[test]
fn quote_styles_every_paragraph() {
    let dir = tempfile_dir("quote");
    let dest = export("> one\n>\n> two\n", &dir);
    let document = xml(&dest, "word/document.xml");
    assert!(document.contains("one"));
    assert!(document.contains("two"));
    assert!(
        document.matches("Quote").count() >= 2,
        "both paragraphs should use Quote: {document}"
    );
}

#[test]
fn task_checkbox_keeps_marks() {
    let dir = tempfile_dir("task");
    let dest = export("- [x] **done** and *more*\n", &dir);
    let document = xml(&dest, "word/document.xml");
    assert!(document.contains("[x]"));
    assert!(document.contains("done"));
    assert!(document.contains("more"));
    assert!(
        document.contains("<w:b/>") || document.contains("<w:b ") || document.contains("<w:b>"),
        "bold mark missing: {document}"
    );
}

fn tempfile_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("markloom-docx-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    path
}
