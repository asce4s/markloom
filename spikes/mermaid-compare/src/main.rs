//! Compare mermaid-rs-renderer and rusty-mermaid on fixtures/mermaid.
//!
//! For each diagram: render SVG, parse it with usvg, embed it in Typst, and
//! write a PNG of the Typst page. Status is printed as ok / wrong / error.
//! "wrong" is reserved for a render that parses but draws no ink.

use std::fs;
use std::path::{Path, PathBuf};

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

fn main() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mermaid");
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out");
    fs::create_dir_all(&out_dir).expect("create out dir");

    let mut files: Vec<_> = fs::read_dir(&fixtures)
        .expect("read fixtures")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "mmd"))
        .collect();
    files.sort();

    println!("type,crate,status,detail");
    for path in &files {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("diagram");
        let source = fs::read_to_string(path).expect("read fixture");
        report("mermaid-rs-renderer", name, &out_dir, &render_mrr(&source));
        report("rusty-mermaid", name, &out_dir, &render_rusty(&source));
    }
}

fn report(crate_name: &str, diagram: &str, out_dir: &Path, result: &Result<String, String>) {
    let status = match result {
        Err(error) => {
            println!("{diagram},{crate_name},error,{error}");
            return;
        }
        Ok(svg) => match check_svg(svg, diagram, crate_name, out_dir) {
            Ok(detail) => {
                println!("{diagram},{crate_name},ok,{detail}");
                "ok"
            }
            Err(error) => {
                println!("{diagram},{crate_name},wrong,{error}");
                "wrong"
            }
        },
    };
    let _ = status;
}

fn render_mrr(source: &str) -> Result<String, String> {
    let mut options = mermaid_rs_renderer::RenderOptions::mermaid_default();
    options.theme.font_family = "DejaVu Sans Mono".to_string();
    mermaid_rs_renderer::render_with_options(source, options).map_err(|error| error.to_string())
}

fn render_rusty(source: &str) -> Result<String, String> {
    let theme = rusty_mermaid::Theme::default();
    rusty_mermaid::to_svg(source, &theme).map_err(|error| error.to_string())
}

fn check_svg(svg: &str, diagram: &str, crate_name: &str, out_dir: &Path) -> Result<String, String> {
    if !svg.contains("<svg") {
        return Err("output is not an svg".into());
    }
    usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|error| error.to_string())?;
    let slug = crate_name.replace('-', "_");
    let png_path = out_dir.join(format!("{diagram}-{slug}.png"));
    let (width, height, ink) = embed_in_typst(svg, &png_path)?;
    if ink < 50 {
        return Err(format!(
            "typst page has almost no ink ({ink} px, {width}x{height})"
        ));
    }
    let textish = svg.contains("<text") || svg.contains("<path");
    if !textish {
        return Err("svg has neither text nor paths".into());
    }
    Ok(format!("{width}x{height} ink={ink}"))
}

fn embed_in_typst(svg: &str, png_path: &Path) -> Result<(u32, u32, u32), String> {
    let main = "#set page(width: auto, height: auto, margin: 12pt)\n#image(\"/diagram.svg\", format: \"svg\")\n";
    let mut world = SvgWorld::new(main.to_string(), svg.to_string());
    let Warned {
        output,
        warnings: _,
    } = typst::compile::<PagedDocument>(&world);
    let document = output.map_err(|errors| {
        errors
            .iter()
            .map(|error| error.message.to_string())
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let page = document
        .pages()
        .first()
        .ok_or_else(|| "typst produced no pages".to_string())?;
    let pixmap = render(
        page,
        &RenderOptions {
            pixel_per_pt: typst::utils::Scalar::new(2.0),
            render_bleed: false,
        },
    );
    let rgba = unpremultiply(pixmap.data());
    let ink = ink_pixels(&rgba);
    write_png(png_path, pixmap.width(), pixmap.height(), &rgba)?;
    let _ = &mut world;
    Ok((pixmap.width(), pixmap.height(), ink))
}

fn ink_pixels(rgba: &[u8]) -> u32 {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| {
            let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
            a > 16 && (r < 245 || g < 245 || b < 245)
        })
        .count() as u32
}

fn unpremultiply(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for pixel in data.as_chunks::<4>().0 {
        let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        if a == 0 || a == 255 {
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

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = fs::File::create(path).map_err(|error| error.to_string())?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(rgba)
        .map_err(|error| error.to_string())
}

struct SvgWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    source: Source,
    main: FileId,
    svg: String,
}

impl SvgWorld {
    fn new(typst_source: String, svg: String) -> Self {
        let mut fonts = FontStore::new();
        fonts.extend(embedded());
        let main = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("main.typ").expect("virtual path"),
        )
        .intern();
        Self {
            library: LazyHash::new(Library::builder().build()),
            fonts,
            source: Source::new(main, typst_source),
            main,
            svg,
        }
    }
}

impl World for SvgWorld {
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
            Err(FileError::NotSource)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main {
            return Ok(Bytes::from_string(self.source.text().to_string()));
        }
        let path = id.vpath().get_without_slash();
        if path.ends_with("diagram.svg") {
            Ok(Bytes::from_string(self.svg.clone()))
        } else {
            Err(FileError::NotFound(PathBuf::from(path)))
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}
