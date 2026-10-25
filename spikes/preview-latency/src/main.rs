//! Throwaway gpui-kit window: a code editor beside a Typst page bitmap.
//!
//! Edits debounce for 120 ms, then compile and render on the background executor.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::input::{Editor, EditorState, InputEvent};
use gpui_kit::*;
use image::{ImageBuffer, ImageEncoder, Rgba};
use preview_latency::{PageImage, SpikeWorld, wrap_body};

struct Preview {
    editor: Entity<EditorState>,
    world: Arc<Mutex<SpikeWorld>>,
    page: Option<Arc<Image>>,
    status: SharedString,
    _subscriptions: Vec<Subscription>,
    _task: Option<Task<()>>,
}

impl Preview {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let body = preview_latency::twenty_page_body();
        let world = Arc::new(Mutex::new(SpikeWorld::new(wrap_body(&body))));
        let editor = cx.new(|cx| EditorState::new(window, cx).default_value(body));
        let subscription =
            cx.subscribe_in(&editor, window, move |this, _editor, event, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.schedule(cx);
                }
            });
        let preview = Self {
            editor,
            world,
            page: None,
            status: "ready".into(),
            _subscriptions: vec![subscription],
            _task: None,
        };
        // First paint, without waiting for a keystroke.
        let world = Arc::clone(&preview.world);
        cx.spawn(async move |this, cx| {
            let rendered = cx
                .background_spawn(async move { compile_current(&world) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.apply(rendered, cx);
            });
        })
        .detach();
        preview
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        let world = Arc::clone(&self.world);
        let editor = self.editor.clone();
        self.status = "waiting".into();
        cx.notify();
        self._task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
            let text = this
                .update(cx, |_this, cx| editor.read(cx).value().to_string())
                .unwrap_or_default();
            let rendered = cx
                .background_spawn(async move {
                    let mut world = world.lock().expect("world lock");
                    world.replace(&wrap_body(&text));
                    let started = Instant::now();
                    let compiled = world.compile();
                    let compile_ms = started.elapsed().as_secs_f64() * 1000.0;
                    match compiled {
                        Ok(document) => {
                            let render_started = Instant::now();
                            let page = preview_latency::render_page(&document, 0, 2.0);
                            let render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
                            typst::comemo::evict(30);
                            Ok((page, compile_ms, render_ms, document.pages().len()))
                        }
                        Err(message) => Err(message),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| this.apply(rendered, cx));
        }));
    }

    fn apply(
        &mut self,
        rendered: Result<(PageImage, f64, f64, usize), String>,
        cx: &mut Context<Self>,
    ) {
        match rendered {
            Ok((page, compile_ms, render_ms, pages)) => {
                self.status =
                    format!("pages {pages}  compile {compile_ms:.1} ms  render {render_ms:.1} ms")
                        .into();
                self.page = Some(Arc::new(png_image(&page)));
                eprintln!(
                    "compile {compile_ms:.2} ms, render {render_ms:.2} ms, total {:.2} ms",
                    compile_ms + render_ms
                );
            }
            Err(message) => {
                self.status = format!("error: {message}").into();
            }
        }
        cx.notify();
    }
}

fn compile_current(world: &Arc<Mutex<SpikeWorld>>) -> Result<(PageImage, f64, f64, usize), String> {
    let world = world.lock().expect("world lock");
    let started = Instant::now();
    let document = world.compile()?;
    let compile_ms = started.elapsed().as_secs_f64() * 1000.0;
    let render_started = Instant::now();
    let page = preview_latency::render_page(&document, 0, 2.0);
    let render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
    typst::comemo::evict(30);
    Ok((page, compile_ms, render_ms, document.pages().len()))
}

fn png_image(page: &PageImage) -> Image {
    let buffer: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_raw(page.width, page.height, page.rgba.clone())
            .expect("rgba length matches width * height");
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            buffer.as_raw(),
            page.width,
            page.height,
            image::ExtendedColorType::Rgba8,
        )
        .expect("png encode");
    Image::from_bytes(ImageFormat::Png, bytes)
}

impl Render for Preview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(self.status.clone()),
            )
            .child(
                h_flex()
                    .flex_1()
                    .items_stretch()
                    .overflow_hidden()
                    .child(
                        div()
                            .w(relative(0.45))
                            .h_full()
                            .border_r_1()
                            .border_color(cx.theme().border)
                            .child(Editor::new(&self.editor).h(relative(1.0))),
                    )
                    .child(
                        div()
                            .id("page-preview")
                            .flex_1()
                            .h_full()
                            .overflow_y_scroll()
                            .p_4()
                            .child(match &self.page {
                                Some(image) => img(image.clone()).max_w_full().into_any_element(),
                                None => div().child("Compiling…").into_any_element(),
                            }),
                    ),
            )
    }
}

fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Preview::new(window, cx))
            })
            .expect("failed to open window");
        });
}
