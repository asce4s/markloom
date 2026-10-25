//! Zoom, page cache, and the rule for when an export must recompile.

use std::collections::HashMap;
use std::ops::Range;
use std::time::Duration;

/// Wait after the last edit before compiling.
pub const DEBOUNCE: Duration = Duration::from_millis(120);

/// How large a page is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zoom {
    FitWidth,
    FitPage,
    Percent(f32),
}

/// Pixels per Typst point, including the window scale, clamped to 0.25–4.
pub fn pixels_per_pt(
    zoom: Zoom,
    viewport_w: f32,
    viewport_h: f32,
    page_w_pt: f32,
    page_h_pt: f32,
    hidpi: f32,
) -> f32 {
    let fit = match zoom {
        Zoom::Percent(percent) => percent / 100.0,
        Zoom::FitWidth => ratio(viewport_w, page_w_pt),
        Zoom::FitPage => ratio(viewport_w, page_w_pt).min(ratio(viewport_h, page_h_pt)),
    };
    (fit * hidpi.max(1.0)).clamp(0.25, 4.0)
}

fn ratio(viewport: f32, page: f32) -> f32 {
    if page <= 0.0 { 1.0 } else { viewport / page }
}

/// Pages to rasterise: the visible range plus one page on each side.
pub fn pages_to_rasterize(first: usize, last: usize, count: usize) -> Range<usize> {
    if count == 0 {
        return 0..0;
    }
    let first = first.min(count - 1);
    let last = last.min(count - 1).max(first);
    let start = first.saturating_sub(1);
    let end = (last + 2).min(count);
    start..end
}

/// Quantise a scale so the cache key is stable.
pub fn scale_bits(pixels_per_pt: f32) -> u32 {
    (pixels_per_pt * 1000.0).round().max(0.0) as u32
}

/// Cached page bitmaps keyed by index, frame hash, and scale.
#[derive(Debug, Default, Clone)]
pub struct PageCache {
    pages: HashMap<(usize, u128, u32), Vec<u8>>,
}

impl PageCache {
    /// A stored bitmap.
    pub fn get(&self, index: usize, hash: u128, scale_bits: u32) -> Option<&[u8]> {
        self.pages
            .get(&(index, hash, scale_bits))
            .map(Vec::as_slice)
    }

    /// Store a bitmap.
    pub fn insert(&mut self, index: usize, hash: u128, scale_bits: u32, bytes: Vec<u8>) {
        self.pages.insert((index, hash, scale_bits), bytes);
    }

    /// Drop pages outside `keep`.
    pub fn retain_pages(&mut self, mut keep: impl FnMut(usize) -> bool) {
        self.pages.retain(|(index, _, _), _| keep(*index));
    }
}

/// Edit generation versus the generation that was compiled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExportGate {
    edited: u64,
    compiled: u64,
}

impl ExportGate {
    /// The buffer changed.
    pub fn note_edit(&mut self) {
        self.edited = self.edited.saturating_add(1);
    }

    /// A compile caught up with the buffer.
    pub fn note_compile(&mut self) {
        self.compiled = self.edited;
    }

    /// Export must compile again before writing.
    pub fn is_stale(&self) -> bool {
        self.edited != self.compiled
    }
}
