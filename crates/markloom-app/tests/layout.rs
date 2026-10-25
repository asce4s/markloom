//! Window layout: columns, insert, status, unsaved prompt, preview scroll.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, InputEvent as _, KeyUpEvent, Keystroke,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollStrategy, TestAppContext,
    Window, WindowBounds, WindowOptions, px, size,
};
use markloom_app::app::{AppRoot, bind_keys};
use markloom_app::probe;
use markloom_mermaid::DiagramService;
use markloom_typeset::{Engine, EngineOptions};
use toml::Value;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn scratch(body: &str) -> PathBuf {
    let n = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("markloom-layout-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("note.md"), body).unwrap();
    dir.join("note.md")
}

fn settings_path(note: &Path) -> PathBuf {
    note.parent().unwrap().join("settings.toml")
}

struct Harness {
    window: AnyWindowHandle,
    app: Entity<AppRoot>,
}

fn open(cx: &mut TestAppContext, note: &Path, engine: Option<Engine>) -> Harness {
    let settings = settings_path(note);
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        let mut app = None;
        let (window, _) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1100.), px(700.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let root = cx.new(|cx| AppRoot::open(note, &settings, engine, false, window, cx));
                app = Some(root.clone());
                root
            },
        )
        .expect("open test window");
        Harness {
            window,
            app: app.expect("app"),
        }
    })
}

fn engine() -> Engine {
    Engine::new(
        EngineOptions {
            system_fonts: false,
        },
        Arc::new(DiagramService::with_defaults()),
    )
}

fn drain(cx: &mut TestAppContext) {
    for _ in 0..4_000 {
        if !cx.executor().tick() {
            break;
        }
    }
}

fn advance(cx: &mut TestAppContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    drain(cx);
}

fn frame(window: &mut Window, cx: &mut App) {
    window.refresh();
    window.draw(cx).clear(cx);
}

fn painted(id: &str) -> bool {
    probe::bounds(id).is_some()
}

fn click(window: &mut Window, id: &'static str, cx: &mut App) {
    frame(window, cx);
    let bounds = probe::bounds(id).unwrap_or_else(|| panic!("missing {id}"));
    let position = bounds.center();
    let modifiers = window.modifiers();
    window.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers,
        }
        .to_platform_input(),
        cx,
    );
    frame(window, cx);
    window.dispatch_event(
        MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    frame(window, cx);
    window.dispatch_event(
        MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
    frame(window, cx);
}

fn press(window: &mut Window, key: &str, cx: &mut App) {
    let key = Keystroke::parse(key).expect("key");
    frame(window, cx);
    let key = key.with_simulated_ime();
    window.dispatch_keystroke(key.clone(), cx);
    window.dispatch_event(KeyUpEvent { keystroke: key }.to_platform_input(), cx);
    frame(window, cx);
}

#[gpui_kit::test]
fn default_layout_hides_files(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        frame(window, cx);
        assert!(!painted("files"), "files start hidden");
        assert!(painted("settings"), "settings start open");
    })
    .unwrap();
}

#[gpui_kit::test]
fn ctrl_b_toggles_files(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        press(window, "ctrl-b", cx);
        assert!(painted("files"), "ctrl-b shows files");
        press(window, "ctrl-b", cx);
        assert!(!painted("files"), "ctrl-b hides files");
    })
    .unwrap();
}

#[gpui_kit::test]
fn insert_table_at_cursor(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        click(window, "insert", cx);
        frame(window, cx);
        click(window, "insert-table", cx);
        let document = harness.app.read(cx).document().clone();
        let editor = document.read(cx).editor().clone();
        let selected = editor.read(cx).selected_text().to_string();
        let text = document.read(cx).text().to_string();
        assert_eq!(selected, "Column");
        assert!(text.contains("| Column |"), "{text}");
    })
    .unwrap();
}

#[gpui_kit::test]
fn status_shows_word_count(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        let document = harness.app.read(cx).document().clone();
        document.update(cx, |document, cx| document.set_value("a b c", window, cx));
        frame(window, cx);
        let label = probe::text("editor-words").unwrap_or_default();
        assert_eq!(label, "3 words · 1 lines");
    })
    .unwrap();
}

#[gpui_kit::test]
fn open_folder_with_dirty_buffer_prompts(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        let document = harness.app.read(cx).document().clone();
        let root_before = document.read(cx).workspace().root().to_path_buf();
        document.update(cx, |document, cx| {
            document.set_value("dirty", window, cx);
        });
        press(window, "ctrl-o", cx);
        frame(window, cx);
        let prompt = probe::text("unsaved-prompt").unwrap_or_default();
        assert_eq!(prompt, "Save changes to 1 file(s)?");
        assert_eq!(
            document.read(cx).workspace().root(),
            root_before.as_path(),
            "the folder stays until the prompt is answered"
        );
        click(window, "cancel-changes", cx);
        assert_eq!(document.read(cx).text(), "dirty");
        assert_eq!(document.read(cx).workspace().root(), root_before.as_path());
    })
    .unwrap();
}

#[gpui_kit::test]
fn save_failure_keeps_the_window(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        let document = harness.app.read(cx).document().clone();
        let root_before = document.read(cx).workspace().root().to_path_buf();
        document.update(cx, |document, cx| {
            document.set_value("dirty", window, cx);
        });
        press(window, "ctrl-o", cx);
        frame(window, cx);
        std::fs::remove_file(&note).unwrap();
        std::fs::create_dir(&note).unwrap();
        click(window, "save-changes", cx);
        assert_eq!(document.read(cx).text(), "dirty");
        assert_eq!(document.read(cx).workspace().root(), root_before.as_path());
        assert!(
            painted("unsaved-prompt") || probe::text("notice").is_some(),
            "a failed save stays in the window and says why"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn preview_scroll_not_overridden(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, Some(engine()));
    cx.update_window(harness.window, |_, window, cx| {
        let style = harness.app.read(cx).style().clone();
        let document = harness.app.read(cx).document().clone();
        style.update(cx, |style, cx| {
            style.set("page.break_before_h1", Value::Boolean(true), cx);
        });
        document.update(cx, |document, cx| {
            document.set_value("# A\n\n# B\n\n# C\n\n# D\n\n# E\n\n# F\n", window, cx);
        });
    })
    .unwrap();
    advance(cx, 400);
    let handle = cx
        .update_window(harness.window, |_, _, cx| {
            assert!(
                harness
                    .app
                    .read(cx)
                    .preview()
                    .read(cx)
                    .page_count()
                    .unwrap_or(0)
                    >= 6,
                "need six pages to scroll to page 5"
            );
            harness.app.read(cx).preview_scroll().clone()
        })
        .unwrap();
    handle.scroll_to_item(5, ScrollStrategy::Top);
    cx.update_window(harness.window, |_, window, cx| frame(window, cx))
        .unwrap();
    let pages = cx
        .update_window(harness.window, |_, _, cx| {
            harness
                .app
                .read(cx)
                .preview()
                .read(cx)
                .page_count()
                .unwrap_or(0)
        })
        .unwrap();
    let offset = scroll_y(&handle);
    let item = content_height(&handle) / pages as f32;
    assert!(
        (offset + item * 5.0).abs() < 1.0,
        "page 5 is the scroll target, offset {offset} item {item}"
    );
    for _ in 0..20 {
        advance(cx, 100);
        cx.update_window(harness.window, |_, window, cx| frame(window, cx))
            .unwrap();
    }
    let later = scroll_y(&handle);
    assert!(
        (later - offset).abs() < 1.0,
        "blinking must not pull the preview back: {offset} became {later}"
    );
}

#[gpui_kit::test]
fn unknown_default_preset_notifies(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let settings = markloom_app::settings::Settings {
        default_preset: "nope".into(),
        ..markloom_app::settings::Settings::default()
    };
    markloom_app::settings::save(&settings_path(&note), &settings).unwrap();
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        frame(window, cx);
        assert_eq!(harness.app.read(cx).style().read(cx).preset(), "github");
        let notice = probe::text("notice").unwrap_or_default();
        assert!(notice.contains("github"), "{notice}");
    })
    .unwrap();
}

#[gpui_kit::test]
fn user_preset_is_selected_at_startup(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let presets = note.parent().unwrap().join("presets");
    std::fs::create_dir_all(&presets).unwrap();
    std::fs::write(presets.join("mine.toml"), "extends = \"github\"\n").unwrap();
    let settings = markloom_app::settings::Settings {
        default_preset: "mine".into(),
        ..markloom_app::settings::Settings::default()
    };
    markloom_app::settings::save(&settings_path(&note), &settings).unwrap();
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        frame(window, cx);
        assert_eq!(harness.app.read(cx).style().read(cx).preset(), "mine");
    })
    .unwrap();
}

#[gpui_kit::test]
fn non_utf8_open_sets_a_notice(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    std::fs::write(note.parent().unwrap().join("bad.md"), [0xff, 0xfe, 0x00]).unwrap();
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        let document = harness.app.read(cx).document().clone();
        let error = document.update(cx, |document, cx| {
            document.open_file(Path::new("bad.md"), window, cx)
        });
        assert!(error.is_err());
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        frame(window, cx);
        let notice = probe::text("notice").unwrap_or_default();
        assert!(notice.contains("UTF-8"), "{notice}");
        assert_eq!(harness.app.read(cx).document().read(cx).text(), "hello\n");
    })
    .unwrap();
}

fn scroll_y(handle: &gpui_kit::UniformListScrollHandle) -> f32 {
    handle.0.borrow().base_handle.offset().y.as_f32()
}

fn content_height(handle: &gpui_kit::UniformListScrollHandle) -> f32 {
    handle
        .0
        .borrow()
        .last_item_size
        .expect("measured page")
        .contents
        .height
        .as_f32()
}
