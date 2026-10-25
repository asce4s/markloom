//! Render a Markdown file to a page count, a PDF, or PNG pages.
//!
//! ```text
//! cargo run -p markloom-typeset --example render -- <file.md> [--preset NAME] [--pdf OUT] [--png-dir DIR] [--scale 2.0]
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use markloom_core::parse;
use markloom_mermaid::DiagramService;
use markloom_style::PresetRegistry;
use markloom_typeset::{Engine, EngineOptions, PdfMeta};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse(env::args().skip(1))?;
    let markdown = fs::read_to_string(&args.file)
        .map_err(|error| format!("read {}: {error}", args.file.display()))?;
    let doc = parse(&markdown);
    let preset = doc.front_matter.preset.clone().unwrap_or(args.preset);
    let overrides = doc
        .front_matter
        .style
        .clone()
        .into_iter()
        .collect::<Vec<_>>();
    let style = PresetRegistry::builtin()
        .resolve(&preset, &overrides)
        .map_err(|error| error.to_string())?;
    let base = args
        .file
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut engine = Engine::new(
        EngineOptions { system_fonts: true },
        Arc::new(DiagramService::with_defaults()),
    );
    let started = Instant::now();
    let compiled = engine
        .compile(&doc, &style, &base)
        .map_err(|failure| failure.to_string())?;
    let compile_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!("pages {}", compiled.page_count());
    println!("compile {compile_ms:.2} ms");

    if let Some(pdf) = &args.pdf {
        let meta = PdfMeta {
            title: doc.front_matter.title.clone(),
            author: doc.front_matter.author.clone(),
        };
        compiled
            .write_pdf(pdf, &meta)
            .map_err(|error| error.to_string())?;
    }
    if let Some(dir) = &args.png_dir {
        fs::create_dir_all(dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
        for index in 0..compiled.page_count() {
            let bitmap = compiled.render_page(index, args.scale);
            let path = dir.join(format!("page-{index}.png"));
            let bytes = encode_png(&bitmap)?;
            fs::write(&path, bytes)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
    }
    Ok(())
}

struct Args {
    file: PathBuf,
    preset: String,
    pdf: Option<PathBuf>,
    png_dir: Option<PathBuf>,
    scale: f32,
}

impl Args {
    fn parse(values: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut file = None;
        let mut preset = "github".to_string();
        let mut pdf = None;
        let mut png_dir = None;
        let mut scale = 2.0;
        let mut values = values.peekable();
        while let Some(arg) = values.next() {
            match arg.as_str() {
                "--preset" => preset = next_value(&mut values, "--preset")?,
                "--pdf" => pdf = Some(PathBuf::from(next_value(&mut values, "--pdf")?)),
                "--png-dir" => {
                    png_dir = Some(PathBuf::from(next_value(&mut values, "--png-dir")?));
                }
                "--scale" => {
                    let text = next_value(&mut values, "--scale")?;
                    scale = text
                        .parse()
                        .map_err(|_| format!("--scale {text} is not a number"))?;
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown argument {other}"));
                }
                other => {
                    if file.is_some() {
                        return Err(format!("unexpected extra path {other}"));
                    }
                    file = Some(PathBuf::from(other));
                }
            }
        }
        let file = file.ok_or_else(|| {
            "usage: render <file.md> [--preset NAME] [--pdf OUT] [--png-dir DIR] [--scale 2.0]"
                .to_string()
        })?;
        Ok(Self {
            file,
            preset,
            pdf,
            png_dir,
            scale,
        })
    }
}

fn next_value(
    values: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    flag: &str,
) -> Result<String, String> {
    values.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn encode_png(bitmap: &markloom_typeset::PageBitmap) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, bitmap.width, bitmap.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("png header: {error}"))?;
    writer
        .write_image_data(&bitmap.rgba)
        .map_err(|error| format!("png data: {error}"))?;
    drop(writer);
    Ok(bytes)
}
