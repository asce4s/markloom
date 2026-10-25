//! Document settings: margins, orientation, section reset, custom Typst, presets.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, InputEvent as _, KeyUpEvent, Keystroke,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollDelta, ScrollWheelEvent,
    TestAppContext, Window, WindowBounds, WindowOptions, point, px, size,
};
use markloom_app::app::{AppRoot, bind_keys};
use markloom_app::preview_state::PreviewStatus;
use markloom_app::probe;
use markloom_mermaid::DiagramService;
use markloom_style::Length;
use markloom_typeset::{Engine, EngineOptions};
use toml::Value;

static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn scratch(body: &str) -> PathBuf {
    let n = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("markloom-settings-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("note.md"), body).unwrap();
    dir.join("note.md")
}

struct Harness {
    window: AnyWindowHandle,
    app: Entity<AppRoot>,
}

fn open(cx: &mut TestAppContext, note: &Path, engine: Option<Engine>) -> Harness {
    let settings = note.parent().unwrap().join("settings.toml");
    cx.update(|cx| {
        cx.set_reduce_motion(true);
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

fn reveal(window: &mut Window, id: &str, cx: &mut App) {
    for _ in 0..24 {
        frame(window, cx);
        let Some(bounds) = probe::bounds(id) else {
            scroll_settings(window, px(-200.), cx);
            continue;
        };
        let y = bounds.center().y.as_f32();
        if (40.0..660.0).contains(&y) {
            return;
        }
        let delta = if y >= 660.0 { px(-200.) } else { px(200.) };
        scroll_settings(window, delta, cx);
    }
}

fn scroll_settings(window: &mut Window, delta_y: gpui_kit::Pixels, cx: &mut App) {
    let settings = probe::bounds("settings");
    let x = settings
        .map(|bounds| bounds.origin.x + px(24.))
        .unwrap_or(px(820.));
    let y = settings
        .map(|bounds| bounds.origin.y + px(120.))
        .unwrap_or(px(200.));
    window.dispatch_event(
        ScrollWheelEvent {
            position: point(x, y),
            delta: ScrollDelta::Pixels(point(px(0.), delta_y)),
            ..ScrollWheelEvent::default()
        }
        .to_platform_input(),
        cx,
    );
}

fn click(window: &mut Window, id: &'static str, cx: &mut App) {
    reveal(window, id, cx);
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
    let key = Keystroke::parse(key).expect("key").with_simulated_ime();
    frame(window, cx);
    window.dispatch_keystroke(key.clone(), cx);
    window.dispatch_event(KeyUpEvent { keystroke: key }.to_platform_input(), cx);
    frame(window, cx);
}

fn type_text(window: &mut Window, text: &str, cx: &mut App) {
    let key = Keystroke {
        key: "x".into(),
        key_char: Some(text.into()),
        ..Keystroke::default()
    };
    frame(window, cx);
    window.dispatch_keystroke(key.clone(), cx);
    window.dispatch_event(KeyUpEvent { keystroke: key }.to_platform_input(), cx);
    frame(window, cx);
}

fn margin_mm(length: Length) -> f32 {
    match length {
        Length::Pt(points) => points * 25.4 / 72.0,
        Length::Em(ems) => panic!("expected a point length, got {ems}em"),
    }
}

#[gpui_kit::test]
fn margin_input_writes_mm(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        click(window, "margin-top", cx);
        press(window, "ctrl-a", cx);
        press(window, "2", cx);
        press(window, "5", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, _, cx| {
        let style = harness.app.read(cx).style().clone();
        assert_eq!(
            style.read(cx).override_text("page.margin.top").as_deref(),
            Some("25mm")
        );
        let mm = margin_mm(style.read(cx).sheet().page.margin.top);
        assert!((mm - 25.0).abs() < 0.05, "resolved margin {mm}mm");
    })
    .unwrap();
}

#[gpui_kit::test]
fn orientation_select_updates_status(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        click(window, "orientation", cx);
        click(window, "orientation-landscape", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        frame(window, cx);
        assert_eq!(
            probe::text("preview-paper").unwrap_or_default(),
            "A4 · Landscape"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn section_reset_clears_only_section(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        let style = harness.app.read(cx).style().clone();
        style.update(cx, |style, cx| {
            style.set("table.borders", Value::String("None".into()), cx);
            style.set("page.orientation", Value::String("landscape".into()), cx);
        });
        click(window, "section-tables", cx);
        click(window, "reset-tables", cx);
        let style = style.read(cx);
        assert!(
            !style.is_overridden("table"),
            "tables reset drops table overrides"
        );
        assert!(
            style.is_overridden("page.orientation"),
            "page override stays"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn header_reset_clears_all(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        let style = harness.app.read(cx).style().clone();
        style.update(cx, |style, cx| {
            style.set("page.orientation", Value::String("landscape".into()), cx);
            style.set("table.borders", Value::String("None".into()), cx);
        });
        click(window, "reset-style", cx);
        let style = style.read(cx);
        assert!(!style.is_overridden("page.orientation"));
        assert!(!style.is_overridden("table"));
    })
    .unwrap();
}

#[gpui_kit::test]
fn custom_typst_error_shows_in_status(cx: &mut TestAppContext) {
    let note = scratch("# Hello\n");
    let harness = open(cx, &note, Some(engine()));
    advance(cx, 400);
    cx.update_window(harness.window, |_, window, cx| {
        let pages = harness
            .app
            .read(cx)
            .preview()
            .read(cx)
            .page_count()
            .unwrap_or(0);
        assert!(pages >= 1, "a good page exists before the bad custom code");
        click(window, "section-custom", cx);
        click(window, "custom-typst", cx);
        type_text(window, "#let x =", cx);
    })
    .unwrap();
    // The field waits 400 ms, then the preview waits its own debounce.
    advance(cx, 500);
    advance(cx, 400);
    cx.update_window(harness.window, |_, _, cx| {
        let preview = harness.app.read(cx).preview().clone();
        let status = preview.read(cx).status().clone();
        let PreviewStatus::Error(message) = status else {
            panic!("expected a custom typst error, got {status:?}");
        };
        assert!(message.starts_with("Custom Typst:"), "status was {message}");
        assert!(
            preview.read(cx).page_count().unwrap_or(0) >= 1,
            "the last good page stays"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn save_as_preset_selects_it(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        let style = harness.app.read(cx).style().clone();
        style.update(cx, |style, cx| {
            style.set("page.margin.top", Value::String("25mm".into()), cx);
        });
        let before = margin_mm(style.read(cx).sheet().page.margin.top);
        click(window, "more", cx);
        click(window, "save-preset", cx);
        click(window, "preset-name", cx);
        type_text(window, "mine", cx);
        click(window, "confirm-preset", cx);
        let style = style.read(cx);
        assert!(style.names().iter().any(|name| name == "mine"));
        assert_eq!(style.preset(), "mine");
        assert!(!style.is_overridden("page.margin.top"));
        let after = margin_mm(style.sheet().page.margin.top);
        assert!((after - before).abs() < 0.05, "{before} vs {after}");
        let preset = note.parent().unwrap().join("presets").join("mine.toml");
        let text = std::fs::read_to_string(&preset).unwrap_or_default();
        assert!(
            text.contains("extends"),
            "preset file should extend the previous preset: {text}"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn builtin_name_rejected(cx: &mut TestAppContext) {
    let note = scratch("hello\n");
    let harness = open(cx, &note, None);
    cx.update_window(harness.window, |_, window, cx| {
        click(window, "more", cx);
        click(window, "save-preset", cx);
        click(window, "preset-name", cx);
        type_text(window, "github", cx);
        click(window, "confirm-preset", cx);
        assert_eq!(
            probe::text("preset-error").unwrap_or_default(),
            "Choose a name that isn't a built-in preset"
        );
        let preset = note.parent().unwrap().join("presets").join("github.toml");
        assert!(!preset.exists(), "a built-in name writes no file");
    })
    .unwrap();
}
