use markloom_style::{Color, Length, StyleSheet};
use serde::Deserialize;

#[derive(Deserialize)]
struct LengthDoc {
    value: Length,
}

#[derive(Deserialize)]
struct ColorDoc {
    value: Color,
}

fn length(text: &str) -> Result<Length, toml::de::Error> {
    toml::from_str::<LengthDoc>(&format!("value = \"{text}\"")).map(|doc| doc.value)
}

fn color(text: &str) -> Result<Color, toml::de::Error> {
    toml::from_str::<ColorDoc>(&format!("value = \"{text}\"")).map(|doc| doc.value)
}

#[test]
fn length_units() {
    let two_cm = length("2cm").expect("2cm");
    match two_cm {
        Length::Pt(points) => assert!((points - 56.6929).abs() < 0.01, "{points}"),
        Length::Em(em) => panic!("expected pt, got {em}em"),
    }
    assert_eq!(length("1in").expect("1in"), Length::Pt(72.0));
    assert_eq!(length("1.5em").expect("1.5em"), Length::Em(1.5));
    assert!(length("12px").is_err());
}

#[test]
fn color_forms() {
    let white = color("#fff").expect("fff");
    assert_eq!((white.r, white.g, white.b, white.a), (255, 255, 255, 255));
    let translucent = color("#11223380").expect("rgba");
    assert_eq!(translucent.a, 0x80);
    assert_eq!(
        (translucent.r, translucent.g, translucent.b),
        (0x11, 0x22, 0x33)
    );
    assert!(color("red").is_err());
}

#[test]
fn margin_shorthand() {
    let sheet: StyleSheet = toml::from_str("[page]\nmargin = \"1cm\"\n").expect("margin");
    let points = 72.0 / 2.54;
    for side in [
        sheet.page.margin.top,
        sheet.page.margin.right,
        sheet.page.margin.bottom,
        sheet.page.margin.left,
    ] {
        match side {
            Length::Pt(value) => assert!((value - points).abs() < 0.01, "{value}"),
            Length::Em(em) => panic!("expected pt, got {em}em"),
        }
    }
}

#[test]
fn unknown_field_rejected() {
    let error = toml::from_str::<StyleSheet>("[typography]\nbase_sise = \"10pt\"\n")
        .expect_err("typo must fail");
    assert!(error.to_string().contains("base_sise"), "{error}");
}

#[test]
fn default_is_github() {
    let sheet = StyleSheet::default();
    assert_eq!(sheet.page.size, markloom_style::PaperSize::A4);
    assert_eq!(sheet.typography.base_size, Length::Pt(11.0));
    assert_eq!(sheet.typography.line_height, 1.6);
    assert_eq!(
        sheet.colors.link,
        Color {
            r: 0x09,
            g: 0x69,
            b: 0xda,
            a: 255
        }
    );
    assert_eq!(
        sheet.page.page_numbers,
        markloom_style::PageNumbers::BottomCenter
    );
    assert_eq!(sheet.typography.body_font[0], "Inter");
}

#[test]
fn validate_ranges() {
    let mut sheet = StyleSheet::default();
    sheet.typography.line_height = 5.0;
    assert!(sheet.validate().is_err());
}
