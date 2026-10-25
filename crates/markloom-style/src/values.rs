//! Length and colour literals from style TOML.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

/// A length stored in points, or in ems until a font size resolves it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Length {
    Pt(f32),
    Em(f32),
}

impl Length {
    /// Typst length literal, such as `11pt` or `1.5em`.
    pub fn to_typst(self) -> String {
        match self {
            Length::Pt(points) => format!("{}pt", format_number(points)),
            Length::Em(ems) => format!("{}em", format_number(ems)),
        }
    }
}

impl<'de> Deserialize<'de> for Length {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_length(&text).map_err(de::Error::custom)
    }
}

pub(crate) fn parse_length(text: &str) -> Result<Length, String> {
    let text = text.trim();
    let split = text
        .find(|ch: char| ch.is_ascii_alphabetic())
        .ok_or_else(|| format!("length {text} has no unit"))?;
    let (number, unit) = (text[..split].trim(), &text[split..]);
    let value: f32 = number
        .parse()
        .map_err(|_| format!("invalid length {text}"))?;
    match unit {
        "pt" => Ok(Length::Pt(value)),
        "cm" => Ok(Length::Pt(value * 72.0 / 2.54)),
        "mm" => Ok(Length::Pt(value * 72.0 / 25.4)),
        "in" => Ok(Length::Pt(value * 72.0)),
        "em" => Ok(Length::Em(value)),
        _ => Err(format!("unsupported unit in {text}")),
    }
}

fn format_number(value: f32) -> String {
    if (value - value.round()).abs() < 0.000_1 {
        return format!("{}", value.round() as i64);
    }
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// An sRGB colour. Alpha defaults to opaque when the source omits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Typst colour literal `rgb("#rrggbbaa")`.
    pub fn to_typst(self) -> String {
        format!(
            "rgb(\"#{:02x}{:02x}{:02x}{:02x}\")",
            self.r, self.g, self.b, self.a
        )
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_color(&text).map_err(de::Error::custom)
    }
}

pub(crate) fn parse_color(text: &str) -> Result<Color, String> {
    let hex = text
        .trim()
        .strip_prefix('#')
        .ok_or_else(|| format!("color {text} is not hex"))?;
    let expanded = match hex.len() {
        3 => {
            let mut wide = String::with_capacity(8);
            for ch in hex.chars() {
                wide.push(ch);
                wide.push(ch);
            }
            wide.push_str("ff");
            wide
        }
        6 => format!("{hex}ff"),
        8 => hex.to_string(),
        _ => return Err(format!("color {text} has the wrong length")),
    };
    if expanded.chars().any(|ch| !ch.is_ascii_hexdigit()) {
        return Err(format!("color {text} is not hex"));
    }
    let channel = |start: usize| -> Result<u8, String> {
        u8::from_str_radix(&expanded[start..start + 2], 16)
            .map_err(|_| format!("color {text} is not hex"))
    };
    Ok(Color {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
        a: channel(6)?,
    })
}

/// `none` or a missing value clears an optional colour. A hex string sets it.
pub(crate) fn deserialize_optional_color<'de, D>(deserializer: D) -> Result<Option<Color>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value.as_deref() {
        None | Some("none") | Some("") => Ok(None),
        Some(text) => parse_color(text).map(Some).map_err(de::Error::custom),
    }
}
