//! One `#set` or `#show` rule per style field.

use markloom_style::{
    Color, Length, Orientation, PageNumbers, PaperSize, StyleSheet, TableBorders, Weight,
};

use super::escape::{fmt_number, str_lit};

pub(crate) fn render(style: &StyleSheet) -> String {
    let mut out = String::new();
    out.push_str(&page_rule(style));
    out.push('\n');
    out.push_str(&format!(
        "#set text(font: {}, size: {}, fill: {})\n",
        font_list(&style.typography.body_font),
        style.typography.base_size.to_typst(),
        style.colors.text.to_typst(),
    ));
    let leading = style.typography.line_height - 1.0;
    out.push_str(&format!(
        "#set par(leading: {}em, spacing: {}, justify: {})\n",
        fmt_number(leading),
        style.typography.paragraph_spacing.to_typst(),
        style.typography.justify,
    ));
    out.push_str(&format!(
        "#show heading: set text(font: {}, fill: {}, weight: \"{}\")\n",
        font_list(&style.typography.heading_font),
        style.colors.heading.to_typst(),
        weight_name(style.typography.heading_weight),
    ));
    if style.typography.number_headings {
        out.push_str("#set heading(numbering: \"1.1\")\n");
    }
    for level in 1..=6 {
        let size = scaled(
            style.typography.base_size,
            style.typography.heading_scale[level - 1],
        );
        out.push_str(&format!(
            "#show heading.where(level: {level}): set text(size: {size})\n"
        ));
    }
    if style.page.break_before_h1 {
        out.push_str("#show heading.where(level: 1): it => pagebreak(weak: true) + it\n");
    }
    out.push_str(&format!(
        "#show link: set text(fill: {})\n",
        style.colors.link.to_typst()
    ));
    out.push_str(&format!(
        "#show raw: set text(font: {}, size: {})\n",
        font_list(&style.typography.code_font),
        style.code.font_size.to_typst(),
    ));
    if !style.code.highlight {
        out.push_str("#set raw(theme: none)\n");
    }
    let stroke = match style.code.border {
        Some(color) => format!("stroke: {}", color.to_typst()),
        None => "stroke: none".to_string(),
    };
    out.push_str(&format!(
        "#show raw.where(block: true): it => block(fill: {}, radius: {}, inset: {}, {stroke}, width: 100%, it)\n",
        style.code.background.to_typst(),
        style.code.radius.to_typst(),
        style.code.inset.to_typst(),
    ));
    out.push_str(&table_rule(style));
    out.push('\n');
    if let Some(color) = style.table.header_text {
        out.push_str(&format!(
            "#show table.cell.where(y: 0): set text(fill: {})\n",
            color.to_typst()
        ));
    }
    let quote_style = if style.blockquote.italic {
        "\"italic\""
    } else {
        "\"normal\""
    };
    out.push_str(&format!(
        "#show quote.where(block: true): it => block(stroke: (left: {} + {}), inset: (left: 0.6em, y: 2pt), text(fill: {}, style: {quote_style}, it.body))\n",
        style.blockquote.bar_width.to_typst(),
        style.blockquote.bar_color.to_typst(),
        style.blockquote.text_color.to_typst(),
    ));
    out.push_str(&format!(
        "#set list(indent: {}, marker: [#{bullet}])\n",
        style.lists.indent.to_typst(),
        bullet = str_lit(&style.lists.bullet),
    ));
    out.push_str(&format!(
        "#set enum(indent: {})\n",
        style.lists.indent.to_typst()
    ));
    out
}

fn page_rule(style: &StyleSheet) -> String {
    let margin = &style.page.margin;
    let mut parts = vec![
        format!("paper: \"{}\"", paper_name(style.page.size)),
        format!(
            "margin: (top: {}, right: {}, bottom: {}, left: {})",
            margin.top.to_typst(),
            margin.right.to_typst(),
            margin.bottom.to_typst(),
            margin.left.to_typst(),
        ),
        format!("fill: {}", style.colors.page_bg.to_typst()),
    ];
    let footer_replaces_number = !style.page.footer.is_empty();
    if !footer_replaces_number {
        match style.page.page_numbers {
            PageNumbers::None => {}
            PageNumbers::BottomCenter => {
                parts.push("numbering: \"1\"".into());
                parts.push("number-align: center".into());
            }
            PageNumbers::BottomRight => {
                parts.push("numbering: \"1\"".into());
                parts.push("number-align: right".into());
            }
        }
    }
    if style.page.orientation == Orientation::Landscape {
        parts.push("flipped: true".into());
    }
    if !style.page.header.is_empty() {
        parts.push(format!(
            "header: align(center, text(fill: {}, {}))",
            style.colors.muted.to_typst(),
            str_lit(&style.page.header),
        ));
    }
    if !style.page.footer.is_empty() {
        parts.push(format!(
            "footer: align(center, text(fill: {}, {}))",
            style.colors.muted.to_typst(),
            str_lit(&style.page.footer),
        ));
    }
    format!("#set page({})", parts.join(", "))
}

fn table_rule(style: &StyleSheet) -> String {
    let stroke = format!(
        "{} + {}",
        style.table.border_width.to_typst(),
        style.table.border_color.to_typst()
    );
    let stroke_expr = match style.table.borders {
        TableBorders::Grid => stroke.clone(),
        TableBorders::None => "none".to_string(),
        TableBorders::Horizontal => format!(
            "(x, y) => (top: if y == 0 {{ {stroke} }} else {{ none }}, bottom: {stroke}, left: none, right: none)"
        ),
        TableBorders::HeaderOnly => {
            format!("(x, y) => if y == 0 {{ (bottom: {stroke}) }} else {{ none }}")
        }
    };
    let header = optional_color(style.table.header_bg);
    let zebra = optional_color(style.table.zebra);
    format!(
        "#set table(stroke: {stroke_expr}, inset: {}, fill: (x, y) => if y == 0 {{ {header} }} else if calc.even(y) {{ {zebra} }} else {{ none }})",
        style.table.cell_padding.to_typst(),
    )
}

fn optional_color(color: Option<Color>) -> String {
    match color {
        Some(color) => color.to_typst(),
        None => "none".to_string(),
    }
}

fn font_list(fonts: &[String]) -> String {
    let items: Vec<String> = fonts.iter().map(|font| str_lit(font)).collect();
    format!("({})", items.join(", "))
}

fn paper_name(size: PaperSize) -> &'static str {
    match size {
        PaperSize::A4 => "a4",
        PaperSize::Letter => "us-letter",
        PaperSize::A5 => "a5",
        PaperSize::Legal => "us-legal",
    }
}

fn weight_name(weight: Weight) -> &'static str {
    match weight {
        Weight::Regular => "regular",
        Weight::Medium => "medium",
        Weight::Semibold => "semibold",
        Weight::Bold => "bold",
    }
}

fn scaled(base: Length, scale: f32) -> String {
    match base {
        Length::Pt(points) => format!("{}pt", fmt_number(points * scale)),
        Length::Em(ems) => format!("{}em", fmt_number(ems * scale)),
    }
}
