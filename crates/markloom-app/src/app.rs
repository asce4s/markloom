//! The application window: bars, columns, and the unsaved-changes prompt.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{
    ActiveTheme as _, Theme, ThemeMode, WindowExt as _, h_resizable, resizable_panel, v_flex,
};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, Focusable as _,
    InteractiveElement as _, IntoElement, ParentElement, PathPromptOptions, Pixels, Render, Styled,
    Subscription, Task, UniformListScrollHandle, Window, WindowOptions, div, px,
};
use markloom_style::StyleSheet;
use markloom_typeset::{Engine, PdfMeta};

use crate::jobs;
use crate::search::SearchHit;

use crate::document::{Document, DocumentEvent};
use crate::preview_state::{PreviewState, PreviewStatus};
use crate::probe;
use crate::settings::{self, AppTheme, Settings};
use crate::style_state::StyleState;
use crate::views::{self, files::FilesColumn, settings::SettingsColumn};

gpui_kit::actions!(
    markloom,
    [
        ToggleFiles,
        ToggleSettings,
        QuickOpen,
        FindInFiles,
        SaveFile,
        ExportPdf,
        ExportDocx,
        ZoomIn,
        ZoomOut,
        OpenPath,
    ]
);

enum Pending {
    Open,
    Close,
}

pub struct AppRoot {
    document: Entity<Document>,
    style: Entity<StyleState>,
    preview: Entity<PreviewState>,
    files: Entity<FilesColumn>,
    settings_column: Entity<SettingsColumn>,
    preview_scroll: UniformListScrollHandle,
    settings: Settings,
    settings_path: PathBuf,
    pending: Option<Pending>,
    pending_path: Option<PathBuf>,
    allow_close: bool,
    preset_error: Option<String>,
    search_cache: Option<SearchCache>,
    search_gen: u64,
    search_runs: u64,
    _search_input: Option<Subscription>,
    _document: Subscription,
    _preview: Subscription,
    _style: Subscription,
}

struct SearchCache {
    root: PathBuf,
    query: String,
    stamp: u64,
    hits: Vec<SearchHit>,
}

impl AppRoot {
    pub fn open(
        path: &Path,
        settings_path: &Path,
        engine: Option<Engine>,
        load_fonts: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = settings::load(settings_path);
        let mode = match settings.theme {
            AppTheme::Dark => ThemeMode::Dark,
            AppTheme::Light => ThemeMode::Light,
        };
        Theme::change(mode, Some(window), cx);
        let document = cx.new(|cx| {
            Document::open(path, window, cx)
                .or_else(|_| Document::open(Path::new("."), window, cx))
                .expect("open a folder")
        });
        let style = cx.new(|cx| StyleState::new(settings.default_preset.clone(), cx));
        let sync = settings.sync_scroll;
        let preview = match engine {
            Some(engine) => cx.new(|cx| {
                let mut preview =
                    PreviewState::with_engine(engine, document.clone(), style.clone(), cx);
                preview.set_sync(sync);
                preview
            }),
            None => cx.new(|cx| {
                let mut preview = PreviewState::new(document.clone(), style.clone(), cx);
                preview.set_sync(sync);
                preview
            }),
        };
        if load_fonts && matches!(preview.read(cx).status(), PreviewStatus::LoadingFonts) {
            spawn_engine(preview.clone(), cx);
        }
        let files = cx.new(|cx| FilesColumn::new(document.clone(), cx));
        let settings_column =
            cx.new(|cx| SettingsColumn::new(style.clone(), preview.clone(), window, cx));
        let document_sub = cx.subscribe(&document, |this, _, event: &DocumentEvent, cx| {
            if let DocumentEvent::CursorLineChanged(line) = event
                && this.preview.read(cx).sync_scroll()
                && let Some(page) = this.preview.read(cx).scroll_target(*line)
            {
                this.preview_scroll
                    .scroll_to_item(page, gpui_kit::ScrollStrategy::Nearest);
            }
            cx.notify();
        });
        let preview_sub = cx.observe(&preview, |_, _, cx| cx.notify());
        let style_sub = cx.observe(&style, |_, _, cx| cx.notify());
        let this = Self {
            document,
            style,
            preview,
            files,
            settings_column,
            preview_scroll: UniformListScrollHandle::new(),
            settings,
            settings_path: settings_path.to_path_buf(),
            pending: None,
            pending_path: None,
            allow_close: false,
            preset_error: None,
            search_cache: None,
            search_gen: 0,
            search_runs: 0,
            _search_input: None,
            _document: document_sub,
            _preview: preview_sub,
            _style: style_sub,
        };
        let editor = this.document.read(cx).editor().clone();
        editor.read(cx).focus_handle(cx).focus(window, cx);
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| this.should_close(window, cx))
                .unwrap_or(true)
        });
        this
    }

    pub fn document(&self) -> &Entity<Document> {
        &self.document
    }

    pub fn style(&self) -> &Entity<StyleState> {
        &self.style
    }

    pub fn preview(&self) -> &Entity<PreviewState> {
        &self.preview
    }

    pub fn preview_scroll(&self) -> &UniformListScrollHandle {
        &self.preview_scroll
    }

    pub fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.theme = match self.settings.theme {
            AppTheme::Dark => AppTheme::Light,
            AppTheme::Light => AppTheme::Dark,
        };
        let mode = match self.settings.theme {
            AppTheme::Dark => ThemeMode::Dark,
            AppTheme::Light => ThemeMode::Light,
        };
        Theme::change(mode, Some(window), cx);
        self.persist();
        cx.notify();
    }

    pub fn toggle_sync(&mut self, cx: &mut Context<Self>) {
        self.settings.sync_scroll = !self.settings.sync_scroll;
        let sync = self.settings.sync_scroll;
        self.preview.update(cx, |preview, _| preview.set_sync(sync));
        self.persist();
        cx.notify();
    }

    /// Ask for a preset name, then write it beside the settings file.
    pub fn save_style_as_preset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preset_error = None;
        let name = cx.new(|cx| {
            gpui_kit::component::input::InputState::new(window, cx).placeholder("Preset name")
        });
        let app = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let field = name.clone();
            let confirm = app.clone();
            let named = name.clone();
            let message = app
                .upgrade()
                .and_then(|root| root.read(cx).preset_error.clone());
            dialog
                .title("Save style as preset")
                .child(preset_name(field))
                .child(preset_error(message, cx))
                .footer(DialogFooter::new().child(preset_confirm(confirm, named)))
        });
    }

    fn commit_preset(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = name.trim();
        if BUILTIN_PRESETS.contains(&name) {
            self.preset_error = Some("Choose a name that isn't a built-in preset".into());
            cx.notify();
            return;
        }
        if name.is_empty() || name.contains(['/', '\\']) {
            self.preset_error = Some("preset name is empty".into());
            cx.notify();
            return;
        }
        let dir = self
            .settings_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("presets");
        let current = self.style.read(cx).preset().to_string();
        if let Err(error) = self.style.read(cx).save_preset(&dir, name) {
            self.preset_error = Some(error);
            cx.notify();
            return;
        }
        let path = dir.join(format!("{name}.toml"));
        if let Ok(body) = std::fs::read_to_string(&path)
            && !body.contains("extends")
        {
            let _ = std::fs::write(&path, format!("extends = \"{current}\"\n{body}"));
        }
        self.style.update(cx, |style, cx| {
            style.load_user_presets(&dir);
            style.set_preset(name, cx);
        });
        self.preset_error = None;
        window.close_dialog(cx);
    }

    pub fn new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = fresh_name(self.document.read(cx).workspace().files());
        let path = PathBuf::from(name);
        self.document.update(cx, |document, cx| {
            let _ = document.create_file(&path, window, cx);
        });
        self.files.update(cx, |_, cx| cx.notify());
    }

    fn persist(&self) {
        let _ = settings::save(&self.settings_path, &self.settings);
    }

    fn toggle_files(&mut self, cx: &mut Context<Self>) {
        self.settings.files_open = !self.settings.files_open;
        self.persist();
        cx.notify();
    }

    fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings.settings_open = !self.settings.settings_open;
        self.persist();
        cx.notify();
    }

    fn begin_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let unsaved = self.document.read(cx).workspace().has_unsaved();
        if unsaved.is_empty() {
            self.pick_folder(cx);
            return;
        }
        self.pending = Some(Pending::Open);
        self.ask_unsaved(unsaved.len(), window, cx);
    }

    fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.allow_close {
            return true;
        }
        let unsaved = self.document.read(cx).workspace().has_unsaved();
        if unsaved.is_empty() {
            return true;
        }
        self.pending = Some(Pending::Close);
        self.ask_unsaved(unsaved.len(), window, cx);
        false
    }

    fn ask_unsaved(&mut self, count: usize, window: &mut Window, cx: &mut Context<Self>) {
        let title = format!("Save changes to {count} file(s)?");
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let save = app.clone();
            let discard = app.clone();
            let cancel = app.clone();
            alert
                .title(title.clone())
                .footer(
                    DialogFooter::new()
                        .child(prompt_button("save-changes", "Save", move |window, cx| {
                            save.update(cx, |app, cx| app.finish_pending(true, true, window, cx))
                                .ok();
                        }))
                        .child(prompt_button(
                            "discard-changes",
                            "Discard",
                            move |window, cx| {
                                discard
                                    .update(cx, |app, cx| {
                                        app.finish_pending(true, false, window, cx)
                                    })
                                    .ok();
                            },
                        ))
                        .child(prompt_button(
                            "cancel-changes",
                            "Cancel",
                            move |window, cx| {
                                cancel
                                    .update(cx, |app, cx| {
                                        app.finish_pending(false, false, window, cx)
                                    })
                                    .ok();
                            },
                        )),
                )
                .description(prompt_copy(title.clone()))
        });
    }

    fn finish_pending(
        &mut self,
        proceed: bool,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.close_dialog(cx);
        let pending = self.pending.take();
        if !proceed {
            cx.notify();
            return;
        }
        if save {
            self.document.update(cx, |document, cx| {
                let _ = document.save_dirty(cx);
            });
        }
        match pending {
            Some(Pending::Open) => self.pick_folder(cx),
            Some(Pending::Close) => {
                self.allow_close = true;
                window.remove_window();
            }
            None => {}
        }
    }

    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| {
                this.pending_path = Some(path);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let document = self.document.clone();
        let ranked = ranked_files(document.read(cx).workspace().files());
        window.open_dialog(cx, move |dialog, _, _| {
            let mut dialog = dialog.title("Quick open");
            for file in ranked.clone() {
                let document = document.clone();
                let label = file.display().to_string();
                dialog = dialog.child(
                    Button::new(gpui_kit::ElementId::Name(label.clone().into()))
                        .label(label)
                        .ghost()
                        .on_click(move |_, window, cx| {
                            let path = file.clone();
                            document.update(cx, |document, cx| {
                                let _ = document.open_file(&path, window, cx);
                            });
                            window.close_dialog(cx);
                        }),
                );
            }
            dialog
        });
    }

    fn find_in_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.entity().clone();
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find in files"));
        self._search_input = Some(cx.subscribe(&query, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = input.read(cx).value().to_string();
                this.find(&text, cx);
            }
        }));
        window.open_dialog(cx, move |dialog, _, cx| {
            let mut dialog = dialog
                .title("Find in files")
                .child("Results appear after you stop typing.")
                .child(Input::new(&query));
            for hit in app.read(cx).search_hits() {
                dialog = dialog.child(format!("{}:{} {}", hit.path, hit.line, hit.text));
            }
            dialog
        });
    }

    /// How many search jobs have finished. A repeated query does not add one.
    pub fn search_runs(&self) -> u64 {
        self.search_runs
    }

    /// Hits from the last finished query. Empty until that job applies.
    pub fn search_hits(&self) -> &[SearchHit] {
        self.search_cache
            .as_ref()
            .map(|cache| cache.hits.as_slice())
            .unwrap_or(&[])
    }

    /// Debounced file search. The same query and workspace snapshot does not run again.
    pub fn find(&mut self, query: &str, cx: &mut Context<Self>) {
        let query = query.to_string();
        let workspace = self.document.read(cx).workspace();
        let root = workspace.root().to_path_buf();
        let stamp = workspace_stamp(workspace);
        if self
            .search_cache
            .as_ref()
            .is_some_and(|cache| cache.root == root && cache.query == query && cache.stamp == stamp)
        {
            return;
        }
        self.search_gen = self.search_gen.wrapping_add(1);
        let generation = self.search_gen;
        let open: Vec<(String, String)> = workspace
            .buffers()
            .iter()
            .map(|buffer| (buffer.relative.display().to_string(), buffer.text.clone()))
            .collect();
        let open_paths: HashSet<PathBuf> = workspace
            .buffers()
            .iter()
            .map(|buffer| buffer.relative.clone())
            .collect();
        let extra: Vec<PathBuf> = workspace
            .files()
            .iter()
            .filter(|path| !open_paths.contains(*path))
            .cloned()
            .collect();
        let task = jobs::search(query.clone(), open, root.clone(), extra, cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let hits = task.await;
            this.update(cx, |this, cx| {
                if this.search_gen != generation {
                    return;
                }
                this.search_runs = this.search_runs.wrapping_add(1);
                this.search_cache = Some(SearchCache {
                    root,
                    query,
                    stamp,
                    hits,
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn export_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dest, markdown, base)) = export_target(self, "pdf", cx) else {
            window.push_notification("Nothing to export", cx);
            return;
        };
        let handle = window.window_handle();
        let task = if matches!(self.preview.read(cx).status(), PreviewStatus::Ready { .. }) {
            let Some(compiled) = self.preview.read(cx).compiled().cloned() else {
                window.push_notification("Nothing to export", cx);
                return;
            };
            jobs::export_pdf(compiled, dest, PdfMeta::default(), cx)
        } else if let Some(engine) = self.preview.read(cx).engine_handle() {
            let style = StyleSheet::clone(self.style.read(cx).sheet());
            jobs::export_fresh_pdf(engine, markdown, style, base, dest, PdfMeta::default(), cx)
        } else {
            window.push_notification("Fonts are still loading", cx);
            return;
        };
        finish_export(handle, task, cx);
    }

    fn export_docx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((dest, markdown, base)) = export_target(self, "docx", cx) else {
            window.push_notification("Nothing to export", cx);
            return;
        };
        let style = StyleSheet::clone(self.style.read(cx).sheet());
        let handle = window.window_handle();
        let task = jobs::export_docx(markdown, style, base, dest, cx);
        finish_export(handle, task, cx);
    }

    fn remember_widths(&mut self, sizes: &[Pixels]) {
        let next = map_widths(
            self.settings.column_widths,
            self.settings.files_open,
            self.settings.settings_open,
            sizes,
        );
        if widths_changed(self.settings.column_widths, next) {
            self.settings.column_widths = Some(next);
            self.persist();
        }
    }

    fn apply_pending_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.pending_path.take() else {
            return;
        };
        let document = self.document.clone();
        let files = self.files.clone();
        window.defer(cx, move |window, cx| {
            document.update(cx, |document, cx| {
                let _ = document.open_path(&path, window, cx);
            });
            files.update(cx, |_, cx| cx.notify());
        });
    }

    fn editor_width(&self, window: &Window) -> Pixels {
        if let Some(widths) = self.settings.column_widths {
            return px(widths[1].max(160.0));
        }
        let total = window.bounds().size.width.as_f32();
        let mut side = 0.0;
        if self.settings.files_open {
            side += 220.0;
        }
        if self.settings.settings_open {
            side += 300.0;
        }
        px(((total - side) * 0.32).max(160.0))
    }
}

impl Render for AppRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        probe::clear();
        self.apply_pending_path(window, cx);
        let document = self.document.clone();
        window.defer(cx, {
            let document = document.clone();
            move |window, cx| {
                document.update(cx, |document, cx| document.drain_watch(window, cx));
            }
        });

        let app = cx.entity().clone();
        let files_open = self.settings.files_open;
        let settings_open = self.settings.settings_open;
        let editor_width = self.editor_width(window);
        let files_width = px(self
            .settings
            .column_widths
            .map(|widths| widths[0].max(160.0))
            .unwrap_or(220.0));
        let settings_width = px(self
            .settings
            .column_widths
            .map(|widths| widths[3].max(200.0))
            .unwrap_or(300.0));
        let widths = cx.entity().downgrade();

        let mut columns = h_resizable("columns");
        if files_open {
            columns = columns.child(
                resizable_panel()
                    .size(files_width)
                    .size_range(px(160.)..px(480.))
                    .fixed()
                    .child(self.files.clone()),
            );
        }
        columns = columns
            .child(
                resizable_panel()
                    .size(editor_width)
                    .child(views::editor::column(self.document.clone(), cx)),
            )
            .child(resizable_panel().child(views::preview::view(
                self.document.clone(),
                self.style.clone(),
                self.preview.clone(),
                self.preview_scroll.clone(),
                {
                    let app = app.clone();
                    move |_, cx| {
                        app.update(cx, |app, cx| app.toggle_sync(cx));
                    }
                },
                cx,
            )));
        if settings_open {
            columns = columns.child(
                resizable_panel()
                    .size(settings_width)
                    .size_range(px(200.)..px(560.))
                    .fixed()
                    .child(self.settings_column.clone()),
            );
        }

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .on_action({
                let app = app.clone();
                move |_: &ToggleFiles, _, cx| {
                    app.update(cx, |app, cx| app.toggle_files(cx));
                }
            })
            .on_action({
                let app = app.clone();
                move |_: &ToggleSettings, _, cx| {
                    app.update(cx, |app, cx| app.toggle_settings(cx));
                }
            })
            .on_action({
                let app = app.clone();
                move |_: &SaveFile, _, cx| {
                    app.update(cx, |app, cx| {
                        app.document.update(cx, |document, cx| {
                            let _ = document.save(cx);
                        });
                    });
                }
            })
            .on_action({
                let app = app.clone();
                move |_: &OpenPath, window, cx| {
                    app.update(cx, |app, cx| app.begin_open(window, cx));
                }
            })
            .on_action({
                let app = app.clone();
                move |_: &QuickOpen, window, cx| {
                    app.update(cx, |app, cx| app.quick_open(window, cx));
                }
            })
            .on_action({
                let app = app.clone();
                move |_: &FindInFiles, window, cx| {
                    app.update(cx, |app, cx| app.find_in_files(window, cx));
                }
            })
            .on_action({
                let app = app.clone();
                move |_: &ZoomIn, _, cx| {
                    app.update(cx, |app, cx| {
                        app.preview.update(cx, |preview, cx| preview.zoom_in(cx));
                    });
                }
            })
            .on_action({
                let app = app.clone();
                move |_: &ZoomOut, _, cx| {
                    app.update(cx, |app, cx| {
                        app.preview.update(cx, |preview, cx| preview.zoom_out(cx));
                    });
                }
            })
            .on_action({
                let app = app.clone();
                move |_: &ExportPdf, window, cx| {
                    app.update(cx, |app, cx| app.export_pdf(window, cx));
                }
            })
            .on_action({
                let app = app.clone();
                move |_: &ExportDocx, window, cx| {
                    app.update(cx, |app, cx| app.export_docx(window, cx));
                }
            })
            .child(views::top_bar::top_bar(app, document, cx))
            .child(views::sub_bar::sub_bar(self.style.clone(), cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .child(columns.on_resize(move |state, window, cx| {
                        let sizes = state.read(cx).sizes().clone();
                        let widths = widths.clone();
                        window.defer(cx, move |_, cx| {
                            widths.update(cx, |app, _| app.remember_widths(&sizes)).ok();
                        });
                    })),
            )
    }
}

fn spawn_engine(preview: Entity<PreviewState>, cx: &mut Context<AppRoot>) {
    let task = jobs::load_engine(cx);
    cx.spawn(async move |_, cx| {
        let engine = task.await;
        preview.update(cx, |preview, cx| preview.set_engine(engine, cx));
    })
    .detach();
}

fn export_target(app: &AppRoot, extension: &str, cx: &App) -> Option<(PathBuf, String, PathBuf)> {
    let document = app.document.read(cx);
    let workspace = document.workspace();
    let active = workspace.active()?;
    let dest = workspace
        .root()
        .join(&active.relative)
        .with_extension(extension);
    let base = workspace.root().to_path_buf();
    Some((dest, document.text().to_string(), base))
}

fn export_notice(result: &Result<PathBuf, String>) -> String {
    match result {
        Ok(path) => format!(
            "Exported {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        Err(error) => error.clone(),
    }
}

fn finish_export(
    handle: AnyWindowHandle,
    task: Task<Result<PathBuf, String>>,
    cx: &mut Context<AppRoot>,
) {
    cx.spawn(async move |_, cx| {
        let result = task.await;
        cx.update(|cx| {
            handle
                .update(cx, |_, window, cx| {
                    window.push_notification(export_notice(&result), cx);
                })
                .ok();
        });
    })
    .detach();
}

fn workspace_stamp(workspace: &crate::workspace::Workspace) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    workspace.root().hash(&mut hasher);
    for path in workspace.files() {
        path.hash(&mut hasher);
    }
    for buffer in workspace.buffers() {
        buffer.relative.hash(&mut hasher);
        buffer.text.hash(&mut hasher);
    }
    hasher.finish()
}

fn ranked_files(files: &[PathBuf]) -> Vec<PathBuf> {
    let labels: Vec<String> = files
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let ranked = crate::search::quick_open("", &labels);
    let mut ordered: Vec<PathBuf> = ranked
        .iter()
        .filter_map(|label| {
            files
                .iter()
                .find(|path| path.display().to_string() == *label)
                .cloned()
        })
        .collect();
    for path in files {
        if !ordered.contains(path) {
            ordered.push(path.clone());
        }
    }
    ordered
}

const BUILTIN_PRESETS: &[&str] = &[
    "github",
    "compact",
    "academic",
    "minimal",
    "print-friendly",
    "corporate",
];

fn preset_name(name: gpui_kit::Entity<gpui_kit::component::input::InputState>) -> impl IntoElement {
    div()
        .id("preset-name")
        .on_prepaint(|bounds, _, _| probe::record("preset-name", bounds))
        .on_mouse_down(gpui_kit::MouseButton::Left, {
            let name = name.clone();
            move |_, window, cx| {
                name.read(cx).focus_handle(cx).focus(window, cx);
            }
        })
        .child(gpui_kit::component::input::Input::new(&name))
}

fn preset_error(message: Option<String>, cx: &App) -> impl IntoElement {
    let shown = message.clone().unwrap_or_default();
    div()
        .id("preset-error")
        .min_h(px(20.))
        .text_sm()
        .text_color(cx.theme().danger)
        .on_prepaint(move |bounds, _, _| {
            probe::record("preset-error", bounds);
            probe::set_text("preset-error", shown.clone());
        })
        .child(message.unwrap_or_default())
}

fn preset_confirm(
    app: gpui_kit::WeakEntity<AppRoot>,
    name: gpui_kit::Entity<gpui_kit::component::input::InputState>,
) -> impl IntoElement {
    div()
        .id("confirm-preset")
        .on_prepaint(|bounds, _, _| probe::record("confirm-preset", bounds))
        .child(
            Button::new("confirm-preset-button")
                .label("Save")
                .primary()
                .on_click(move |_, window, cx| {
                    let text = name.read(cx).value().to_string();
                    app.update(cx, |app, cx| app.commit_preset(&text, window, cx))
                        .ok();
                }),
        )
}

fn prompt_copy(title: String) -> impl IntoElement {
    let shown = title.clone();
    div()
        .id("unsaved-prompt")
        .on_prepaint(move |bounds, _, _| {
            probe::record("unsaved-prompt", bounds);
            probe::set_text("unsaved-prompt", shown.clone());
        })
        .child(title)
}

fn prompt_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_none()
        .on_prepaint(move |bounds, _, _| probe::record(id, bounds))
        .child(
            Button::new(format!("{id}-button"))
                .label(label)
                .outline()
                .on_click(move |_, window, cx| on_click(window, cx)),
        )
}

fn fresh_name(files: &[PathBuf]) -> String {
    if !files.iter().any(|path| path.as_os_str() == "untitled.md") {
        return "untitled.md".into();
    }
    let mut n = 2;
    loop {
        let name = format!("untitled-{n}.md");
        if !files.iter().any(|path| path.as_os_str() == name.as_str()) {
            return name;
        }
        n += 1;
    }
}

fn map_widths(
    current: Option<[f32; 4]>,
    files_open: bool,
    settings_open: bool,
    sizes: &[Pixels],
) -> [f32; 4] {
    let mut widths = current.unwrap_or([220.0, 480.0, 640.0, 300.0]);
    let mut index = 0;
    if files_open {
        if let Some(size) = sizes.get(index) {
            widths[0] = size.as_f32();
        }
        index += 1;
    }
    if let Some(size) = sizes.get(index) {
        widths[1] = size.as_f32();
    }
    index += 1;
    if let Some(size) = sizes.get(index) {
        widths[2] = size.as_f32();
    }
    index += 1;
    if settings_open && let Some(size) = sizes.get(index) {
        widths[3] = size.as_f32();
    }
    widths
}

fn widths_changed(old: Option<[f32; 4]>, new: [f32; 4]) -> bool {
    match old {
        None => true,
        Some(old) => old
            .iter()
            .zip(new)
            .any(|(before, after)| (before - after).abs() > 0.5),
    }
}

pub fn bind_keys(cx: &mut App) {
    use gpui_kit::KeyBinding;
    cx.bind_keys([
        KeyBinding::new("ctrl-b", ToggleFiles, None),
        KeyBinding::new("ctrl-,", ToggleSettings, None),
        KeyBinding::new("ctrl-p", QuickOpen, None),
        KeyBinding::new("ctrl-shift-f", FindInFiles, None),
        KeyBinding::new("ctrl-s", SaveFile, None),
        KeyBinding::new("ctrl-e", ExportPdf, None),
        KeyBinding::new("ctrl-shift-e", ExportDocx, None),
        KeyBinding::new("ctrl-=", ZoomIn, None),
        KeyBinding::new("ctrl--", ZoomOut, None),
        KeyBinding::new("ctrl-o", OpenPath, None),
    ]);
}

pub fn run() {
    let folder = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let settings_path = settings::config_dir().join("settings.toml");
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            bind_keys(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| AppRoot::open(&folder, &settings_path, None, true, window, cx))
            })
            .expect("open window");
        });
}
