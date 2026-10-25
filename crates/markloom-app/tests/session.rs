use std::fs;

use markloom_app::search::{quick_open, search_files};
use markloom_app::style_session::StyleSession;
use markloom_style::{Length, PresetRegistry};
use toml::Value;

#[test]
fn overrides_change_the_sheet_and_unknown_preset_is_an_error() {
    let registry = PresetRegistry::builtin();
    let mut session = StyleSession::new("github");
    session.set("typography.base_size", Value::String("14pt".into()));
    session.set("colors.text", Value::String("#112233".into()));
    session.set("table.borders", Value::String("Horizontal".into()));
    session.set("diagram.theme", Value::String("Dark".into()));
    session.set("diagram.scale", Value::Float(1.5));
    session.set(
        "typography.body_font.0",
        Value::String("Libertinus Serif".into()),
    );
    let sheet = session.resolve(&registry).unwrap();
    assert_eq!(sheet.typography.body_font[0], "Libertinus Serif");
    assert!((sheet.diagram.scale - 1.5).abs() < 0.001);
    session.set_preset("no-such-preset");
    assert!(session.resolve(&registry).is_err());
    session.reset("academic");
    assert_eq!(session.preset(), "academic");
    assert!(session.overrides().is_empty());
}

#[test]
fn save_preset_writes_toml() {
    let dir = std::env::temp_dir().join(format!("markloom-preset-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let registry = PresetRegistry::builtin();
    let session = StyleSession::new("github");
    let sheet = session.resolve(&registry).unwrap();
    session.save_preset(&dir, "mine", &sheet).unwrap();
    let text = fs::read_to_string(dir.join("mine.toml")).unwrap();
    assert!(text.contains("base_size"));
}

#[test]
fn set_nested_path() {
    let registry = PresetRegistry::builtin();
    let mut session = StyleSession::new("github");
    session.set("page.margin.top", Value::String("20mm".into()));
    let sheet = session.resolve(&registry).expect("resolve");
    let top = match sheet.page.margin.top {
        Length::Pt(points) => points,
        Length::Em(em) => panic!("expected pt, got {em}em"),
    };
    let expected = 20.0 * 72.0 / 25.4;
    assert!((top - expected).abs() < 0.01, "{top} vs {expected}");
}

#[test]
fn set_array_index() {
    let registry = PresetRegistry::builtin();
    let mut session = StyleSession::new("github");
    let preset = session.resolve(&registry).expect("preset");
    session.set("typography.heading_scale.0", Value::Float(2.4));
    for (index, value) in preset.typography.heading_scale.iter().enumerate().skip(1) {
        session.set(
            &format!("typography.heading_scale.{index}"),
            Value::Float(f64::from(*value)),
        );
    }
    let sheet = session.resolve(&registry).expect("resolve");
    assert_eq!(sheet.typography.heading_scale[0], 2.4);
}

#[test]
fn clear_prefix() {
    let registry = PresetRegistry::builtin();
    let mut session = StyleSession::new("github");
    session.set("table.zebra", Value::String("#f8fafc".into()));
    session.set("table.border_color", Value::String("#112233".into()));
    session.clear("table");
    assert!(!session.is_overridden("table"));
    assert!(session.overrides().is_empty());
    assert!(session.resolve(&registry).is_ok());
}

#[test]
fn clear_all() {
    let registry = PresetRegistry::builtin();
    let mut session = StyleSession::new("github");
    session.set("page.toc", Value::Boolean(true));
    session.clear_all();
    assert!(session.overrides().is_empty());
    assert!(!session.is_overridden("page.toc"));
    let sheet = session.resolve(&registry).expect("resolve");
    assert!(!sheet.page.toc);
}

#[test]
fn bad_value_reports_error() {
    let registry = PresetRegistry::builtin();
    let mut session = StyleSession::new("github");
    session.set("page.toc_depth", Value::Integer(9));
    assert!(session.resolve(&registry).is_err());
    session.clear("page.toc_depth");
    assert!(session.resolve(&registry).is_ok());
}

#[test]
fn quick_open_and_search_rank_matches() {
    let paths = vec![
        "notes/other.md".into(),
        "fixtures/index.md".into(),
        "readme.md".into(),
    ];
    let opened = quick_open("idx", &paths);
    assert_eq!(opened, vec!["fixtures/index.md"]);
    assert!(quick_open("zzz", &paths).is_empty());
    let hits = search_files(
        "Hello",
        &[
            ("a.md".into(), "hello there\nnope\n".into()),
            ("b.md".into(), "HELLO\n".into()),
        ],
    );
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[1].path, "b.md");
}
