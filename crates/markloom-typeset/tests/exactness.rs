//! Preview pixels against PDF rasterization and checked-in first pages.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use markloom_core::parse;
use markloom_mermaid::DiagramService;
use markloom_style::PresetRegistry;
use markloom_typeset::{Compiled, Engine, EngineOptions, PageBitmap, PdfMeta};

fn showcase_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/showcase")
        .canonicalize()
        .expect("showcase dir")
}

fn compile(preset: &str) -> Compiled {
    let dir = showcase_dir();
    let markdown = std::fs::read_to_string(dir.join("index.md")).expect("index.md");
    let style = PresetRegistry::builtin()
        .resolve(preset, &[])
        .expect(preset);
    Engine::new(
        EngineOptions {
            system_fonts: false,
        },
        Arc::new(DiagramService::with_defaults()),
    )
    .compile(&parse(&markdown), &style, &dir)
    .unwrap_or_else(|failure| panic!("{preset}: {failure}"))
}

fn encode_png(bitmap: &PageBitmap) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, bitmap.width, bitmap.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&bitmap.rgba).expect("png data");
    drop(writer);
    bytes
}

fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("png info");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).expect("png frame");
    buffer.truncate(info.buffer_size());
    (info.width, info.height, buffer)
}

fn differing_fraction(left: &[u8], right: &[u8]) -> f64 {
    let mut pixels = 0usize;
    let mut differ = 0usize;
    for (a, b) in left.as_chunks::<4>().0.iter().zip(right.as_chunks::<4>().0) {
        pixels += 1;
        if a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > 32) {
            differ += 1;
        }
    }
    if pixels == 0 {
        1.0
    } else {
        differ as f64 / pixels as f64
    }
}

fn assert_close(label: &str, preview: &PageBitmap, other: &[u8]) {
    let (width, height, rgba) = decode_png(other);
    assert_eq!(
        (preview.width, preview.height),
        (width, height),
        "{label} size"
    );
    let fraction = differing_fraction(&preview.rgba, &rgba);
    assert!(
        fraction <= 0.005,
        "{label} differs in {fraction:.4} of pixels (limit 0.005)"
    );
}

fn preview_equals_pdf(preset: &str) {
    let compiled = compile(preset);
    if preset == "github" {
        assert!(
            compiled.page_count() >= 3,
            "showcase has {} pages",
            compiled.page_count()
        );
    }
    if Command::new("pdftoppm").arg("-v").output().is_err() {
        eprintln!("skipping preview_equals_pdf_{preset}: pdftoppm is not installed");
        return;
    }
    let dir = std::env::temp_dir().join(format!("markloom-exact-{preset}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let pdf_path = dir.join("doc.pdf");
    compiled
        .write_pdf(&pdf_path, &PdfMeta::default())
        .expect("pdf");
    let status = Command::new("pdftoppm")
        .args([
            "-r",
            "72",
            "-png",
            pdf_path.to_str().expect("utf8 pdf path"),
            dir.join("pdf").to_str().expect("utf8 prefix"),
        ])
        .status()
        .expect("pdftoppm");
    assert!(status.success(), "pdftoppm {status}");
    let mut pdf_pages = 0usize;
    for index in 0..compiled.page_count() {
        let png_path = dir.join(format!("pdf-{}.png", index + 1));
        let bytes =
            std::fs::read(&png_path).unwrap_or_else(|_| panic!("missing {}", png_path.display()));
        pdf_pages += 1;
        let preview = compiled.render_page(index, 1.0);
        assert_close(&format!("{preset} page {index}"), &preview, &bytes);
    }
    assert_eq!(pdf_pages, compiled.page_count());
    let _ = std::fs::remove_dir_all(&dir);
}

fn golden_first_page(preset: &str) {
    let compiled = compile(preset);
    let preview = compiled.render_page(0, 1.0);
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{preset}-p1.png"));
    if std::env::var("MARKLOOM_BLESS").ok().as_deref() == Some("1") {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("golden dir");
        }
        std::fs::write(&path, encode_png(&preview)).expect("write golden");
        return;
    }
    let bytes =
        std::fs::read(&path).unwrap_or_else(|_| panic!("missing golden {}", path.display()));
    assert_close(&format!("golden {preset}"), &preview, &bytes);
}

#[test]
fn preview_equals_pdf_github() {
    preview_equals_pdf("github");
}

#[test]
fn preview_equals_pdf_compact() {
    preview_equals_pdf("compact");
}

#[test]
fn preview_equals_pdf_academic() {
    preview_equals_pdf("academic");
}

#[test]
fn preview_equals_pdf_minimal() {
    preview_equals_pdf("minimal");
}

#[test]
fn preview_equals_pdf_print_friendly() {
    preview_equals_pdf("print-friendly");
}

#[test]
fn golden_first_page_github() {
    golden_first_page("github");
}

#[test]
fn golden_first_page_compact() {
    golden_first_page("compact");
}

#[test]
fn golden_first_page_academic() {
    golden_first_page("academic");
}

#[test]
fn golden_first_page_minimal() {
    golden_first_page("minimal");
}

#[test]
fn golden_first_page_print_friendly() {
    golden_first_page("print-friendly");
}
