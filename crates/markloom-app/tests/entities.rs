//! Document, style, and preview entities.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, atomic::AtomicU64};
use std::time::Duration;

use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Context, Entity, IntoElement, Render, TestAppContext,
    Window, WindowBounds, WindowOptions, div, px, size,
};
use markloom_app::document::{Document, DocumentEvent};
use markloom_app::preview_state::PreviewState;
use markloom_app::style_state::StyleState;
use markloom_mermaid::DiagramService;
use markloom_typeset::{Engine, EngineOptions};
use toml::Value;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

struct Host;

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

struct Harness {
    window: AnyWindowHandle,
    document: Entity<Document>,
    style: Entity<StyleState>,
    preview: Entity<PreviewState>,
}

fn temp_file(body: &str) -> PathBuf {
    let n = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("markloom-entities-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("note.md");
    std::fs::write(&path, body).unwrap();
    path
}

fn open(cx: &mut TestAppContext, path: &Path) -> Harness {
    cx.update(|cx| {
        gpui_kit::init(cx);
        let mut document = None;
        let mut style = None;
        let mut preview = None;
        let (window, _host) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(800.), px(600.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let doc = cx
                    .new(|cx| Document::open(path, window, cx).expect("open document"))
                    .clone();
                let sheet = cx.new(|cx| StyleState::new("github", cx));
                let engine = Engine::new(
                    EngineOptions {
                        system_fonts: false,
                    },
                    Arc::new(DiagramService::with_defaults()),
                );
                let pages =
                    cx.new(|cx| PreviewState::with_engine(engine, doc.clone(), sheet.clone(), cx));
                document = Some(doc);
                style = Some(sheet);
                preview = Some(pages);
                cx.new(|_| Host)
            },
        )
        .expect("open test window");
        Harness {
            window,
            document: document.expect("document"),
            style: style.expect("style"),
            preview: preview.expect("preview"),
        }
    })
}

fn set_text(cx: &mut TestAppContext, harness: &Harness, text: &str) {
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .document
            .update(cx, |document, cx| document.set_value(text, window, cx));
    })
    .unwrap();
}

fn drain_executor(cx: &mut TestAppContext) {
    for _ in 0..4_000 {
        if !cx.executor().tick() {
            break;
        }
    }
}

fn advance(cx: &mut TestAppContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    drain_executor(cx);
}

#[gpui_kit::test]
fn typing_compiles_once_after_debounce(cx: &mut TestAppContext) {
    let harness = open(cx, &temp_file("hello"));
    let sheet = harness
        .style
        .update(cx, |style, _| Arc::as_ptr(style.sheet()) as usize);
    for index in 0..5 {
        set_text(cx, &harness, &format!("hello {index}"));
    }
    advance(cx, 200);
    let compiles = harness
        .preview
        .update(cx, |preview, _| preview.compile_count());
    let sheet_after = harness
        .style
        .update(cx, |style, _| Arc::as_ptr(style.sheet()) as usize);
    assert_eq!(compiles, 1, "debounced edits compile once");
    assert_eq!(sheet, sheet_after, "typing does not re-resolve the style");
}

#[gpui_kit::test]
fn stale_compile_result_is_ignored(cx: &mut TestAppContext) {
    let harness = open(cx, &temp_file("hello"));
    harness.style.update(cx, |style, cx| {
        style.set("page.break_before_h1", Value::Boolean(true), cx);
    });
    advance(cx, 200);
    harness.preview.update(cx, |preview, _| {
        preview.arm_compile_hold(Duration::from_millis(500))
    });
    set_text(cx, &harness, "hello!");
    cx.executor().advance_clock(Duration::from_millis(200));
    let mut started = false;
    for _ in 0..4_000 {
        if harness
            .preview
            .update(cx, |preview, _| preview.compile_entered())
        {
            started = true;
            break;
        }
        if !cx.executor().tick() {
            cx.executor().advance_clock(Duration::from_millis(1));
        }
    }
    assert!(started, "the first compile should be in flight");
    set_text(cx, &harness, "# A\n\n# B\n");
    advance(cx, 800);
    let pages = harness
        .preview
        .update(cx, |preview, _| preview.page_count());
    assert_eq!(pages, Some(2), "a stale one-page compile must not win");
}

#[gpui_kit::test]
fn cursor_blink_does_not_emit(cx: &mut TestAppContext) {
    let harness = open(cx, &temp_file("hello"));
    let events = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&events);
    let document = harness.document.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&document, move |_, event: &DocumentEvent, _| {
            log.lock().expect("event log").push(event.clone());
        })
    });
    advance(cx, 2_000);
    let cursor_events = events
        .lock()
        .expect("event log")
        .iter()
        .filter(|event| matches!(event, DocumentEvent::CursorLineChanged(_)))
        .count();
    assert_eq!(cursor_events, 0);
}

#[gpui_kit::test]
fn save_keeps_cursor(cx: &mut TestAppContext) {
    let body = "abcde\nabcde\nabcde\nabcde\nabcde\n";
    let harness = open(cx, &temp_file(body));
    let events = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&events);
    let document = harness.document.clone();
    let _subscription = cx.update(|cx| {
        cx.subscribe(&document, move |_, event: &DocumentEvent, _| {
            log.lock().expect("event log").push(event.clone());
        })
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness.document.update(cx, |document, cx| {
            document.set_cursor(5, 3, window, cx);
        });
    })
    .unwrap();
    assert_eq!(
        harness.document.update(cx, |document, _| document.cursor()),
        (5, 3)
    );
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .document
            .update(cx, |document, cx| document.save(cx).unwrap());
        std::thread::sleep(Duration::from_millis(200));
        harness
            .document
            .update(cx, |document, cx| document.drain_watch(window, cx));
    })
    .unwrap();
    assert_eq!(
        harness.document.update(cx, |document, _| document.cursor()),
        (5, 3)
    );
    let reloaded = events
        .lock()
        .expect("event log")
        .iter()
        .any(|event| matches!(event, DocumentEvent::Reloaded));
    assert!(!reloaded, "an own save must not reload the editor");
}

#[gpui_kit::test]
fn paper_size_change_recompiles_once(cx: &mut TestAppContext) {
    let harness = open(cx, &temp_file("hello"));
    harness.style.update(cx, |style, cx| {
        style.set("page.size", Value::String("letter".into()), cx);
    });
    advance(cx, 200);
    let compiles = harness
        .preview
        .update(cx, |preview, _| preview.compile_count());
    let width = harness
        .preview
        .update(cx, |preview, _| preview.page_width_pt());
    assert_eq!(compiles, 1);
    let width = width.expect("compiled page");
    assert!(
        (width - 612.0).abs() < 0.5,
        "letter width was {width}, expected 612 pt"
    );
}

#[gpui_kit::test]
fn replaced_images_are_dropped(cx: &mut TestAppContext) {
    let harness = open(cx, &temp_file("alpha"));
    advance(cx, 200);
    let first = harness
        .preview
        .update(cx, |preview, _| preview.page_image(0).map(|image| image.id))
        .expect("first page image");
    set_text(cx, &harness, "beta is a different page");
    advance(cx, 200);
    let (held, visible, still_held) = harness.preview.update(cx, |preview, _| {
        (
            preview.held_len(),
            preview.visible_len(),
            preview.holds(first),
        )
    });
    assert!(held <= visible + 2, "held {held}, visible {visible}");
    assert!(!still_held, "the replaced image is still held");
}
