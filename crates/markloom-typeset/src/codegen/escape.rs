//! Typst string literals. Markdown text is emitted only through here.

/// A quoted Typst string. Backslash, quotes, newlines, carriage returns and
/// tabs are escaped. The caller adds `#` when the literal should be content.
pub fn str_lit(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

pub(crate) fn fmt_number(value: f32) -> String {
    if (value - value.round()).abs() < 0.000_1 {
        return format!("{}", value.round() as i64);
    }
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}
