//! gpui-kit window: files, editor, paged preview, and style.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use gpui_kit::actions;
use gpui_kit::base::input::{Diagnostic, DiagnosticSeverity};
use gpui_kit::base::{Selectable as _, h_flex, v_flex};
use gpui_kit::component::button::Button;
use gpui_kit::component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_kit::component::dock::{
    self, DockArea, DockPlacement, DockSkin, PanelEvent, panel_handle,
};
use gpui_kit::component::input::{Editor, EditorState, InputEvent, Position};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState, SliderValue};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use markloom_app::preview::{self, PageCache, Zoom};
use markloom_app::search::{self, SearchHit};
use markloom_app::settings::{self, AppTheme, Settings};
use markloom_app::style_session::StyleSession;
use markloom_app::workspace::{self, SystemTrash, Workspace};
use markloom_core::parse;
use markloom_docx::export_docx;
use markloom_mermaid::DiagramService;
use markloom_style::{PresetRegistry, StyleSheet};
use markloom_typeset::{Compiled, Diag, Engine, EngineOptions, PdfMeta, Severity};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

actions!(
    markloom,
    [
        SaveFile,
        QuickOpen,
        FindInFiles,
        CycleView,
        ExportPdf,
        ExportDocx,
        ToggleStyle
    ]
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Split,
    Editor,
    Preview,
}

struct Session {
    workspace: Workspace,
    settings: Settings,
    style: StyleSession,
    last_style: StyleSheet,
    registry: PresetRegistry,
    engine: Arc<Mutex<Engine>>,
    compiled: Option<Compiled>,
    compiled_pages: usize,
    source_lines: Vec<(u32, usize, f32)>,
    diagnostics: Vec<Diag>,
    images: Vec<Option<Arc<Image>>>,
    cache: PageCache,
    scroll: ScrollHandle,
    zoom: Zoom,
    viewport: (f32, f32),
    hidpi: f32,
    visible: (usize, usize),
    last_scale: f32,
    page_px: (f32, f32),
    view: ViewMode,
    edited: u64,
    compiled_gen: u64,
    token: u64,
    pending_sync: bool,
    status: SharedString,
    fonts: Vec<String>,
    quick_open: bool,
    find_open: bool,
    query: String,
    watch_rx: Option<mpsc::Receiver<notify::Event>>,
    _watcher: Option<RecommendedWatcher>,
    editor: Entity<EditorState>,
}

impl Session {
    fn new(
        folder: PathBuf,
        editor: Entity<EditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace = match Workspace::open(&folder) {
            Ok(workspace) => workspace,
            Err(_) => {
                let _ = std::fs::create_dir_all(&folder);
                Workspace::open(&folder).unwrap_or_else(|error| {
                    panic!("open {}: {error}", folder.display());
                })
            }
        };
        let mut settings = settings::load(&settings::config_dir().join("settings.toml"));
        settings::remember(&mut settings, workspace.root());
        let _ = settings::save(&settings::config_dir().join("settings.toml"), &settings);
        let mut registry = PresetRegistry::builtin();
        let _errors = registry.load_user_dir(&settings::config_dir().join("presets"));
        let style = StyleSession::new(settings.default_preset.clone());
        let last_style = style.resolve(&registry).unwrap_or_default();
        let engine = Arc::new(Mutex::new(Engine::new(
            EngineOptions { system_fonts: true },
            Arc::new(DiagramService::with_defaults()),
        )));
        let fonts = engine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .font_families();
        let (watch_rx, watcher) = watch(workspace.root());
        let mut session = Self {
            workspace,
            settings,
            style,
            last_style,
            registry,
            engine,
            compiled: None,
            compiled_pages: 0,
            source_lines: Vec::new(),
            diagnostics: Vec::new(),
            images: Vec::new(),
            cache: PageCache::default(),
            scroll: ScrollHandle::new(),
            zoom: Zoom::Percent(100.0),
            viewport: (640.0, 800.0),
            hidpi: 1.0,
            visible: (0, 0),
            last_scale: 1.0,
            page_px: (595.0, 842.0),
            view: ViewMode::Split,
            edited: 0,
            compiled_gen: 0,
            token: 0,
            pending_sync: false,
            status: "Ready".into(),
            fonts,
            quick_open: false,
            find_open: false,
            query: String::new(),
            watch_rx,
            _watcher: watcher,
            editor,
        };
        if let Some(path) = session.workspace.files().first().cloned() {
            let _ = session.workspace.open_file(&path);
        }
        session.sync_editor(window, cx);
        session.schedule(cx);
        session.pump(cx);
        session
    }

    fn resolved_style(&mut self) -> StyleSheet {
        match self.style.resolve(&self.registry) {
            Ok(sheet) => {
                self.last_style = sheet.clone();
                sheet
            }
            Err(error) => {
                self.status = error.to_string().into();
                self.last_style.clone()
            }
        }
    }

    fn sync_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_sync = false;
        let text = self
            .workspace
            .active()
            .map(|buffer| buffer.text.clone())
            .unwrap_or_default();
        self.editor.update(cx, |editor, cx| {
            editor.set_value(text, window, cx);
        });
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        self.edited = self.edited.saturating_add(1);
        self.token = self.token.saturating_add(1);
        let token = self.token;
        let generation = self.edited;
        let engine = Arc::clone(&self.engine);
        let markdown = self
            .workspace
            .active()
            .map(|buffer| buffer.text.clone())
            .unwrap_or_default();
        let style = self.resolved_style();
        let base = self.workspace.root().to_path_buf();
        let compile_markdown = markdown.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(preview::DEBOUNCE).await;
            let still = this
                .update(cx, |this, _| this.token == token)
                .unwrap_or(false);
            if !still {
                return;
            }
            let outcome = cx
                .background_spawn(async move {
                    let doc = parse(&compile_markdown);
                    let mut engine = engine
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    engine.compile(&doc, &style, &base)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.token != token {
                    return;
                }
                match outcome {
                    Ok(compiled) => this.finish_compile(generation, &markdown, compiled, cx),
                    Err(failure) => {
                        this.diagnostics = failure.diagnostics.clone();
                        this.status = "Compile error".into();
                        this.push_diagnostics(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn finish_compile(
        &mut self,
        generation: u64,
        markdown: &str,
        compiled: Compiled,
        cx: &mut Context<Self>,
    ) {
        if self.edited != generation {
            return;
        }
        self.compiled_pages = compiled.page_count();
        self.source_lines = (1..=markdown.lines().count().max(1) as u32)
            .filter_map(|line| {
                compiled
                    .source_map
                    .locate_line(line)
                    .map(|location| (line, location.page, location.y_pt))
            })
            .collect();
        self.diagnostics = compiled.warnings.clone();
        self.compiled_gen = generation;
        self.status = format!("{} pages", compiled.page_count()).into();
        self.paint_pages(&compiled, cx);
        self.compiled = Some(compiled);
        self.push_diagnostics(cx);
    }

    fn paint_pages(&mut self, compiled: &Compiled, _cx: &mut Context<Self>) {
        let count = compiled.page_count();
        let (page_w, page_h) = compiled.page_size_pt(0);
        let scale = preview::pixels_per_pt(
            self.zoom,
            self.viewport.0,
            self.viewport.1,
            page_w,
            page_h,
            self.hidpi,
        );
        self.last_scale = scale;
        if page_w > 0.0 && page_h > 0.0 {
            self.page_px = (page_w * scale, page_h * scale);
        }
        let bits = preview::scale_bits(scale);
        let (first, last) = self.visible;
        let range = preview::pages_to_rasterize(first, last, count);
        self.cache.retain_pages(|index| range.contains(&index));
        self.images.resize(count, None);
        for index in range {
            let hash = compiled.page_hash(index);
            let bytes = if let Some(hit) = self.cache.get(index, hash, bits) {
                hit.to_vec()
            } else {
                let bitmap = compiled.render_page(index, scale);
                match encode_png(&bitmap) {
                    Ok(bytes) => {
                        self.cache.insert(index, hash, bits, bytes.clone());
                        bytes
                    }
                    Err(_) => continue,
                }
            };
            self.images[index] = Some(Arc::new(Image::from_bytes(ImageFormat::Png, bytes)));
        }
    }

    fn note_view(
        &mut self,
        viewport: (f32, f32),
        hidpi: f32,
        first: usize,
        last: usize,
        cx: &mut Context<Self>,
    ) {
        let changed = self.viewport != viewport
            || (self.hidpi - hidpi).abs() > f32::EPSILON
            || self.visible != (first, last);
        self.viewport = viewport;
        self.hidpi = hidpi;
        self.visible = (first, last);
        if !changed {
            return;
        }
        if let Some(compiled) = self.compiled.clone() {
            self.paint_pages(&compiled, cx);
            cx.notify();
        }
    }

    fn set_zoom(&mut self, zoom: Zoom, cx: &mut Context<Self>) {
        self.zoom = zoom;
        if let Some(compiled) = self.compiled.clone() {
            self.paint_pages(&compiled, cx);
        }
        cx.notify();
    }

    fn push_diagnostics(&self, cx: &mut Context<Self>) {
        let diagnostics = self.diagnostics.clone();
        self.editor.update(cx, |editor, cx| {
            let text = editor.text().clone();
            let Some(set) = editor.diagnostics_mut() else {
                return;
            };
            set.reset(&text);
            for diagnostic in diagnostics {
                let line = diagnostic.md_line.unwrap_or(1).saturating_sub(1);
                let start = Position { line, character: 0 };
                let end = Position { line, character: 1 };
                let severity = match diagnostic.severity {
                    Severity::Error => DiagnosticSeverity::Error,
                    Severity::Warning => DiagnosticSeverity::Warning,
                };
                set.push(Diagnostic::new(start..end, diagnostic.message).with_severity(severity));
            }
            cx.notify();
        });
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        match self.workspace.save_active() {
            Ok(()) => {
                self.status = "Saved".into();
                let _ = settings::save(
                    &settings::config_dir().join("settings.toml"),
                    &self.settings,
                );
            }
            Err(message) => self.status = message.into(),
        }
        cx.notify();
    }

    /// Write the PDF of the current buffer. A stale preview is compiled first.
    fn export_pdf(&mut self, cx: &mut Context<Self>) {
        let Some(buffer) = self.workspace.active() else {
            self.status = "No file".into();
            cx.notify();
            return;
        };
        let generation = self.edited;
        let markdown = buffer.text.clone();
        let dest = self
            .workspace
            .root()
            .join(buffer.relative.with_extension("pdf"));
        let style = self.resolved_style();
        let base = self.workspace.root().to_path_buf();
        let doc = parse(&markdown);
        let title = doc.front_matter.title.clone();
        let author = doc.front_matter.author.clone();
        let compiled = {
            let mut engine = self
                .engine
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            engine.compile(&doc, &style, &base)
        };
        match compiled {
            Ok(compiled) => match compiled.write_pdf(&dest, &PdfMeta { title, author }) {
                Ok(()) => {
                    self.status = format!("PDF {}", dest.display()).into();
                    self.finish_compile(generation, &markdown, compiled, cx);
                }
                Err(error) => self.status = error.to_string().into(),
            },
            Err(error) => self.status = error.to_string().into(),
        }
        cx.notify();
    }

    fn export_docx(&mut self, cx: &mut Context<Self>) {
        let Some(buffer) = self.workspace.active() else {
            self.status = "No file".into();
            cx.notify();
            return;
        };
        let dest = self
            .workspace
            .root()
            .join(buffer.relative.with_extension("docx"));
        let doc = parse(&buffer.text);
        let style = self.resolved_style();
        let diagrams = DiagramService::with_defaults();
        match export_docx(&doc, &style, self.workspace.root(), &diagrams, &dest) {
            Ok(()) => self.status = format!("DOCX {}", dest.display()).into(),
            Err(error) => self.status = error.to_string().into(),
        }
        cx.notify();
    }

    fn open_relative(&mut self, relative: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match self.workspace.open_file(relative) {
            Ok(()) => {
                self.sync_editor(window, cx);
                self.schedule(cx);
                self.quick_open = false;
                self.find_open = false;
            }
            Err(message) => self.status = message.into(),
        }
        cx.notify();
    }

    fn create_markdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let relative = fresh_name(&self.workspace);
        match self.workspace.create_file(&relative) {
            Ok(()) => {
                self.sync_editor(window, cx);
                self.schedule(cx);
                self.status = format!("Created {}", relative.display()).into();
            }
            Err(message) => self.status = message.into(),
        }
        cx.notify();
    }

    fn delete_relative(&mut self, relative: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match self.workspace.delete(relative, &SystemTrash) {
            Ok(()) => {
                self.sync_editor(window, cx);
                self.schedule(cx);
                self.status = format!("Trashed {}", relative.display()).into();
            }
            Err(message) => self.status = message.into(),
        }
        cx.notify();
    }

    fn jump_to_line(&mut self, line: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.set_cursor_position(
                Position {
                    line: line.saturating_sub(1),
                    character: 0,
                },
                window,
                cx,
            );
        });
        if let Some((_, page, _)) = self
            .source_lines
            .iter()
            .find(|(source, _, _)| *source == line)
        {
            self.scroll.scroll_to_item(*page);
            self.status = format!("page {}", page + 1).into();
        }
    }

    fn search_corpus(&self) -> Vec<(String, String)> {
        self.workspace
            .files()
            .iter()
            .map(|relative| {
                let text = self
                    .workspace
                    .buffers()
                    .iter()
                    .find(|buffer| buffer.relative == *relative)
                    .map(|buffer| buffer.text.clone())
                    .unwrap_or_else(|| {
                        std::fs::read_to_string(self.workspace.root().join(relative))
                            .unwrap_or_default()
                    });
                (relative.display().to_string(), text)
            })
            .collect()
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let _ = this.update(cx, |this, cx| this.drain_watch(cx));
            }
        })
        .detach();
    }

    fn drain_watch(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = &self.watch_rx else {
            return;
        };
        let events: Vec<_> = rx.try_iter().collect();
        for event in events {
            for path in event.paths {
                let hits_active = self
                    .workspace
                    .active()
                    .is_some_and(|buffer| path.ends_with(&buffer.relative));
                let Some(disk) = workspace::disk_event(event.kind, path) else {
                    continue;
                };
                if self.workspace.apply_disk(disk).is_err() {
                    continue;
                }
                if hits_active
                    && self
                        .workspace
                        .active()
                        .is_some_and(|buffer| buffer.conflict)
                {
                    self.status = "File changed on disk".into();
                } else if hits_active
                    && self
                        .workspace
                        .active()
                        .is_some_and(|buffer| !buffer.is_dirty() && !buffer.missing)
                {
                    self.pending_sync = true;
                }
            }
        }
        cx.notify();
    }
}

fn fresh_name(workspace: &Workspace) -> PathBuf {
    for index in 1..10_000 {
        let name = if index == 1 {
            "untitled.md".to_string()
        } else {
            format!("untitled-{index}.md")
        };
        let path = PathBuf::from(name);
        if !workspace.files().contains(&path) {
            return path;
        }
    }
    PathBuf::from("untitled-extra.md")
}

fn watch(
    root: &Path,
) -> (
    Option<mpsc::Receiver<notify::Event>>,
    Option<RecommendedWatcher>,
) {
    let (tx, rx) = mpsc::channel();
    let mut watcher =
        match notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            if let Ok(event) = result {
                let _ = tx.send(event);
            }
        }) {
            Ok(watcher) => watcher,
            Err(_) => return (None, None),
        };
    if watcher.watch(root, RecursiveMode::Recursive).is_err() {
        return (None, None);
    }
    (Some(rx), Some(watcher))
}

fn encode_png(bitmap: &markloom_typeset::PageBitmap) -> Result<Vec<u8>, String> {
    if bitmap.width == 0 || bitmap.height == 0 {
        return Err("empty page".into());
    }
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

fn hex_of(color: Hsla) -> String {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u32;
    format!(
        "#{:02X}{:02X}{:02X}",
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b)
    )
}

struct FilesPanel {
    session: Entity<Session>,
    focus: FocusHandle,
}

impl FilesPanel {
    fn new(session: Entity<Session>, cx: &mut Context<Self>) -> Self {
        Self {
            session,
            focus: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for FilesPanel {}

impl Focusable for FilesPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl gpui_kit::base::dock::Panel for FilesPanel {
    fn panel_name(&self) -> &'static str {
        "Files"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl dock::Panel for FilesPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Files"
    }
}

impl Render for FilesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session_ref = self.session.read(cx);
        let files = session_ref.workspace.files().to_vec();
        let active = session_ref
            .workspace
            .active()
            .map(|buffer| buffer.relative.clone());
        let session = self.session.clone();
        v_flex()
            .size_full()
            .p_2()
            .gap_1()
            .child({
                let session = session.clone();
                Button::new("new-file")
                    .label("New file")
                    .on_click(move |_event, window, cx| {
                        session.update(cx, |session, cx| session.create_markdown(window, cx));
                    })
            })
            .children(files.into_iter().map(|path| {
                let label = path.display().to_string();
                let selected = active.as_ref() == Some(&path);
                let session = session.clone();
                let open = path.clone();
                h_flex()
                    .gap_1()
                    .child({
                        let session = session.clone();
                        let open = open.clone();
                        Button::new(ElementId::Name(format!("file-{label}").into()))
                            .label(label.clone())
                            .selected(selected)
                            .on_click(move |_event, window, cx| {
                                let open = open.clone();
                                session.update(cx, |session, cx| {
                                    session.open_relative(&open, window, cx)
                                });
                            })
                    })
                    .child({
                        let session = session.clone();
                        Button::new(ElementId::Name(format!("trash-{label}").into()))
                            .label("Trash")
                            .on_click(move |_event, window, cx| {
                                let open = open.clone();
                                session.update(cx, |session, cx| {
                                    session.delete_relative(&open, window, cx);
                                });
                            })
                    })
            }))
    }
}

struct EditorPanel {
    session: Entity<Session>,
    focus: FocusHandle,
}

impl EditorPanel {
    fn new(session: Entity<Session>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = session.read(cx).editor.clone();
        cx.subscribe_in(
            &editor,
            window,
            |this, editor, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = editor.read(cx).value().to_string();
                    this.session.update(cx, |session, cx| {
                        session.workspace.set_text(text);
                        if settings::should_autosave(
                            &session.settings,
                            session
                                .workspace
                                .active()
                                .is_some_and(|buffer| buffer.is_dirty()),
                        ) {
                            session.save(cx);
                        }
                        session.schedule(cx);
                    });
                }
            },
        )
        .detach();
        cx.observe(&editor, |this, editor, cx| {
            let line = editor.read(cx).cursor_position().line;
            this.session.update(cx, |session, cx| {
                if let Some((_, page, _)) = session
                    .source_lines
                    .iter()
                    .find(|(source, _, _)| *source == line + 1)
                {
                    session.status = format!("page {}", page + 1).into();
                    session.scroll.scroll_to_item(*page);
                }
                cx.notify();
            });
        })
        .detach();
        Self {
            session,
            focus: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for EditorPanel {}

impl Focusable for EditorPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl gpui_kit::base::dock::Panel for EditorPanel {
    fn panel_name(&self) -> &'static str {
        "Editor"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn visible(&self, cx: &App) -> bool {
        !matches!(self.session.read(cx).view, ViewMode::Preview)
    }
}

impl dock::Panel for EditorPanel {}

impl Render for EditorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.session.read(cx).pending_sync {
            cx.defer_in(window, |this, window, cx| {
                this.session.update(cx, |session, cx| {
                    if session.pending_sync {
                        session.sync_editor(window, cx);
                        session.schedule(cx);
                    }
                });
            });
        }
        let editor = self.session.read(cx).editor.clone();
        let buffers = self.session.read(cx).workspace.buffers().to_vec();
        let active = self.session.read(cx).workspace.active_index();
        let session = self.session.clone();
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .gap_1()
                    .p_1()
                    .children(buffers.iter().enumerate().map(|(index, buffer)| {
                        let session = session.clone();
                        let label = buffer.relative.display().to_string();
                        Button::new(ElementId::Name(format!("tab-{label}").into()))
                            .label(label)
                            .selected(Some(index) == active)
                            .on_click(move |_event, window, cx| {
                                session.update(cx, |session, cx| {
                                    let _ = session.workspace.select(index);
                                    session.sync_editor(window, cx);
                                    session.schedule(cx);
                                });
                            })
                    })),
            )
            .child(Editor::new(&editor).h(relative(1.0)))
    }
}

struct PreviewPanel {
    session: Entity<Session>,
    focus: FocusHandle,
}

impl PreviewPanel {
    fn new(session: Entity<Session>, cx: &mut Context<Self>) -> Self {
        Self {
            session,
            focus: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for PreviewPanel {}

impl Focusable for PreviewPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl gpui_kit::base::dock::Panel for PreviewPanel {
    fn panel_name(&self) -> &'static str {
        "Preview"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }

    fn visible(&self, cx: &App) -> bool {
        !matches!(self.session.read(cx).view, ViewMode::Editor)
    }
}

impl dock::Panel for PreviewPanel {
    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.session.read(cx).status.clone()
    }
}

impl Render for PreviewPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let size = window.viewport_size();
        let viewport = (size.width.as_f32().round(), size.height.as_f32().round());
        let hidpi = window.scale_factor();
        let scroll = self.session.read(cx).scroll.clone();
        let first = scroll.top_item();
        let last = scroll.bottom_item();
        let unchanged = {
            let session = self.session.read(cx);
            session.viewport == viewport
                && (session.hidpi - hidpi).abs() <= f32::EPSILON
                && session.visible == (first, last)
        };
        if !unchanged {
            let session = self.session.clone();
            cx.defer_in(window, move |_this, _window, cx| {
                session.update(cx, |session, cx| {
                    session.note_view(viewport, hidpi, first, last, cx);
                });
            });
        }
        let images = self.session.read(cx).images.clone();
        let page_px = self.session.read(cx).page_px;
        let zoom = self.session.read(cx).zoom;
        let session = self.session.clone();
        v_flex()
            .size_full()
            .child(zoom_row(session.clone(), zoom))
            .child(
                div()
                    .id("preview-pages")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .p_4()
                    .children(images.into_iter().enumerate().map(|(index, image)| {
                        let session = session.clone();
                        let scroll = scroll.clone();
                        div()
                            .id(ElementId::Name(format!("page-{index}").into()))
                            .w(px(page_px.0.max(1.0)))
                            .h(px(page_px.1.max(1.0)))
                            .child(match image {
                                Some(image) => img(image).max_w_full().into_any_element(),
                                None => div().size_full().child("…").into_any_element(),
                            })
                            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                                let y = event.position.y.as_f32();
                                let top = scroll
                                    .bounds_for_item(index)
                                    .map(|bounds| bounds.origin.y.as_f32())
                                    .unwrap_or(y);
                                session.update(cx, |session, cx| {
                                    let scale = session.last_scale.max(0.25);
                                    let y_pt = ((y - top) / scale).max(0.0);
                                    if let Some((line, _, _)) =
                                        session.source_lines.iter().rev().find(
                                            |(_, page, point)| *page == index && *point <= y_pt,
                                        )
                                    {
                                        session.jump_to_line(*line, window, cx);
                                    }
                                });
                            })
                    })),
            )
    }
}

fn zoom_row(session: Entity<Session>, zoom: Zoom) -> impl IntoElement {
    let choice = |id: &'static str, label: &'static str, next: Zoom, current: Zoom| {
        let session = session.clone();
        Button::new(id)
            .label(label)
            .selected(current == next)
            .on_click(move |_event, _window, cx| {
                session.update(cx, |session, cx| session.set_zoom(next, cx));
            })
    };
    h_flex()
        .gap_1()
        .p_1()
        .child(choice("zoom-width", "Fit width", Zoom::FitWidth, zoom))
        .child(choice("zoom-page", "Fit page", Zoom::FitPage, zoom))
        .child(choice("zoom-100", "100%", Zoom::Percent(100.0), zoom))
}

struct StylePanel {
    session: Entity<Session>,
    focus: FocusHandle,
    size: Entity<SliderState>,
    scale: Entity<SliderState>,
    color: Entity<ColorPickerState>,
}

impl StylePanel {
    fn new(session: Entity<Session>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let size = cx.new(|_| {
            SliderState::new()
                .min(6.0)
                .max(32.0)
                .step(0.5)
                .default_value(11.0)
        });
        let scale = cx.new(|_| {
            SliderState::new()
                .min(0.25)
                .max(4.0)
                .step(0.05)
                .default_value(1.0)
        });
        let color = cx.new(|cx| ColorPickerState::new(window, cx));
        cx.subscribe_in(
            &size,
            window,
            |this, _slider, event: &SliderEvent, _window, cx| {
                if let SliderEvent::Change(SliderValue::Single(value)) = event {
                    this.session.update(cx, |session, cx| {
                        session.style.set_base_size_pt(*value);
                        session.schedule(cx);
                    });
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &scale,
            window,
            |this, _slider, event: &SliderEvent, _window, cx| {
                if let SliderEvent::Change(SliderValue::Single(value)) = event {
                    this.session.update(cx, |session, cx| {
                        session.style.set_diagram_scale(*value);
                        session.schedule(cx);
                    });
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &color,
            window,
            |this, _picker, event: &ColorPickerEvent, _window, cx| {
                if let ColorPickerEvent::Change(Some(color)) = event {
                    let hex = hex_of(*color);
                    this.session.update(cx, |session, cx| {
                        session.style.set_text_color(&hex);
                        session.schedule(cx);
                    });
                }
            },
        )
        .detach();
        Self {
            session,
            focus: cx.focus_handle(),
            size,
            scale,
            color,
        }
    }
}

impl EventEmitter<PanelEvent> for StylePanel {}

impl Focusable for StylePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl gpui_kit::base::dock::Panel for StylePanel {
    fn panel_name(&self) -> &'static str {
        "Style"
    }

    fn closable(&self, _: &App) -> bool {
        false
    }
}

impl dock::Panel for StylePanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Style"
    }
}

impl Render for StylePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let names = self.session.read(cx).registry.names();
        let current = self.session.read(cx).style.preset().to_string();
        let fonts = self.session.read(cx).fonts.clone();
        let session = self.session.clone();
        let preset_buttons = names.into_iter().map(|name| {
            let session = session.clone();
            let label = name.clone();
            Button::new(ElementId::Name(format!("preset-{name}").into()))
                .label(name.clone())
                .selected(name == current)
                .on_click(move |_event, _window, cx| {
                    let label = label.clone();
                    session.update(cx, |session, cx| {
                        session.style.set_preset(label);
                        session.schedule(cx);
                    });
                })
        });
        let font_buttons = fonts.into_iter().take(8).map(|font| {
            let session = session.clone();
            let label = font.clone();
            Button::new(ElementId::Name(format!("font-{font}").into()))
                .label(font)
                .on_click(move |_event, _window, cx| {
                    let label = label.clone();
                    session.update(cx, |session, cx| {
                        session.style.set_body_font(&label);
                        session.schedule(cx);
                    });
                })
        });
        h_flex()
            .size_full()
            .gap_2()
            .p_2()
            .child(v_flex().gap_1().children(preset_buttons))
            .child(
                v_flex()
                    .gap_1()
                    .child("Size")
                    .child(Slider::new(&self.size).w(px(160.)))
                    .child("Diagram scale")
                    .child(Slider::new(&self.scale).w(px(160.))),
            )
            .child(ColorPicker::new(&self.color))
            .child(v_flex().gap_1().children(font_buttons))
            .child(choice_row(
                &session,
                "borders",
                &["Grid", "Horizontal", "HeaderOnly", "None"],
            ))
            .child(choice_row(
                &session,
                "theme",
                &["Default", "Neutral", "Dark", "Forest"],
            ))
            .child({
                let session = session.clone();
                Button::new("reset-style")
                    .label("Reset")
                    .on_click(move |_event, _window, cx| {
                        session.update(cx, |session, cx| {
                            let preset = session.settings.default_preset.clone();
                            session.style.reset(&preset);
                            session.schedule(cx);
                        });
                    })
            })
            .child({
                let session = session.clone();
                Button::new("save-preset").label("Save as preset").on_click(
                    move |_event, _window, cx| {
                        session.update(cx, |session, cx| {
                            let sheet = session.resolved_style();
                            let name = format!("{}-custom", session.style.preset());
                            let dir = settings::config_dir().join("presets");
                            match session.style.save_preset(&dir, &name, &sheet) {
                                Ok(()) => {
                                    let _ = session.registry.load_user_dir(&dir);
                                    session.status = format!("Saved preset {name}").into();
                                }
                                Err(error) => session.status = error.into(),
                            }
                            cx.notify();
                        });
                    },
                )
            })
    }
}

fn choice_row(session: &Entity<Session>, kind: &str, names: &[&str]) -> impl IntoElement {
    let session = session.clone();
    let kind = kind.to_string();
    h_flex().gap_1().children(names.iter().map(|name| {
        let session = session.clone();
        let kind = kind.clone();
        let label = (*name).to_string();
        Button::new(ElementId::Name(format!("{kind}-{label}").into()))
            .label(label.clone())
            .on_click(move |_event, _window, cx| {
                let label = label.clone();
                let kind = kind.clone();
                session.update(cx, |session, cx| {
                    if kind == "borders" {
                        session.style.set_table_borders(&label);
                    } else {
                        session.style.set_diagram_theme(&label);
                    }
                    session.schedule(cx);
                });
            })
    }))
}

struct Shell {
    dock: Entity<DockArea>,
    session: Entity<Session>,
    query_editor: Entity<EditorState>,
}

impl Shell {
    fn new(folder: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx);
            editor.set_highlighter("markdown", cx);
            editor
        });
        let query_editor = cx.new(|cx| EditorState::new(window, cx));
        let session = cx.new(|cx| Session::new(folder, editor, window, cx));
        if session.read(cx).settings.theme == AppTheme::Dark {
            Theme::change(ThemeMode::Dark, Some(window), cx);
        }
        cx.subscribe_in(
            &query_editor,
            window,
            |this, editor, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = editor.read(cx).value().to_string();
                    this.session.update(cx, |session, cx| {
                        session.query = text.trim().to_string();
                        cx.notify();
                    });
                }
            },
        )
        .detach();
        let (dock, _skin) = DockSkin::dock_area("markloom", Some(1), window, cx);
        let files = cx.new(|cx| FilesPanel::new(session.clone(), cx));
        let editor_panel = cx.new(|cx| EditorPanel::new(session.clone(), window, cx));
        let preview = cx.new(|cx| PreviewPanel::new(session.clone(), cx));
        let style = cx.new(|cx| StylePanel::new(session.clone(), window, cx));
        dock.update(cx, |dock, cx| {
            dock.add_panel_view(
                panel_handle(files),
                DockPlacement::Left,
                Some(px(240.)),
                window,
                cx,
            );
            dock.add_panel_view(
                panel_handle(editor_panel),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
            dock.add_panel_view(
                panel_handle(preview),
                DockPlacement::Right,
                Some(px(420.)),
                window,
                cx,
            );
            dock.add_panel_view(
                panel_handle(style),
                DockPlacement::Bottom,
                Some(px(180.)),
                window,
                cx,
            );
        });
        Self {
            dock,
            session,
            query_editor,
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let session = self.session.clone();
        let snapshot = self.session.read(cx);
        let status = snapshot.status.clone();
        let quick_open = snapshot.quick_open;
        let find_open = snapshot.find_open;
        let query = snapshot.query.clone();
        let paths: Vec<String> = snapshot
            .workspace
            .files()
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        let quick_hits = search::quick_open(&query, &paths)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let find_hits = if find_open {
            let mut hits = search::search_files(&query, &snapshot.search_corpus());
            hits.truncate(40);
            hits
        } else {
            Vec::new()
        };
        let query_editor = self.query_editor.clone();
        v_flex()
            .size_full()
            .child(toolbar(session.clone(), status))
            .children(palette(
                quick_open,
                find_open,
                session.clone(),
                query_editor,
                quick_hits,
                find_hits,
            ))
            .child(div().flex_1().child(self.dock.clone()))
            .on_action(cx.listener(|this, _: &SaveFile, _window, cx| {
                this.session.update(cx, |session, cx| session.save(cx));
            }))
            .on_action(cx.listener(|this, _: &CycleView, _window, cx| {
                this.session.update(cx, |session, cx| {
                    session.view = match session.view {
                        ViewMode::Split => ViewMode::Editor,
                        ViewMode::Editor => ViewMode::Preview,
                        ViewMode::Preview => ViewMode::Split,
                    };
                    cx.notify();
                });
            }))
            .on_action(cx.listener(|this, _: &ExportPdf, _window, cx| {
                this.session
                    .update(cx, |session, cx| session.export_pdf(cx));
            }))
            .on_action(cx.listener(|this, _: &ExportDocx, _window, cx| {
                this.session
                    .update(cx, |session, cx| session.export_docx(cx));
            }))
            .on_action(cx.listener(|this, _: &QuickOpen, window, cx| {
                this.session.update(cx, |session, cx| {
                    session.quick_open = !session.quick_open;
                    session.find_open = false;
                    cx.notify();
                });
                this.query_editor.update(cx, |editor, cx| {
                    editor.focus(window, cx);
                });
            }))
            .on_action(cx.listener(|this, _: &FindInFiles, window, cx| {
                this.session.update(cx, |session, cx| {
                    session.find_open = !session.find_open;
                    session.quick_open = false;
                    cx.notify();
                });
                this.query_editor.update(cx, |editor, cx| {
                    editor.focus(window, cx);
                });
            }))
            .on_action(cx.listener(|this, _: &ToggleStyle, window, cx| {
                this.dock.update(cx, |dock, cx| {
                    dock.toggle_dock(DockPlacement::Bottom, window, cx);
                });
            }))
    }
}

fn toolbar(session: Entity<Session>, status: SharedString) -> impl IntoElement {
    h_flex()
        .gap_1()
        .p_1()
        .child(status)
        .child(command("view", "View", session.clone(), |session, cx| {
            session.view = match session.view {
                ViewMode::Split => ViewMode::Editor,
                ViewMode::Editor => ViewMode::Preview,
                ViewMode::Preview => ViewMode::Split,
            };
            cx.notify();
        }))
        .child(command(
            "pdf",
            "Export PDF",
            session.clone(),
            |session, cx| session.export_pdf(cx),
        ))
        .child(command(
            "docx",
            "Export DOCX",
            session.clone(),
            |session, cx| session.export_docx(cx),
        ))
        .child(command("theme", "Theme", session, |session, cx| {
            session.settings.theme = match session.settings.theme {
                AppTheme::Light => AppTheme::Dark,
                AppTheme::Dark => AppTheme::Light,
            };
            let mode = match session.settings.theme {
                AppTheme::Light => ThemeMode::Light,
                AppTheme::Dark => ThemeMode::Dark,
            };
            Theme::change(mode, None, cx);
            let _ = settings::save(
                &settings::config_dir().join("settings.toml"),
                &session.settings,
            );
        }))
}

fn command(
    id: &str,
    label: &str,
    session: Entity<Session>,
    apply: impl Fn(&mut Session, &mut Context<Session>) + 'static,
) -> impl IntoElement {
    let apply = Arc::new(apply);
    Button::new(ElementId::Name(id.to_string().into()))
        .label(label.to_string())
        .on_click(move |_event, _window, cx| {
            let apply = Arc::clone(&apply);
            session.update(cx, |session, cx| apply(session, cx));
        })
}

fn palette(
    quick_open: bool,
    find_open: bool,
    session: Entity<Session>,
    query_editor: Entity<EditorState>,
    quick_hits: Vec<String>,
    find_hits: Vec<SearchHit>,
) -> Option<impl IntoElement> {
    if !quick_open && !find_open {
        return None;
    }
    let heading = if quick_open {
        "Quick open"
    } else {
        "Find in files"
    };
    Some(
        v_flex()
            .p_2()
            .gap_1()
            .border_b_1()
            .child(heading)
            .child(Editor::new(&query_editor).h(px(72.)))
            .children(if quick_open {
                quick_hits
                    .into_iter()
                    .map(|path| {
                        let session = session.clone();
                        let open = PathBuf::from(&path);
                        Button::new(ElementId::Name(format!("quick-{path}").into()))
                            .label(path)
                            .on_click(move |_event, window, cx| {
                                let open = open.clone();
                                session.update(cx, |session, cx| {
                                    session.open_relative(&open, window, cx);
                                });
                            })
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            } else {
                find_hits
                    .into_iter()
                    .map(|hit| {
                        let session = session.clone();
                        let open = PathBuf::from(&hit.path);
                        let line = hit.line;
                        let label = format!("{}:{} {}", hit.path, hit.line, hit.text);
                        Button::new(ElementId::Name(format!("find-{}-{line}", hit.path).into()))
                            .label(label)
                            .on_click(move |_event, window, cx| {
                                let open = open.clone();
                                session.update(cx, |session, cx| {
                                    session.open_relative(&open, window, cx);
                                    session.jump_to_line(line, window, cx);
                                });
                            })
                            .into_any_element()
                    })
                    .collect::<Vec<_>>()
            }),
    )
}

pub fn run() {
    let folder = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.bind_keys([
                KeyBinding::new("ctrl-s", SaveFile, None),
                KeyBinding::new("ctrl-p", QuickOpen, None),
                KeyBinding::new("ctrl-shift-f", FindInFiles, None),
                KeyBinding::new("ctrl-\\", CycleView, None),
                KeyBinding::new("ctrl-e", ExportPdf, None),
                KeyBinding::new("ctrl-shift-d", ExportDocx, None),
                KeyBinding::new("ctrl-alt-s", ToggleStyle, None),
            ]);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Shell::new(folder, window, cx))
            })
            .expect("open window");
        });
}
