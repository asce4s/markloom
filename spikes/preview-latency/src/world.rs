//! Minimal in-memory Typst world used by the latency spike.
//!
//! One `Source` stays alive and is updated with `Source::replace` so comemo
//! can reuse work from the previous compile.

use std::path::PathBuf;
use std::time::Instant;

use typst::LibraryExt;
use typst::diag::{FileError, FileResult, Warned};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_kit::fonts::{FontStore, embedded};
use typst_layout::PagedDocument;
use typst_render::{RenderOptions, render};

pub struct PageImage {
    pub width: u32,
    pub height: u32,
    /// Straight (unpremultiplied) RGBA.
    pub rgba: Vec<u8>,
}

pub struct Timing {
    pub compile_ms: f64,
    pub render_ms: f64,
    pub total_ms: f64,
}

pub struct Report {
    pub page_count: usize,
    pub warmup_ms: f64,
    pub samples: Vec<Timing>,
    pub median_total_ms: f64,
    pub passed: bool,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "pages {}", self.page_count)?;
        writeln!(f, "warmup compile {warmup:.2} ms", warmup = self.warmup_ms)?;
        writeln!(f, "samples {}", self.samples.len())?;
        for (i, sample) in self.samples.iter().enumerate() {
            writeln!(
                f,
                "  {i}: compile {compile:.2} ms, render {render:.2} ms, total {total:.2} ms",
                compile = sample.compile_ms,
                render = sample.render_ms,
                total = sample.total_ms
            )?;
        }
        writeln!(
            f,
            "median total {median:.2} ms",
            median = self.median_total_ms
        )?;
        writeln!(f, "passed {}", self.passed)
    }
}

/// Body text: headings, a markdown table, a fenced code block, and enough
/// paragraphs that the wrapped Typst document fills about 20 A4 pages.
pub fn twenty_page_body() -> String {
    let mut body = String::new();
    body.push_str("Markloom latency fixture\n\n");
    body.push_str("EDITME sits in the first paragraph so a one-word edit is local.\n\n");
    body.push_str("| Column | Kind | Notes |\n| --- | --- | --- |\n| alpha | text | row |\n| beta | code | row |\n| gamma | table | row |\n\n");
    body.push_str("```rust\nfn main() {\n    println!(\"markloom\");\n}\n```\n\n");
    let sentence = "The quick brown fox jumps over the lazy dog while the preview recompiles a long document. ";
    for section in 1..=32 {
        body.push_str(&format!("Section {section}\n\n"));
        for paragraph in 0..6 {
            body.push_str(&format!("Paragraph {section}.{paragraph}. "));
            for _ in 0..8 {
                body.push_str(sentence);
            }
            body.push_str("\n\n");
        }
    }
    body
}

/// Wrap editor text as Typst: page setup, one real heading, one real table,
/// one real code block, then every paragraph as a string literal.
pub fn wrap_body(body: &str) -> String {
    let mut source = String::from(
        "#set page(paper: \"a4\", margin: 1.5cm)\n\
         #set text(size: 11pt)\n\
         #set par(leading: 0.65em, spacing: 0.65em)\n\
         = Fixture\n\
         #table(columns: 3, [Column], [Kind], [Notes], [alpha], [text], [row])\n\
         #raw(block: true, lang: \"rust\", \"fn main() { println!(\\\"markloom\\\"); }\")\n",
    );
    for paragraph in body.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        source.push_str(&str_lit(paragraph));
        source.push('\n');
    }
    source
}

fn str_lit(text: &str) -> String {
    let mut out = String::from("#\"");
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

pub struct SpikeWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    source: Source,
    main: FileId,
}

impl SpikeWorld {
    pub fn new(typst_source: String) -> Self {
        let mut fonts = FontStore::new();
        fonts.extend(embedded());
        let main = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("main.typ").expect("main.typ is a valid virtual path"),
        )
        .intern();
        let source = Source::new(main, typst_source);
        Self {
            library: LazyHash::new(Library::builder().build()),
            fonts,
            source,
            main,
        }
    }

    pub fn replace(&mut self, typst_source: &str) {
        self.source.replace(typst_source);
    }

    pub fn compile(&self) -> Result<PagedDocument, String> {
        let Warned {
            output,
            warnings: _,
        } = typst::compile::<PagedDocument>(self);
        output.map_err(|errors| {
            errors
                .iter()
                .map(|error| error.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })
    }
}

impl World for SpikeWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(PathBuf::from(format!("{id:?}"))))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let source = self.source(id)?;
        Ok(Bytes::from_string(source.text().to_string()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

pub fn render_page(document: &PagedDocument, index: usize, pixels_per_pt: f32) -> PageImage {
    let page = &document.pages()[index];
    let opts = RenderOptions {
        pixel_per_pt: typst::utils::Scalar::new(f64::from(pixels_per_pt)),
        render_bleed: false,
    };
    let pixmap = render(page, &opts);
    let width = pixmap.width();
    let height = pixmap.height();
    let rgba = unpremultiply(pixmap.data());
    PageImage {
        width,
        height,
        rgba,
    }
}

fn unpremultiply(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for pixel in data.as_chunks::<4>().0 {
        let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else if a == 255 {
            out.extend_from_slice(&[r, g, b, a]);
        } else {
            let scale = 255.0 / f32::from(a);
            out.push((f32::from(r) * scale).round().min(255.0) as u8);
            out.push((f32::from(g) * scale).round().min(255.0) as u8);
            out.push((f32::from(b) * scale).round().min(255.0) as u8);
            out.push(a);
        }
    }
    out
}

/// Warm up once, then measure eleven one-word edits. Each sample is
/// `Source::replace` + compile + render of page 0 at 2 px/pt.
pub fn compile_and_render() -> Report {
    let mut body = twenty_page_body();
    let mut world = SpikeWorld::new(wrap_body(&body));
    let warmup_started = Instant::now();
    let first = world.compile().expect("warmup compile");
    let warmup_ms = warmup_started.elapsed().as_secs_f64() * 1000.0;
    let page_count = first.pages().len();
    // Drop the document before the next compile so frames are not held twice.
    drop(first);
    typst::comemo::evict(30);

    let mut samples = Vec::with_capacity(11);
    let mut token = "EDITME".to_string();
    for index in 0..11 {
        // A new token every time, so a sample cannot be a cache hit of a
        // previous document that Typst has already compiled.
        let next = format!("ED{index:04}");
        body = body.replacen(&token, &next, 1);
        token = next;
        let started = Instant::now();
        world.replace(&wrap_body(&body));
        let compiled = world.compile().expect("recompile");
        let compile_ms = started.elapsed().as_secs_f64() * 1000.0;
        let render_started = Instant::now();
        let _page = render_page(&compiled, 0, 2.0);
        let render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
        drop(compiled);
        typst::comemo::evict(30);
        samples.push(Timing {
            compile_ms,
            render_ms,
            total_ms: compile_ms + render_ms,
        });
    }

    let mut totals: Vec<f64> = samples.iter().map(|sample| sample.total_ms).collect();
    totals.sort_by(|a, b| a.partial_cmp(b).expect("finite timings"));
    let median_total_ms = totals[totals.len() / 2];
    Report {
        page_count,
        warmup_ms,
        samples,
        median_total_ms,
        passed: page_count >= 20 && median_total_ms < 150.0,
    }
}
