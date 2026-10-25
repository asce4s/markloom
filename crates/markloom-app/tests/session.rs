use std::fs;

use markloom_app::search::{quick_open, search_files};
use markloom_app::style_session::StyleSession;
use markloom_style::PresetRegistry;

#[test]
fn overrides_change_the_sheet_and_unknown_preset_is_an_error() {
    let registry = PresetRegistry::builtin();
    let mut session = StyleSession::new("github");
    session.set_base_size_pt(14.0);
    session.set_text_color("#112233");
    session.set_table_borders("Horizontal");
    session.set_diagram_theme("Dark");
    session.set_diagram_scale(1.5);
    session.set_body_font("Libertinus Serif");
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
