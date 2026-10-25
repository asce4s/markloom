//! Exports, font loading, and search stay off the UI thread.

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{
    AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions, px, size,
};
use markloom_app::app::{AppRoot, bind_keys};
use markloom_app::jobs;
use markloom_app::preview_state::PreviewStatus;
use markloom_core::parse;
use markloom_mermaid::DiagramService;
use markloom_style::StyleSheet;
use markloom_typeset::{Engine, EngineOptions, PdfMeta};

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn scratch(body: &str) -> PathBuf {
    let n = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("markloom-jobs-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("note.md"), body).unwrap();
    dir.join("note.md")
}

fn open(cx: &mut TestAppContext, note: &std::path::Path, load_fonts: bool) -> Entity<AppRoot> {
    let settings = note.parent().unwrap().join("settings.toml");
    cx.update(|cx| {
        gpui_kit::init(cx);
        bind_keys(cx);
        let mut app = None;
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    Default::default(),
                    size(px(1100.), px(700.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let root =
                    cx.new(|cx| AppRoot::open(note, &settings, None, load_fonts, window, cx));
                app = Some(root.clone());
                root
            },
        )
        .expect("open test window");
        app.expect("app")
    })
}

fn drain(cx: &mut TestAppContext) {
    for _ in 0..8_000 {
        if !cx.executor().tick() {
            break;
        }
    }
}

fn advance(cx: &mut TestAppContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    drain(cx);
}

fn compiled_hello() -> markloom_typeset::Compiled {
    let mut engine = Engine::new(
        EngineOptions {
            system_fonts: false,
        },
        Arc::new(DiagramService::with_defaults()),
    );
    engine
        .compile(
            &parse("hello\n"),
            &StyleSheet::default(),
            std::path::Path::new("."),
        )
        .expect("compile")
}

#[gpui_kit::test]
fn export_runs_off_thread(cx: &mut TestAppContext) {
    let dest = scratch("hello\n").with_extension("pdf");
    let compiled = compiled_hello();
    let slot = Arc::new(Mutex::new(None));
    cx.update(|cx| {
        let task = jobs::export_pdf(compiled, dest.clone(), PdfMeta::default(), cx);
        let slot = slot.clone();
        cx.spawn(async move |_| {
            let result = task.await;
            *slot.lock().expect("slot") = Some(result);
        })
        .detach();
    });
    assert!(
        slot.lock().expect("slot").is_none(),
        "starting the export must return before the PDF is written"
    );
    assert!(
        !dest.exists(),
        "the UI update must not write the PDF before the executor runs"
    );
    cx.executor().advance_clock(Duration::from_millis(20));
    drain(cx);
    let result = slot.lock().expect("slot").clone().expect("export finished");
    let path = result.expect("pdf");
    let bytes = std::fs::read(&path).expect("pdf bytes");
    assert!(bytes.starts_with(b"%PDF-"), "pdf prefix");
}

#[gpui_kit::test]
fn fonts_load_then_compile(cx: &mut TestAppContext) {
    let note = scratch("# Hello\n");
    let app = open(cx, &note, true);
    cx.update(|cx| {
        assert!(matches!(
            app.read(cx).preview().read(cx).status(),
            PreviewStatus::LoadingFonts
        ));
    });
    advance(cx, 2_000);
    cx.update(|cx| {
        assert!(
            matches!(
                app.read(cx).preview().read(cx).status(),
                PreviewStatus::Ready { .. }
            ),
            "fonts then compile, got {:?}",
            app.read(cx).preview().read(cx).status()
        );
    });
}

#[gpui_kit::test]
fn search_is_cached(cx: &mut TestAppContext) {
    let note = scratch("hello world\n");
    let app = open(cx, &note, false);
    cx.update(|cx| {
        app.update(cx, |app, cx| app.find("hello", cx));
    });
    advance(cx, 200);
    cx.update(|cx| {
        assert_eq!(app.read(cx).search_runs(), 1, "the first query runs once");
        app.update(cx, |app, cx| app.find("hello", cx));
    });
    advance(cx, 200);
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).search_runs(),
            1,
            "the same query does not search again"
        );
    });
}

#[gpui_kit::test]
fn search_burst_runs_once(cx: &mut TestAppContext) {
    let note = scratch("hello world\n");
    let app = open(cx, &note, false);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.find("h", cx);
            app.find("he", cx);
        });
    });
    advance(cx, 200);
    cx.update(|cx| {
        assert_eq!(
            app.read(cx).search_starts(),
            1,
            "keystrokes inside the debounce start one search"
        );
    });
}
