//! Block and inline emission. Every Markdown string goes through [`str_lit`].

use std::path::{Path, PathBuf};

use markloom_core::{Align, Block, Doc, Inline, ListItem, SourceRange};
use markloom_mermaid::{DiagramError, DiagramService};
use markloom_style::{HAlign, StyleSheet};

use super::CodegenOutput;
use super::escape::str_lit;
use crate::codegen::{Assets, DiagramRef};

pub(crate) struct Emitter<'a> {
    doc: &'a Doc,
    style: &'a StyleSheet,
    base_dir: &'a Path,
    diagrams: &'a DiagramService,
    pub assets: Assets,
    pub blocks: Vec<SourceRange>,
    pub line_to_block: Vec<(u32, usize)>,
    source: String,
    line: u32,
    image_index: usize,
}

impl<'a> Emitter<'a> {
    pub(crate) fn new(
        doc: &'a Doc,
        style: &'a StyleSheet,
        base_dir: &'a Path,
        diagrams: &'a DiagramService,
        preamble: String,
    ) -> Self {
        let line = u32::try_from(preamble.bytes().filter(|byte| *byte == b'\n').count())
            .unwrap_or(u32::MAX)
            .saturating_add(1);
        Self {
            doc,
            style,
            base_dir,
            diagrams,
            assets: Assets::default(),
            blocks: Vec::new(),
            line_to_block: Vec::new(),
            source: preamble,
            line,
            image_index: 0,
        }
    }

    pub(crate) fn finish(self) -> CodegenOutput {
        CodegenOutput {
            source: self.source,
            blocks: self.blocks,
            line_to_block: self.line_to_block,
            assets: self.assets,
        }
    }

    pub(crate) fn emit_document(&mut self) {
        for (index, block) in self.doc.blocks.iter().enumerate() {
            self.line_to_block.push((self.line, index));
            self.blocks.push(block.src());
            self.write(&format!("#metadata({index}) <ml-b>\n"));
            self.emit_block(block);
            self.write("\n\n");
        }
    }

    fn emit_block(&mut self, block: &Block) {
        match block {
            Block::Heading { level, content, .. } => {
                self.write(&format!("#heading(level: {level})["));
                self.emit_inlines(content);
                self.write("]");
            }
            Block::Paragraph { content, .. } => self.emit_inlines(content),
            Block::Code { lang, code, .. } => {
                self.write("#raw(block: true");
                if let Some(lang) = lang {
                    self.write(", lang: ");
                    self.write(&str_lit(lang));
                }
                self.write(", ");
                self.write(&str_lit(code));
                self.write(")");
            }
            Block::Quote { blocks, .. } => {
                self.write("#quote(block: true)[");
                for (index, child) in blocks.iter().enumerate() {
                    if index > 0 {
                        self.write("\n");
                    }
                    self.emit_block(child);
                }
                self.write("]");
            }
            Block::List {
                ordered,
                start,
                items,
                ..
            } => {
                if *ordered {
                    self.write(&format!("#enum(start: {start}"));
                } else {
                    self.write("#list(");
                }
                for (index, item) in items.iter().enumerate() {
                    if index > 0 || *ordered {
                        self.write(", ");
                    }
                    self.emit_item(item);
                }
                self.write(")");
            }
            Block::Table {
                align,
                header,
                rows,
                ..
            } => {
                let columns = header.len().max(rows.first().map(Vec::len).unwrap_or(0));
                self.write(&format!("#table(columns: {columns}, align: ("));
                for index in 0..columns {
                    if index > 0 {
                        self.write(", ");
                    }
                    self.write(align_name(align.get(index).copied().unwrap_or(Align::None)));
                }
                self.write(")");
                if !header.is_empty() {
                    self.write(", table.header(");
                    self.emit_cells(header);
                    self.write(")");
                }
                for row in rows {
                    self.write(", ");
                    self.emit_cells(row);
                }
                self.write(")");
            }
            Block::Rule { .. } => {
                self.write(&format!(
                    "#line(length: 100%, stroke: {})",
                    self.style.table.border_color.to_typst()
                ));
            }
            Block::Diagram { source, .. } => self.emit_diagram(source),
        }
    }

    fn emit_item(&mut self, item: &ListItem) {
        self.write("[");
        if let Some(checked) = item.checked {
            let mark = if checked { "☑ " } else { "☐ " };
            self.write("#");
            self.write(&str_lit(mark));
            self.write(" ");
        }
        for (index, block) in item.blocks.iter().enumerate() {
            if index > 0 {
                self.write("\n");
            }
            self.emit_block(block);
        }
        self.write("]");
    }

    fn emit_cells(&mut self, cells: &[Vec<Inline>]) {
        for (index, cell) in cells.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.write("[");
            self.emit_inlines(cell);
            self.write("]");
        }
    }

    fn emit_inlines(&mut self, inlines: &[Inline]) {
        for inline in inlines {
            self.emit_inline(inline);
        }
    }

    fn emit_inline(&mut self, inline: &Inline) {
        match inline {
            Inline::Text(text) => {
                self.write("#");
                self.write(&str_lit(text));
            }
            Inline::Code(code) => {
                self.write("#raw(");
                self.write(&str_lit(code));
                self.write(")");
            }
            Inline::Emph(inner) => {
                self.write("#emph[");
                self.emit_inlines(inner);
                self.write("]");
            }
            Inline::Strong(inner) => {
                self.write("#strong[");
                self.emit_inlines(inner);
                self.write("]");
            }
            Inline::Strike(inner) => {
                self.write("#strike[");
                self.emit_inlines(inner);
                self.write("]");
            }
            Inline::Link { url, content } => {
                self.write("#link(");
                self.write(&str_lit(url));
                self.write(")[");
                self.emit_inlines(content);
                self.write("]");
            }
            Inline::Image { url, alt } => self.emit_image(url, alt),
            Inline::SoftBreak => self.write("#\" \""),
            Inline::HardBreak => self.write("#linebreak()"),
            Inline::FootnoteRef(label) => self.emit_footnote(label),
        }
    }

    fn emit_footnote(&mut self, label: &str) {
        let Some(blocks) = self.doc.footnotes.get(label) else {
            self.write("#");
            self.write(&str_lit(&format!("[^{label}]")));
            return;
        };
        self.write("#footnote[");
        for (index, block) in blocks.iter().enumerate() {
            if index > 0 {
                self.write("\n");
            }
            self.emit_block(block);
        }
        self.write("]");
    }

    fn emit_image(&mut self, url: &str, alt: &str) {
        if let Some(path) = resolve_image(self.base_dir, url) {
            let index = self.image_index;
            self.image_index += 1;
            let ext = path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("bin");
            let vpath = format!("/i/{index}.{ext}");
            self.write(&format!(
                "#image({}, alt: {})",
                str_lit(&vpath),
                str_lit(alt)
            ));
            self.assets.images.push((vpath, path));
            return;
        }
        let label = if alt.is_empty() { url } else { alt };
        let muted = self.style.colors.muted.to_typst();
        self.write(&format!(
            "#box(stroke: {muted}, inset: 4pt)[#text(fill: {muted})[Image: {}]]",
            content_escape(label)
        ));
    }

    fn emit_diagram(&mut self, source: &str) {
        let hash = fnv64(source);
        let vpath = format!("/d/{hash:016x}.svg");
        match self.diagrams.render(source, &self.style.diagram) {
            Ok(svg) => {
                self.assets
                    .diagrams
                    .push((vpath.clone(), DiagramRef::Svg(svg)));
                let align = align_h(self.style.diagram.align);
                let width = self.style.diagram.max_width * self.style.diagram.scale * 100.0;
                let mut block = format!(
                    "block(image({}, width: {}%))",
                    str_lit(&vpath),
                    trim_pct(width)
                );
                if let Some(fill) = self.style.diagram.background {
                    block = format!(
                        "block(fill: {}, image({}, width: {}%))",
                        fill.to_typst(),
                        str_lit(&vpath),
                        trim_pct(width)
                    );
                }
                if let Some(stroke) = self.style.diagram.border {
                    block = format!("block(stroke: {}, {block})", stroke.to_typst());
                }
                self.write(&format!("#align({align}, {block})"));
            }
            Err(error) => {
                let message = diagram_message(&error);
                self.assets.diagrams.push((
                    vpath,
                    DiagramRef::Error {
                        message: message.clone(),
                    },
                ));
                self.write("#block(stroke: red, inset: 8pt)[#text(fill: red)[Diagram error: #");
                self.write(&str_lit(&message));
                self.write("] #raw(block: true, ");
                self.write(&str_lit(source));
                self.write(")]");
            }
        }
    }

    fn write(&mut self, text: &str) {
        self.line = self.line.saturating_add(
            u32::try_from(text.bytes().filter(|byte| *byte == b'\n').count()).unwrap_or(0),
        );
        self.source.push_str(text);
    }
}

fn resolve_image(base_dir: &Path, url: &str) -> Option<PathBuf> {
    if url.starts_with("http://") || url.starts_with("https://") {
        return None;
    }
    let path = Path::new(url);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };
    let canonical = joined.canonicalize().ok()?;
    let ext = canonical.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp"
    ) {
        return None;
    }
    Some(canonical)
}

fn align_name(align: Align) -> &'static str {
    match align {
        Align::None => "auto",
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
    }
}

fn align_h(align: HAlign) -> &'static str {
    match align {
        HAlign::Left => "left",
        HAlign::Center => "center",
        HAlign::Right => "right",
    }
}

fn content_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('#', "\\#")
}

fn diagram_message(error: &DiagramError) -> String {
    error.to_string()
}

fn fnv64(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn trim_pct(value: f32) -> String {
    super::escape::fmt_number(value)
}

pub(crate) fn emit(
    doc: &Doc,
    style: &StyleSheet,
    base_dir: &Path,
    diagrams: &DiagramService,
    preamble: String,
) -> CodegenOutput {
    let mut emitter = Emitter::new(doc, style, base_dir, diagrams, preamble);
    emitter.emit_document();
    emitter.finish()
}
