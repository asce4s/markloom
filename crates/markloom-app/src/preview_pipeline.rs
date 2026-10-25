//! Zoom, visibility, and the pure decision of which page images to draw.

use std::ops::Range;
use std::time::Duration;

/// Wait after the last edit before compiling.
pub const DEBOUNCE: Duration = Duration::from_millis(120);

/// Percent steps for the zoom buttons.
pub const ZOOM_STEPS: [f32; 9] = [50.0, 67.0, 75.0, 90.0, 100.0, 110.0, 125.0, 150.0, 200.0];

/// How large a page is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zoom {
    FitWidth,
    FitPage,
    Percent(f32),
}

/// Next larger step. Fit modes use `fit_percent` as the current size.
pub fn zoom_in(zoom: Zoom, fit_percent: f32) -> Zoom {
    Zoom::Percent(step_from(effective_percent(zoom, fit_percent), true))
}

/// Next smaller step. Fit modes use `fit_percent` as the current size.
pub fn zoom_out(zoom: Zoom, fit_percent: f32) -> Zoom {
    Zoom::Percent(step_from(effective_percent(zoom, fit_percent), false))
}

fn effective_percent(zoom: Zoom, fit_percent: f32) -> f32 {
    match zoom {
        Zoom::Percent(percent) => percent,
        Zoom::FitWidth | Zoom::FitPage => fit_percent,
    }
}

fn step_from(current: f32, larger: bool) -> f32 {
    if larger {
        ZOOM_STEPS
            .iter()
            .copied()
            .find(|step| *step > current + 0.01)
            .unwrap_or(200.0)
    } else {
        ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|step| *step < current - 0.01)
            .unwrap_or(50.0)
    }
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

/// Inclusive page indices that intersect a uniform list viewport.
pub fn visible_span(
    scroll_offset_y: f32,
    viewport_h: f32,
    item_h: f32,
    count: usize,
) -> (usize, usize) {
    if count == 0 || item_h <= 0.0 {
        return (0, 0);
    }
    let offset = if scroll_offset_y.is_finite() {
        scroll_offset_y.max(0.0)
    } else {
        0.0
    };
    let viewport = if viewport_h.is_finite() {
        viewport_h.max(0.0)
    } else {
        0.0
    };
    let first = clamp_index((offset / item_h).floor(), count);
    if viewport <= 0.0 {
        return (first, first);
    }
    let ratio = (offset + viewport) / item_h;
    let last_raw = if (ratio - ratio.floor()).abs() < 1.0e-3 {
        ratio.floor() - 1.0
    } else {
        ratio.floor()
    };
    (first, clamp_index(last_raw, count).max(first))
}

fn clamp_index(index: f32, count: usize) -> usize {
    if !index.is_finite() || index <= 0.0 {
        0
    } else if index >= count as f32 {
        count - 1
    } else {
        index as usize
    }
}

/// Quantise a scale so the cache key is stable.
pub fn scale_bits(pixels_per_pt: f32) -> u32 {
    (pixels_per_pt * 1000.0).round().max(0.0) as u32
}

/// One cached page image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageKey {
    pub index: usize,
    pub hash: u128,
    pub scale_bits: u32,
}

/// What to draw, drop, and keep on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterPlan {
    pub draw: Vec<PageKey>,
    pub drop: Vec<PageKey>,
    pub show: Vec<(usize, PageKey)>,
}

/// Decide page images from what is already held.
///
/// `show` prefers the exact key, otherwise a held image of the same page
/// (same hash first). A held image is dropped only when it is outside the
/// visible pages ±3, or when the exact replacement for that page is held.
/// The image currently shown for a page is never the one dropped.
pub fn plan_raster(
    held: &[PageKey],
    hashes: &[u128],
    visible: (usize, usize),
    scale_bits: u32,
) -> RasterPlan {
    let count = hashes.len();
    if count == 0 {
        return RasterPlan {
            draw: Vec::new(),
            drop: held.to_vec(),
            show: Vec::new(),
        };
    }
    let first = visible.0.min(count - 1);
    let last = visible.1.min(count - 1).max(first);
    let want_lo = first.saturating_sub(1);
    let want_hi = (last + 1).min(count - 1);
    let keep_lo = first.saturating_sub(3);
    let keep_hi = (last + 3).min(count - 1);
    let needed = |index: usize| PageKey {
        index,
        hash: hashes[index],
        scale_bits,
    };

    let mut draw = Vec::new();
    let mut show = Vec::new();
    for (index, hash) in hashes.iter().enumerate().take(want_hi + 1).skip(want_lo) {
        let exact = PageKey {
            index,
            hash: *hash,
            scale_bits,
        };
        if !held.contains(&exact) {
            draw.push(exact);
        }
        if held.contains(&exact) {
            show.push((index, exact));
            continue;
        }
        let same_hash = held
            .iter()
            .copied()
            .find(|key| key.index == index && key.hash == *hash);
        let any = held.iter().copied().find(|key| key.index == index);
        if let Some(key) = same_hash.or(any) {
            show.push((index, key));
        }
    }

    let mut drop = Vec::new();
    for key in held {
        let in_keep = key.index < count && (keep_lo..=keep_hi).contains(&key.index);
        let superseded = key.index < count && (want_lo..=want_hi).contains(&key.index) && {
            let exact = needed(key.index);
            *key != exact && held.contains(&exact)
        };
        if in_keep && !superseded {
            continue;
        }
        let only_shown = show.iter().any(|(_, shown)| shown == key)
            && held.iter().filter(|other| other.index == key.index).count() == 1;
        if !only_shown {
            drop.push(*key);
        }
    }

    RasterPlan { draw, drop, show }
}

/// Swap R and B and stay premultiplied. GPUI wants premultiplied BGRA.
pub fn to_bgra(rgba_premultiplied: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba_premultiplied.len());
    let (pixels, rest) = rgba_premultiplied.as_chunks::<4>();
    for pixel in pixels {
        out.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    out.extend_from_slice(rest);
    out
}
