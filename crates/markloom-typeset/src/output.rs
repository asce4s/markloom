//! Page bitmaps and PDF bytes.

use std::path::Path;

use typst::ecow::EcoString;
use typst::layout::Abs;
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;
use typst_render::{RenderOptions, render};

/// Straight-alpha RGBA bitmap of one page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Title and author written into the PDF document info.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdfMeta {
    pub title: Option<String>,
    pub author: Option<String>,
}

/// PDF export failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportError {
    message: String,
}

impl ExportError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ExportError {}

pub(crate) fn page_size_pt(document: &PagedDocument, index: usize) -> (f32, f32) {
    let Some(page) = document.pages().get(index) else {
        return (0.0, 0.0);
    };
    (abs_pt(page.frame.width()), abs_pt(page.frame.height()))
}

pub(crate) fn page_hash(document: &PagedDocument, index: usize) -> u128 {
    let Some(page) = document.pages().get(index) else {
        return 0;
    };
    typst::utils::hash128(&page.frame)
}

pub(crate) fn render_page(
    document: &PagedDocument,
    index: usize,
    pixels_per_pt: f32,
) -> PageBitmap {
    let Some(page) = document.pages().get(index) else {
        return PageBitmap {
            width: 0,
            height: 0,
            rgba: Vec::new(),
        };
    };
    let options = RenderOptions {
        pixel_per_pt: typst::utils::Scalar::new(f64::from(pixels_per_pt)),
        render_bleed: false,
    };
    let pixmap = render(page, &options);
    PageBitmap {
        width: pixmap.width(),
        height: pixmap.height(),
        rgba: unpremultiply(pixmap.data()),
    }
}

pub(crate) fn pdf_bytes(document: &PagedDocument, meta: &PdfMeta) -> Result<Vec<u8>, ExportError> {
    let mut document = document.clone();
    apply_meta(&mut document, meta);
    typst_pdf::pdf(&document, &PdfOptions::default()).map_err(export_diagnostics)
}

pub(crate) fn write_pdf(
    document: &PagedDocument,
    path: &Path,
    meta: &PdfMeta,
) -> Result<(), ExportError> {
    let bytes = pdf_bytes(document, meta)?;
    let temporary = path.with_extension("pdf.tmp");
    std::fs::write(&temporary, bytes).map_err(|error| ExportError::new(error.to_string()))?;
    std::fs::rename(&temporary, path).map_err(|error| ExportError::new(error.to_string()))?;
    Ok(())
}

fn apply_meta(document: &mut PagedDocument, meta: &PdfMeta) {
    let info = document.info_mut();
    if let Some(title) = &meta.title {
        info.title = Some(EcoString::from(title.as_str()));
    }
    if let Some(author) = &meta.author {
        info.author = vec![EcoString::from(author.as_str())];
    }
}

fn export_diagnostics(
    errors: impl IntoIterator<Item = typst::diag::SourceDiagnostic>,
) -> ExportError {
    let message = errors
        .into_iter()
        .map(|error| error.message.to_string())
        .collect::<Vec<_>>()
        .join("; ");
    ExportError::new(message)
}

fn abs_pt(length: Abs) -> f32 {
    let points = length.to_pt();
    if points.is_finite() {
        // Page sizes are a few hundred points. Truncation to f32 loses nothing visible.
        #[allow(clippy::cast_possible_truncation)]
        {
            points as f32
        }
    } else {
        0.0
    }
}

fn unpremultiply(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let (pixels, rest) = data.as_chunks::<4>();
    for pixel in pixels {
        let (red, green, blue, alpha) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        if alpha == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else if alpha == 255 {
            out.extend_from_slice(&[red, green, blue, alpha]);
        } else {
            let scale = 255.0 / f32::from(alpha);
            out.push(unpremultiply_channel(red, scale));
            out.push(unpremultiply_channel(green, scale));
            out.push(unpremultiply_channel(blue, scale));
            out.push(alpha);
        }
    }
    out.extend_from_slice(rest);
    out
}

fn unpremultiply_channel(channel: u8, scale: f32) -> u8 {
    let value = (f32::from(channel) * scale).round();
    if value >= 255.0 {
        255
    } else if value <= 0.0 {
        0
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            value as u8
        }
    }
}
