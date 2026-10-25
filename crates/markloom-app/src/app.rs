//! The application window: bars, columns, and the unsaved-changes prompt.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::base::ElementExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::{
    ActiveTheme as _, Theme, ThemeMode, WindowExt as _, h_resizable, resizable_panel, v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement, PathPromptOptions, Pixels, Render, Styled, Subscription,
    UniformListScrollHandle, Window, WindowOptions, div, px,
};
use markloom_mermaid::DiagramService;
use markloom_typeset::{Engine, EngineOptions};

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
    _document: Subscription,
    _preview: Subscription,
    _style: Subscription,
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
        let settings_column = cx.new(|_| SettingsColumn::new(style.clone()));
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

    /// Task 8 opens the name dialog. The menu item is already wired.
    pub fn save_style_as_preset(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

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
        let files = document.read(cx).workspace().files().to_vec();
        window.open_dialog(cx, move |dialog, _, _| {
            let mut dialog = dialog.title("Quick open");
            for file in files.clone() {
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

    fn find_in_files(&mut self, window: &mut Window, cx: &mut App) {
        window.open_dialog(cx, |dialog, _, _| {
            dialog
                .title("Find in files")
                .child("Results appear after you stop typing.")
        });
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
            .on_action(|_: &ExportPdf, _, _| {})
            .on_action(|_: &ExportDocx, _, _| {})
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
    cx.spawn(async move |_, cx| {
        let engine = cx
            .background_spawn(async move {
                Engine::new(
                    EngineOptions { system_fonts: true },
                    Arc::new(DiagramService::with_defaults()),
                )
            })
            .await;
        preview.update(cx, |preview, cx| preview.set_engine(engine, cx));
    })
    .detach();
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
