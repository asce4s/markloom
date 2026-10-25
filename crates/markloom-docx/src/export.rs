//! Walk a [`Doc`](markloom_core::Doc) into a docx-rs document.

use std::fs::File;
use std::path::{Path, PathBuf};

use docx_rs::{
    AbstractNumbering, BreakType, Docx, Footer, Header, Hyperlink, HyperlinkType, IndentLevel,
    Level, LevelJc, LevelText, NumberFormat, Numbering, NumberingId, PageMargin, PageNum,
    Paragraph, Pic, Run, RunFonts, Shading, Start, Style, StyleType, Table, TableBorders,
    TableCell, TableCellBorder, TableCellBorderPosition, TableCellBorders, TableRow,
};
use markloom_core::{Block, Doc, Inline, ListItem};
use markloom_mermaid::DiagramService;
use markloom_style::{
    Color, Length, PageNumbers, PaperSize, StyleSheet, TableBorders as Borders, Weight,
};

/// DOCX export failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxError {
    message: String,
}

impl DocxError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DocxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DocxError {}

/// Write `doc` as a styled DOCX. Diagrams are rasterised to PNG.
pub fn export_docx(
    doc: &Doc,
    style: &StyleSheet,
    base_dir: &Path,
    diagrams: &DiagramService,
    dest: &Path,
) -> Result<(), DocxError> {
    let mut body = Vec::new();
    let walk = root_walk(style, base_dir, diagrams);
    for block in &doc.blocks {
        push_block(&mut body, block, walk);
    }
    if !doc.footnotes.is_empty() {
        body.push(Body::Paragraph(Box::new(
            Paragraph::new()
                .style("Heading2")
                .add_run(Run::new().add_text("Footnotes")),
        )));
        for (name, blocks) in &doc.footnotes {
            body.push(Body::Paragraph(Box::new(
                Paragraph::new().add_run(Run::new().add_text(format!("[{name}] "))),
            )));
            for block in blocks {
                push_block(&mut body, block, walk);
            }
        }
    }
    if body.is_empty() {
        body.push(Body::Paragraph(Box::new(Paragraph::new())));
    }

    let (width, height) = page_twips(style.page.size);
    let margin = &style.page.margin;
    let mut docx = styled(Docx::new(), style)
        .add_abstract_numbering(numbering_def(1, "bullet", "•"))
        .add_abstract_numbering(numbering_def(2, "decimal", "%1."))
        .add_numbering(Numbering::new(1, 1))
        .add_numbering(Numbering::new(2, 2))
        .page_size(width, height)
        .page_margin(
            PageMargin::new()
                .top(twips(margin.top))
                .right(twips(margin.right))
                .bottom(twips(margin.bottom))
                .left(twips(margin.left)),
        );
    if !style.page.header.is_empty() {
        docx = docx.header(Header::new().add_paragraph(
            Paragraph::new().add_run(Run::new().add_text(style.page.header.clone())),
        ));
    }
    if !style.page.footer.is_empty() || style.page.page_numbers != PageNumbers::None {
        let mut footer = Paragraph::new();
        if !style.page.footer.is_empty() {
            footer = footer.add_run(Run::new().add_text(style.page.footer.clone()));
        }
        if style.page.page_numbers != PageNumbers::None {
            footer = footer
                .add_run(Run::new().add_text(" "))
                .add_page_num(PageNum::new());
        }
        docx = docx.footer(Footer::new().add_paragraph(footer));
    }
    for item in body {
        docx = match item {
            Body::Paragraph(paragraph) => docx.add_paragraph(*paragraph),
            Body::Table(table) => docx.add_table(*table),
        };
    }
    let file =
        File::create(dest).map_err(|error| DocxError::new(format!("create docx: {error}")))?;
    docx.build()
        .pack(file)
        .map_err(|error| DocxError::new(format!("pack docx: {error}")))?;
    Ok(())
}

enum Body {
    Paragraph(Box<Paragraph>),
    Table(Box<Table>),
}

#[derive(Clone, Copy)]
enum ListKind {
    Bullet,
    Ordered,
}

#[derive(Clone, Copy)]
struct Walk<'a> {
    style: &'a StyleSheet,
    base_dir: &'a Path,
    diagrams: &'a DiagramService,
    depth: usize,
    list: Option<ListKind>,
    quoted: bool,
    prefix: Option<&'static str>,
}

fn root_walk<'a>(
    style: &'a StyleSheet,
    base_dir: &'a Path,
    diagrams: &'a DiagramService,
) -> Walk<'a> {
    Walk {
        style,
        base_dir,
        diagrams,
        depth: 0,
        list: None,
        quoted: false,
        prefix: None,
    }
}

fn push_block(body: &mut Vec<Body>, block: &Block, ctx: Walk<'_>) {
    match block {
        Block::Heading { level, content, .. } => {
            let style_id = format!("Heading{}", level.clamp(&1, &6));
            body.push(Body::Paragraph(Box::new(paragraph(
                content,
                ctx.style,
                ctx.base_dir,
                Some(style_id),
                list_numbering(ctx.list, ctx.depth),
                ctx.prefix,
            ))));
        }
        Block::Paragraph { content, .. } => {
            let style_id = ctx.quoted.then(|| "Quote".to_string());
            body.push(Body::Paragraph(Box::new(paragraph(
                content,
                ctx.style,
                ctx.base_dir,
                style_id,
                list_numbering(ctx.list, ctx.depth),
                ctx.prefix,
            ))));
        }
        Block::Code { code, .. } => {
            let style_name = if ctx.quoted { "Quote" } else { "Code" };
            for (index, line) in code.lines().enumerate() {
                let mut paragraph = Paragraph::new().style(style_name);
                if index == 0 {
                    if let Some((id, level)) = list_numbering(ctx.list, ctx.depth) {
                        paragraph = paragraph.numbering(id, level);
                    }
                    if let Some(prefix) = ctx.prefix {
                        paragraph = paragraph.add_run(Run::new().add_text(prefix));
                    }
                }
                body.push(Body::Paragraph(Box::new(
                    paragraph.add_run(Run::new().add_text(line.to_string())),
                )));
            }
            if code.is_empty() {
                let mut paragraph = Paragraph::new().style(style_name);
                if let Some(prefix) = ctx.prefix {
                    paragraph = paragraph.add_run(Run::new().add_text(prefix));
                }
                body.push(Body::Paragraph(Box::new(paragraph)));
            }
        }
        Block::Diagram { source, .. } => match ctx.diagrams.render(source, &ctx.style.diagram) {
            Ok(svg) => match svg_png(svg.as_ref(), ctx.style.diagram.scale * 2.0) {
                Ok(png) => body.push(Body::Paragraph(Box::new(image_paragraph(&png)))),
                Err(message) => body.push(Body::Paragraph(Box::new(error_box(source, &message)))),
            },
            Err(error) => body.push(Body::Paragraph(Box::new(error_box(
                source,
                &error.to_string(),
            )))),
        },
        Block::Quote { blocks, .. } => {
            for (index, inner) in blocks.iter().enumerate() {
                push_block(
                    body,
                    inner,
                    Walk {
                        quoted: true,
                        list: if index == 0 { ctx.list } else { None },
                        prefix: if index == 0 { ctx.prefix } else { None },
                        ..ctx
                    },
                );
            }
        }
        Block::List { ordered, items, .. } => {
            let kind = if *ordered {
                ListKind::Ordered
            } else {
                ListKind::Bullet
            };
            for item in items {
                push_item(body, item, ctx, kind);
            }
        }
        Block::Table { header, rows, .. } => {
            body.push(Body::Table(Box::new(table(
                header,
                rows,
                ctx.style,
                ctx.base_dir,
            ))));
        }
        Block::Rule { .. } => {
            body.push(Body::Paragraph(Box::new(
                Paragraph::new().add_run(Run::new().add_text("────────")),
            )));
        }
    }
}

fn push_item(body: &mut Vec<Body>, item: &ListItem, ctx: Walk<'_>, kind: ListKind) {
    let prefix = match item.checked {
        Some(true) => Some("[x] "),
        Some(false) => Some("[ ] "),
        None => None,
    };
    if item.blocks.is_empty() {
        body.push(Body::Paragraph(Box::new(paragraph(
            &[],
            ctx.style,
            ctx.base_dir,
            None,
            list_numbering(Some(kind), ctx.depth),
            prefix,
        ))));
        return;
    }
    for (index, block) in item.blocks.iter().enumerate() {
        push_block(
            body,
            block,
            Walk {
                list: if index == 0 { Some(kind) } else { None },
                prefix: if index == 0 { prefix } else { None },
                ..ctx
            },
        );
    }
}

fn list_numbering(list: Option<ListKind>, depth: usize) -> Option<(NumberingId, IndentLevel)> {
    let kind = list?;
    let id = match kind {
        ListKind::Bullet => 1,
        ListKind::Ordered => 2,
    };
    Some((NumberingId::new(id), IndentLevel::new(depth.min(8))))
}

fn paragraph(
    content: &[Inline],
    style: &StyleSheet,
    base_dir: &Path,
    style_id: Option<String>,
    numbering: Option<(NumberingId, IndentLevel)>,
    prefix: Option<&str>,
) -> Paragraph {
    let mut paragraph = Paragraph::new();
    if let Some(style_id) = style_id {
        paragraph = paragraph.style(&style_id);
    }
    if let Some((id, level)) = numbering {
        paragraph = paragraph.numbering(id, level);
    }
    if let Some(prefix) = prefix {
        paragraph = paragraph.add_run(Run::new().add_text(prefix));
    }
    write_inlines(&mut paragraph, content, style, base_dir);
    paragraph
}

fn write_inlines(
    paragraph: &mut Paragraph,
    content: &[Inline],
    style: &StyleSheet,
    base_dir: &Path,
) {
    for inline in content {
        write_inline(paragraph, inline, style, base_dir, Marks::default());
    }
}

#[derive(Clone, Copy, Default)]
struct Marks {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

fn write_inline(
    paragraph: &mut Paragraph,
    inline: &Inline,
    style: &StyleSheet,
    base_dir: &Path,
    marks: Marks,
) {
    match inline {
        Inline::Text(text) => {
            *paragraph = paragraph.clone().add_run(styled_run(text, style, marks))
        }
        Inline::Code(text) => {
            let mut marks = marks;
            marks.code = true;
            *paragraph = paragraph.clone().add_run(styled_run(text, style, marks));
        }
        Inline::Emph(inner) => {
            let mut marks = marks;
            marks.italic = true;
            for inline in inner {
                write_inline(paragraph, inline, style, base_dir, marks);
            }
        }
        Inline::Strong(inner) => {
            let mut marks = marks;
            marks.bold = true;
            for inline in inner {
                write_inline(paragraph, inline, style, base_dir, marks);
            }
        }
        Inline::Strike(inner) => {
            let mut marks = marks;
            marks.strike = true;
            for inline in inner {
                write_inline(paragraph, inline, style, base_dir, marks);
            }
        }
        Inline::Link { url, content } => {
            let mut link = Hyperlink::new(url.clone(), HyperlinkType::External);
            for inline in content {
                if let Inline::Text(text) = inline {
                    link =
                        link.add_run(styled_run(text, style, marks).color(hex(style.colors.link)));
                }
            }
            if content.is_empty() {
                link = link.add_run(styled_run(url, style, marks).color(hex(style.colors.link)));
            }
            *paragraph = paragraph.clone().add_hyperlink(link);
        }
        Inline::Image { url, alt } => match load_image(base_dir, url) {
            Some(bytes) => {
                *paragraph = paragraph
                    .clone()
                    .add_run(Run::new().add_image(picture(&bytes)))
            }
            None => {
                *paragraph = paragraph
                    .clone()
                    .add_run(Run::new().add_text(format!("[placeholder: {alt}]")));
            }
        },
        Inline::SoftBreak => *paragraph = paragraph.clone().add_run(Run::new().add_text(" ")),
        Inline::HardBreak => {
            *paragraph = paragraph
                .clone()
                .add_run(Run::new().add_break(BreakType::TextWrapping));
        }
        Inline::FootnoteRef(name) => {
            *paragraph = paragraph
                .clone()
                .add_run(Run::new().add_text(format!("[{name}]")));
        }
    }
}

fn styled_run(text: &str, style: &StyleSheet, marks: Marks) -> Run {
    let mut run = Run::new().add_text(text.to_string());
    if marks.bold {
        run = run.bold();
    }
    if marks.italic {
        run = run.italic();
    }
    if marks.strike {
        run = run.strike();
    }
    if marks.code {
        run = run
            .fonts(face(&style.typography.code_font))
            .size(half_points(style.code.font_size, style));
    }
    run
}

fn image_paragraph(png: &[u8]) -> Paragraph {
    Paragraph::new().add_run(Run::new().add_image(picture(png)))
}

fn picture(bytes: &[u8]) -> Pic {
    let (width, height) = png_size(bytes).unwrap_or((480, 320));
    Pic::new(bytes).size(px_emu(width), px_emu(height))
}

fn error_box(source: &str, message: &str) -> Paragraph {
    Paragraph::new()
        .style("Code")
        .add_run(Run::new().add_text(format!("diagram error: {message}\n{source}")))
}

fn table(
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    style: &StyleSheet,
    base_dir: &Path,
) -> Table {
    let mut table_rows = Vec::new();
    if !header.is_empty() {
        table_rows.push(row(header, style, base_dir, true, false));
    }
    for (index, cells) in rows.iter().enumerate() {
        let zebra = style.table.zebra.is_some() && index % 2 == 1;
        table_rows.push(row(cells, style, base_dir, false, zebra));
    }
    let mut table = Table::new(table_rows);
    if matches!(style.table.borders, Borders::Grid) {
        table = table.set_borders(TableBorders::new());
    }
    table
}

fn row(
    cells: &[Vec<Inline>],
    style: &StyleSheet,
    base_dir: &Path,
    header: bool,
    zebra: bool,
) -> TableRow {
    let cells = cells
        .iter()
        .map(|content| {
            let mut cell = TableCell::new().set_borders(cell_borders(&style.table, header));
            if header && let Some(color) = style.table.header_bg {
                cell = cell.shading(Shading::new().fill(hex(color)));
            } else if zebra && let Some(color) = style.table.zebra {
                cell = cell.shading(Shading::new().fill(hex(color)));
            }
            let mut paragraph = paragraph(content, style, base_dir, None, None, None);
            if header && style.table.header_bold {
                paragraph = paragraph.add_run(Run::new().bold());
            }
            cell.add_paragraph(paragraph)
        })
        .collect();
    TableRow::new(cells)
}

fn cell_borders(style: &markloom_style::TableStyle, header: bool) -> TableCellBorders {
    let color = hex(style.border_color);
    let size = eighths(style.border_width);
    let edge = |position: TableCellBorderPosition| {
        TableCellBorder::new(position)
            .color(color.clone())
            .size(size)
    };
    match style.borders {
        Borders::None => TableCellBorders::new().clear_all(),
        Borders::Grid => TableCellBorders::new()
            .set(edge(TableCellBorderPosition::Top))
            .set(edge(TableCellBorderPosition::Left))
            .set(edge(TableCellBorderPosition::Bottom))
            .set(edge(TableCellBorderPosition::Right)),
        Borders::Horizontal => TableCellBorders::new()
            .clear_all()
            .set(edge(TableCellBorderPosition::Top))
            .set(edge(TableCellBorderPosition::Bottom)),
        Borders::HeaderOnly if header => TableCellBorders::new()
            .clear_all()
            .set(edge(TableCellBorderPosition::Bottom)),
        Borders::HeaderOnly => TableCellBorders::new().clear_all(),
    }
}

fn styled(mut docx: Docx, style: &StyleSheet) -> Docx {
    let body = face(&style.typography.body_font);
    let heading = face(&style.typography.heading_font);
    let code = face(&style.typography.code_font);
    let base = half_points(style.typography.base_size, style);
    let text = hex(style.colors.text);
    docx = docx
        .add_style(
            Style::new("Normal", StyleType::Paragraph)
                .name("Normal")
                .fonts(body.clone())
                .size(base)
                .color(text.clone()),
        )
        .add_style(
            Style::new("Code", StyleType::Paragraph)
                .name("Code")
                .based_on("Normal")
                .fonts(code)
                .size(half_points(style.code.font_size, style))
                .color(text.clone()),
        )
        .add_style(
            Style::new("Quote", StyleType::Paragraph)
                .name("Quote")
                .based_on("Normal")
                .italic()
                .color(hex(style.blockquote.text_color)),
        );
    for level in 1..=6 {
        let scale = style.typography.heading_scale[level - 1];
        let mut heading_style = Style::new(format!("Heading{level}"), StyleType::Paragraph)
            .name(format!("heading {level}"))
            .based_on("Normal")
            .fonts(heading.clone())
            .size(((base as f32) * scale).round() as usize)
            .color(hex(style.colors.heading));
        if matches!(
            style.typography.heading_weight,
            Weight::Medium | Weight::Semibold | Weight::Bold
        ) {
            heading_style = heading_style.bold();
        }
        docx = docx.add_style(heading_style);
    }
    docx
}

fn numbering_def(id: usize, format: &str, text: &str) -> AbstractNumbering {
    let mut numbering = AbstractNumbering::new(id);
    for level in 0..9 {
        numbering = numbering.add_level(Level::new(
            level,
            Start::new(1),
            NumberFormat::new(format),
            LevelText::new(if format == "decimal" {
                format!("%{}.", level + 1)
            } else {
                text.to_string()
            }),
            LevelJc::new("left"),
        ));
    }
    numbering
}

fn load_image(base_dir: &Path, url: &str) -> Option<Vec<u8>> {
    if url.starts_with("http://") || url.starts_with("https://") || url.is_empty() {
        return None;
    }
    let path = image_path(base_dir, url)?;
    let bytes = std::fs::read(&path).ok()?;
    if path.extension().and_then(|ext| ext.to_str()) == Some("svg") {
        return svg_png(&String::from_utf8_lossy(&bytes), 2.0).ok();
    }
    if is_raster(&path) { Some(bytes) } else { None }
}

fn image_path(base_dir: &Path, url: &str) -> Option<PathBuf> {
    let path = PathBuf::from(url);
    let full = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };
    let ext = full
        .extension()
        .and_then(|ext| ext.to_str())?
        .to_ascii_lowercase();
    if matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg"
    ) {
        Some(full)
    } else {
        None
    }
}

fn is_raster(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp")
    )
}

fn svg_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &options).map_err(|error| error.to_string())?;
    let size = tree.size();
    let width = ((size.width() * scale).ceil() as u32).max(1);
    let height = ((size.height() * scale).ceil() as u32).max(1);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| "diagram pixmap is empty".to_string())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|error| format!("png encode: {error}"))
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    if width == 0 || height == 0 {
        None
    } else {
        Some((width, height))
    }
}

fn face(families: &[String]) -> RunFonts {
    let name = families.first().map(String::as_str).unwrap_or("Calibri");
    RunFonts::new().ascii(name).hi_ansi(name)
}

fn hex(color: Color) -> String {
    format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

fn half_points(length: Length, style: &StyleSheet) -> usize {
    let points = match length {
        Length::Pt(points) => points,
        Length::Em(ems) => match style.typography.base_size {
            Length::Pt(points) => points * ems,
            Length::Em(_) => 11.0 * ems,
        },
    };
    (points * 2.0).round().clamp(2.0, 144.0) as usize
}

fn eighths(length: Length) -> usize {
    let points = match length {
        Length::Pt(points) => points,
        Length::Em(ems) => ems * 11.0,
    };
    (points * 8.0).round().clamp(1.0, 48.0) as usize
}

fn twips(length: Length) -> i32 {
    let points = match length {
        Length::Pt(points) => points,
        Length::Em(ems) => ems * 11.0,
    };
    (points * 20.0).round() as i32
}

fn page_twips(size: PaperSize) -> (u32, u32) {
    match size {
        PaperSize::A4 => (11906, 16838),
        PaperSize::Letter => (12240, 15840),
        PaperSize::A5 => (8391, 11906),
        PaperSize::Legal => (12240, 20160),
    }
}

fn px_emu(px: u32) -> u32 {
    px.saturating_mul(9525)
}
