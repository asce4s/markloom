use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use markloom_style::{Length, PresetRegistry, StyleError, deep_merge};

fn scratch_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("markloom-{label}-{nanos}"));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn all_builtins_resolve_and_validate() {
    let registry = PresetRegistry::builtin();
    let names = registry.names();
    assert_eq!(
        names,
        vec![
            "github".to_string(),
            "compact".to_string(),
            "academic".to_string(),
            "minimal".to_string(),
            "print-friendly".to_string(),
            "corporate".to_string(),
        ]
    );
    for name in names {
        let sheet = registry.resolve(&name, &[]).expect(&name);
        sheet.validate().expect(&name);
    }
}

#[test]
fn extends_chain() {
    let dir = scratch_dir("extends");
    fs::write(
        dir.join("child.toml"),
        "extends = \"compact\"\n[colors]\nlink = \"#ff0000\"\n",
    )
    .expect("write");
    let mut registry = PresetRegistry::builtin();
    assert!(registry.load_user_dir(&dir).is_empty());
    let sheet = registry.resolve("child", &[]).expect("child");
    assert_eq!(sheet.typography.base_size, Length::Pt(9.5));
    assert_eq!(sheet.colors.link.r, 255);
    assert_eq!(sheet.colors.link.g, 0);
    assert_eq!(sheet.colors.link.b, 0);
}

#[test]
fn cycle_detected() {
    let dir = scratch_dir("cycle");
    fs::write(dir.join("a.toml"), "extends = \"b\"\n").expect("a");
    fs::write(dir.join("b.toml"), "extends = \"a\"\n").expect("b");
    let mut registry = PresetRegistry::builtin();
    assert!(registry.load_user_dir(&dir).is_empty());
    let error = registry.resolve("a", &[]).expect_err("cycle");
    assert!(matches!(error, StyleError::Cycle(_)), "{error}");
}

#[test]
fn overrides_apply_last() {
    let registry = PresetRegistry::builtin();
    let mut typography = toml::Table::new();
    typography.insert("base_size".into(), toml::Value::String("10pt".into()));
    let mut over = toml::Table::new();
    over.insert("typography".into(), toml::Value::Table(typography));
    let sheet = registry.resolve("github", &[over]).expect("override");
    assert_eq!(sheet.typography.base_size, Length::Pt(10.0));
}

#[test]
fn user_shadows_builtin() {
    let dir = scratch_dir("shadow");
    fs::write(
        dir.join("github.toml"),
        "[typography]\nbase_size = \"8pt\"\n",
    )
    .expect("shadow");
    let mut registry = PresetRegistry::builtin();
    assert!(registry.load_user_dir(&dir).is_empty());
    let sheet = registry.resolve("github", &[]).expect("shadowed");
    assert_eq!(sheet.typography.base_size, Length::Pt(8.0));
    assert_eq!(registry.names()[0], "github");
    assert_eq!(
        registry
            .names()
            .iter()
            .filter(|name| *name == "github")
            .count(),
        1
    );
}

#[test]
fn malformed_user_preset_is_reported_not_fatal() {
    let dir = scratch_dir("malformed");
    fs::write(dir.join("good.toml"), "[colors]\nlink = \"#112233\"\n").expect("good");
    fs::write(dir.join("bad.toml"), "{{{ not toml\n").expect("bad");
    let mut registry = PresetRegistry::builtin();
    let errors = registry.load_user_dir(&dir);
    assert_eq!(errors.len(), 1);
    let message = errors[0].to_string();
    assert!(message.contains("bad.toml"), "{message}");
    assert!(registry.resolve("github", &[]).is_ok());
    let good = registry.resolve("good", &[]).expect("good preset");
    assert_eq!(good.colors.link.r, 0x11);
}

#[test]
fn deep_merge_nested() {
    let mut base: toml::Table =
        toml::from_str("[typography]\nbase_size = \"11pt\"\nline_height = 1.6\n").expect("base");
    let over: toml::Table = toml::from_str("[typography]\nbase_size = \"10pt\"\n").expect("over");
    deep_merge(&mut base, &over);
    let typography = base["typography"].as_table().expect("typography");
    assert_eq!(typography["base_size"].as_str(), Some("10pt"));
    assert_eq!(typography["line_height"].as_float(), Some(1.6));
}

#[test]
fn corporate_resolves() {
    let sheet = PresetRegistry::builtin()
        .resolve("corporate", &[])
        .expect("corporate");
    assert_eq!(
        sheet.table.header_bg,
        Some(markloom_style::Color {
            r: 0x1d,
            g: 0x4e,
            b: 0xd8,
            a: 255,
        })
    );
    assert_eq!(
        sheet.table.header_text,
        Some(markloom_style::Color {
            r: 0xff,
            g: 0xff,
            b: 0xff,
            a: 255,
        })
    );
    assert_eq!(
        sheet.table.borders,
        markloom_style::TableBorders::Horizontal
    );
    let left = match sheet.page.margin.left {
        Length::Pt(points) => points,
        Length::Em(em) => panic!("expected pt, got {em}em"),
    };
    let expected = 18.0 * 72.0 / 25.4;
    assert!((left - expected).abs() < 0.01, "{left} vs {expected}");
}

#[test]
fn academic_turns_backgrounds_off() {
    let sheet = PresetRegistry::builtin()
        .resolve("academic", &[])
        .expect("academic");
    assert!(sheet.typography.justify);
    assert!(sheet.page.toc);
    assert_eq!(
        sheet.table.borders,
        markloom_style::TableBorders::Horizontal
    );
    assert_eq!(sheet.table.header_bg, None);
    assert_eq!(sheet.table.zebra, None);
    assert_eq!(sheet.diagram.theme, markloom_style::MermaidTheme::Neutral);
}
