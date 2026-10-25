//! Debounced compile and retained page images.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{
    AppContext as _, BackgroundExecutor, Context, Entity, ImageId, RenderImage, Subscription,
};
use markloom_style::StyleSheet;
use markloom_typeset::{CompileFailure, Compiled, Engine};

use crate::document::{Document, DocumentEvent};
use crate::preview_pipeline::{
    self, PageKey, RasterPlan, Zoom, pixels_per_pt, plan_raster, scale_bits,
};
use crate::style_state::{StyleEvent, StyleState};

/// What the preview column is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewStatus {
    LoadingFonts,
    Compiling,
    Ready { pages: usize, diagrams: usize },
    Error(String),
}

/// Compile generation, page images, and zoom.
pub struct PreviewState {
    document: Entity<Document>,
    style: Entity<StyleState>,
    engine: Option<Arc<Mutex<Engine>>>,
    fonts: Vec<String>,
    compiled: Option<Compiled>,
    held: HashMap<PageKey, Arc<RenderImage>>,
    show: Vec<(usize, PageKey)>,
    zoom: Zoom,
    sync: bool,
    visible: (usize, usize),
    viewport_w: f32,
    viewport_h: f32,
    hidpi: f32,
    scale_bits: u32,
    status: PreviewStatus,
    generation: u64,
    compile_count: u64,
    hold_ms: Arc<AtomicU64>,
    entered: Arc<AtomicBool>,
    _document_sub: Subscription,
    _style_sub: Subscription,
}

impl PreviewState {
    /// Fonts are not loaded yet. Compiles wait until [`Self::set_engine`].
    pub fn new(
        document: Entity<Document>,
        style: Entity<StyleState>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(None, document, style, cx)
    }

    /// Tests and the font loader hand over an engine that already exists.
    pub fn with_engine(
        engine: Engine,
        document: Entity<Document>,
        style: Entity<StyleState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::build(Some(Arc::new(Mutex::new(engine))), document, style, cx);
        this.schedule(cx);
        this
    }

    /// Replace a missing engine and compile. Font discovery stays with the caller.
    pub fn set_engine(&mut self, engine: Engine, cx: &mut Context<Self>) {
        let engine = Arc::new(Mutex::new(engine));
        self.fonts = font_snapshot(&engine);
        self.engine = Some(engine);
        self.schedule(cx);
    }

    /// Image currently shown for `index`, which may be a previous scale or hash.
    pub fn page_image(&self, index: usize) -> Option<Arc<RenderImage>> {
        let key = self.show.iter().find(|(page, _)| *page == index)?.1;
        self.held.get(&key).cloned()
    }

    /// Page the caret line landed on, from the source map.
    pub fn scroll_target(&self, line: u32) -> Option<usize> {
        self.compiled
            .as_ref()?
            .source_map
            .locate_line(line)
            .map(|location| location.page)
    }

    /// Compiles that actually started, including ones whose result was ignored.
    pub fn compile_count(&self) -> u64 {
        self.compile_count
    }

    /// Last compile that was applied. `None` until the first successful compile.
    pub fn compiled(&self) -> Option<&Compiled> {
        self.compiled.as_ref()
    }

    /// Engine handle, once fonts have loaded. Export uses it when the preview is stale.
    pub fn engine_handle(&self) -> Option<Arc<Mutex<Engine>>> {
        self.engine.clone()
    }

    /// Pages in the last compile that was applied.
    pub fn page_count(&self) -> Option<usize> {
        self.compiled.as_ref().map(Compiled::page_count)
    }

    /// Width in points of the first applied page.
    pub fn page_width_pt(&self) -> Option<f32> {
        self.compiled
            .as_ref()
            .map(|compiled| compiled.page_size_pt(0).0)
    }

    /// How many page images are retained.
    pub fn held_len(&self) -> usize {
        self.held.len()
    }

    /// Whether this render-image id is still in the cache.
    pub fn holds(&self, id: ImageId) -> bool {
        self.held.values().any(|image| image.id == id)
    }

    /// Visible page count, at least one so an empty span still has a bound.
    pub fn visible_len(&self) -> usize {
        self.visible
            .1
            .saturating_sub(self.visible.0)
            .saturating_add(1)
    }

    /// Pages the preview viewport currently shows, inclusive.
    pub fn visible(&self) -> (usize, usize) {
        self.visible
    }

    /// Pages the preview viewport currently shows, inclusive.
    ///
    /// The same span does not start another draw.
    pub fn set_visible(&mut self, visible: (usize, usize), cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        let generation = self.generation;
        self.start_raster(generation, cx);
    }

    /// Current zoom.
    pub fn zoom(&self) -> Zoom {
        self.zoom
    }

    /// Step to the next larger zoom and keep the images already on screen.
    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(preview_pipeline::zoom_in(self.zoom, self.fit_percent()), cx);
    }

    /// Step to the next smaller zoom and keep the images already on screen.
    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(
            preview_pipeline::zoom_out(self.zoom, self.fit_percent()),
            cx,
        );
    }

    /// Switch zoom. Old images stay until the new scale replaces them.
    pub fn set_zoom(&mut self, zoom: Zoom, cx: &mut Context<Self>) {
        self.zoom = zoom;
        self.start_raster(self.generation, cx);
    }

    /// Display size of one page in logical pixels.
    pub fn page_display_px(&self, index: usize) -> Option<(f32, f32)> {
        let compiled = self.compiled.as_ref()?;
        if index >= compiled.page_count() {
            return None;
        }
        let (width, height) = compiled.page_size_pt(index);
        let logical = self.scale() / self.hidpi.max(1.0);
        Some((width * logical, height * logical))
    }

    /// Markdown line under a point on a page, in Typst points from the top.
    pub fn line_at(&self, page: usize, y_pt: f32) -> Option<u32> {
        self.compiled.as_ref()?.source_map.line_at(page, y_pt)
    }

    /// Typst points from the top of a page for a click `y_px` logical pixels down.
    pub fn y_pt(&self, index: usize, y_px: f32) -> Option<f32> {
        let compiled = self.compiled.as_ref()?;
        if index >= compiled.page_count() {
            return None;
        }
        let logical = self.scale() / self.hidpi.max(1.0);
        if logical <= 0.0 {
            return None;
        }
        Some(y_px.max(0.0) / logical)
    }

    /// Remember the preview canvas size. Fit modes raster again.
    pub fn set_viewport(&mut self, width: f32, height: f32, cx: &mut Context<Self>) {
        if (self.viewport_w - width).abs() < 0.5 && (self.viewport_h - height).abs() < 0.5 {
            return;
        }
        self.viewport_w = width.max(1.0);
        self.viewport_h = height.max(1.0);
        if matches!(self.zoom, Zoom::FitWidth | Zoom::FitPage) {
            self.start_raster(self.generation, cx);
        }
    }

    /// Window scale. Raster size includes it; the zoom steps stay logical.
    pub fn set_hidpi(&mut self, scale: f32, cx: &mut Context<Self>) {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        if (self.hidpi - scale).abs() < 0.01 {
            return;
        }
        self.hidpi = scale;
        self.start_raster(self.generation, cx);
    }

    /// Follow the caret only while this is on.
    pub fn sync_scroll(&self) -> bool {
        self.sync
    }

    /// Turn caret-following on or off.
    pub fn set_sync(&mut self, sync: bool) {
        self.sync = sync;
    }

    /// Family names captured when the engine was installed.
    ///
    /// This does not lock the engine, so a compile can hold that mutex.
    pub fn font_families(&self) -> Vec<String> {
        self.fonts.clone()
    }

    pub fn status(&self) -> &PreviewStatus {
        &self.status
    }

    /// Next compile waits `hold` after it has started, once.
    ///
    /// Tests use this to edit again before the result is applied. Production
    /// leaves it at zero.
    pub fn arm_compile_hold(&self, hold: Duration) {
        self.hold_ms.store(
            u64::try_from(hold.as_millis()).unwrap_or(u64::MAX),
            Ordering::SeqCst,
        );
        self.entered.store(false, Ordering::SeqCst);
    }

    /// True once a held compile has entered its wait.
    pub fn compile_entered(&self) -> bool {
        self.entered.load(Ordering::SeqCst)
    }

    fn build(
        engine: Option<Arc<Mutex<Engine>>>,
        document: Entity<Document>,
        style: Entity<StyleState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let document_sub = cx.subscribe(&document, |this, _document, event: &DocumentEvent, cx| {
            if matches!(event, DocumentEvent::TextChanged | DocumentEvent::Reloaded) {
                this.schedule(cx);
            }
        });
        let style_sub = cx.subscribe(&style, |this, _style, event: &StyleEvent, cx| {
            if matches!(event, StyleEvent::Changed) {
                this.schedule(cx);
            }
        });
        let fonts = engine.as_ref().map(font_snapshot).unwrap_or_default();
        Self {
            document,
            style,
            engine,
            fonts,
            compiled: None,
            held: HashMap::new(),
            show: Vec::new(),
            zoom: Zoom::Percent(100.0),
            sync: true,
            visible: (0, 0),
            viewport_w: 800.0,
            viewport_h: 600.0,
            hidpi: 1.0,
            scale_bits: scale_bits(1.0),
            status: PreviewStatus::LoadingFonts,
            generation: 0,
            compile_count: 0,
            hold_ms: Arc::new(AtomicU64::new(0)),
            entered: Arc::new(AtomicBool::new(false)),
            _document_sub: document_sub,
            _style_sub: style_sub,
        }
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.engine.is_none() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.status = PreviewStatus::Compiling;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(preview_pipeline::DEBOUNCE)
                .await;
            let executor = cx.background_executor().clone();
            let job = this
                .update(cx, |this, cx| {
                    if this.generation != generation {
                        return None;
                    }
                    this.compile_count = this.compile_count.wrapping_add(1);
                    Some(this.capture(cx, executor))
                })
                .ok()
                .flatten();
            let Some(job) = job else {
                return;
            };
            let result = cx.background_spawn(job.run()).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.apply(generation, result, cx);
            });
        })
        .detach();
    }

    fn capture(&self, cx: &mut Context<Self>, executor: BackgroundExecutor) -> CompileJob {
        let text = self.document.read(cx).text().to_string();
        let base = self.document.read(cx).workspace().root().to_path_buf();
        let style = Arc::clone(self.style.read(cx).sheet());
        let engine = Arc::clone(self.engine.as_ref().expect("engine"));
        CompileJob {
            text,
            style,
            base,
            engine,
            hold_ms: Arc::clone(&self.hold_ms),
            entered: Arc::clone(&self.entered),
            executor,
        }
    }

    fn apply(
        &mut self,
        generation: u64,
        result: Result<Compiled, CompileFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(compiled) => {
                let pages = compiled.page_count();
                let diagrams = compiled.diagram_count;
                self.compiled = Some(compiled);
                self.status = PreviewStatus::Ready { pages, diagrams };
                cx.notify();
                self.start_raster(generation, cx);
            }
            Err(error) => {
                self.status = PreviewStatus::Error(error.to_string());
                cx.notify();
            }
        }
    }

    fn start_raster(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some(plan) = self.current_plan() else {
            return;
        };
        self.show.clone_from(&plan.show);
        self.drop_keys(&plan.drop, cx);
        let Some(compiled) = self.compiled.clone() else {
            return;
        };
        let scale = self.scale();
        for key in plan.draw {
            let compiled = compiled.clone();
            cx.spawn(async move |this, cx| {
                let image = cx
                    .background_spawn(async move { render_page(&compiled, key.index, scale) })
                    .await;
                let _ = this.update(cx, |this, cx| this.install(generation, key, image, cx));
            })
            .detach();
        }
    }

    fn install(
        &mut self,
        generation: u64,
        key: PageKey,
        image: Option<Arc<RenderImage>>,
        cx: &mut Context<Self>,
    ) {
        let Some(image) = image else {
            return;
        };
        if self.generation != generation {
            cx.drop_image(image, None);
            return;
        }
        self.held.insert(key, image);
        if let Some(plan) = self.current_plan() {
            self.show = plan.show;
            self.drop_keys(&plan.drop, cx);
        }
        cx.notify();
    }

    fn current_plan(&mut self) -> Option<RasterPlan> {
        let compiled = self.compiled.as_ref()?;
        let count = compiled.page_count();
        if count == 0 {
            return None;
        }
        let (page_w, page_h) = compiled.page_size_pt(0);
        let scale = pixels_per_pt(
            self.zoom,
            self.viewport_w,
            self.viewport_h,
            page_w,
            page_h,
            self.hidpi,
        );
        self.scale_bits = scale_bits(scale);
        let hashes: Vec<u128> = (0..count).map(|index| compiled.page_hash(index)).collect();
        let held: Vec<PageKey> = self.held.keys().copied().collect();
        Some(plan_raster(&held, &hashes, self.visible, self.scale_bits))
    }

    /// Logical fit size. Zoom steps compare against this, not the hidpi raster.
    fn fit_percent(&self) -> f32 {
        let Some(compiled) = &self.compiled else {
            return 100.0;
        };
        if compiled.page_count() == 0 {
            return 100.0;
        }
        let (page_w, page_h) = compiled.page_size_pt(0);
        pixels_per_pt(
            self.zoom,
            self.viewport_w,
            self.viewport_h,
            page_w,
            page_h,
            1.0,
        ) * 100.0
    }

    fn scale(&self) -> f32 {
        let Some(compiled) = &self.compiled else {
            return 1.0;
        };
        if compiled.page_count() == 0 {
            return 1.0;
        }
        let (page_w, page_h) = compiled.page_size_pt(0);
        pixels_per_pt(
            self.zoom,
            self.viewport_w,
            self.viewport_h,
            page_w,
            page_h,
            self.hidpi,
        )
    }

    fn drop_keys(&mut self, keys: &[PageKey], cx: &mut Context<Self>) {
        for key in keys {
            if let Some(old) = self.held.remove(key) {
                // `None`: this update is not inside a window update, so the
                // window is still in `App::windows` and drop_image finds it.
                cx.drop_image(old, None);
            }
        }
    }
}

struct CompileJob {
    text: String,
    style: Arc<StyleSheet>,
    base: PathBuf,
    engine: Arc<Mutex<Engine>>,
    hold_ms: Arc<AtomicU64>,
    entered: Arc<AtomicBool>,
    executor: BackgroundExecutor,
}

impl CompileJob {
    async fn run(self) -> Result<Compiled, CompileFailure> {
        let hold = self.hold_ms.swap(0, Ordering::SeqCst);
        if hold > 0 {
            self.entered.store(true, Ordering::SeqCst);
            let wait = self.executor.timer(Duration::from_millis(hold));
            wait.await;
        }
        let doc = markloom_core::parse(&self.text);
        let mut engine = self.engine.lock().expect("engine lock");
        engine.compile(&doc, &self.style, &self.base)
    }
}

fn font_snapshot(engine: &Arc<Mutex<Engine>>) -> Vec<String> {
    engine.lock().expect("engine lock").font_families()
}

fn render_page(compiled: &Compiled, index: usize, scale: f32) -> Option<Arc<RenderImage>> {
    let bitmap = compiled.render_page_premultiplied(index, scale);
    if bitmap.width == 0 || bitmap.height == 0 {
        return None;
    }
    let bgra = preview_pipeline::to_bgra(&bitmap.rgba);
    let buffer = image::RgbaImage::from_raw(bitmap.width, bitmap.height, bgra)?;
    let frame = image::Frame::new(buffer);
    Some(Arc::new(RenderImage::new(vec![frame])))
}
