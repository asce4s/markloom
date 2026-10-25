//! Markdown lines to page positions, and Typst lines back to Markdown.

use markloom_core::SourceRange;
use typst::foundations::{Content, Label, Selector, Value};
use typst::introspection::Introspector;
use typst::utils::PicoStr;
use typst_layout::PagedDocument;

use crate::codegen::CodegenOutput;
use crate::output::abs_pt;

/// Where a Markdown line landed in the paged document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    /// 0-based page index.
    pub page: usize,
    /// Distance from the top of that page, in points.
    pub y_pt: f32,
}

/// Block positions in reading order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceMap {
    entries: Vec<(SourceRange, Location)>,
}

impl SourceMap {
    /// Location of the block that contains `md_line`.
    ///
    /// When the line sits in a gap, this is the last block that starts before it.
    pub fn locate_line(&self, md_line: u32) -> Option<Location> {
        if let Some((_, location)) = self
            .entries
            .iter()
            .find(|(range, _)| range.start_line <= md_line && md_line <= range.end_line)
        {
            return Some(*location);
        }
        self.entries
            .iter()
            .rev()
            .find(|(range, _)| range.start_line < md_line)
            .map(|(_, location)| *location)
    }

    /// Markdown start line of the last block at or above `(page, y_pt)`.
    pub fn line_at(&self, page: usize, y_pt: f32) -> Option<u32> {
        self.entries
            .iter()
            .rev()
            .find(|(_, location)| {
                location.page < page || (location.page == page && location.y_pt <= y_pt)
            })
            .map(|(range, _)| range.start_line)
    }
}

pub(crate) fn from_document(document: &PagedDocument, blocks: &[SourceRange]) -> SourceMap {
    let Some(label) = Label::new(PicoStr::intern("ml-b")) else {
        return SourceMap::default();
    };
    let introspector = document.introspector().as_ref();
    let mut found = Vec::new();
    for element in introspector.query(&Selector::Label(label)) {
        let Some(index) = metadata_index(&element) else {
            continue;
        };
        let Some(range) = blocks.get(index).copied() else {
            continue;
        };
        let Some(location) = element.location() else {
            continue;
        };
        let Some(position) = introspector.position(location) else {
            continue;
        };
        found.push((
            index,
            range,
            Location {
                page: position.page.get().saturating_sub(1),
                y_pt: abs_pt(position.point.y),
            },
        ));
    }
    found.sort_by_key(|(index, _, _)| *index);
    found.dedup_by_key(|(index, _, _)| *index);
    SourceMap {
        entries: found
            .into_iter()
            .map(|(_, range, location)| (range, location))
            .collect(),
    }
}

/// Markdown start line of the block whose generated Typst contains `typst_line`.
///
/// `typst_line` is 1-based, matching [`CodegenOutput::line_to_block`].
pub fn md_line_for_typst_line(codegen: &CodegenOutput, typst_line: u32) -> Option<u32> {
    let (_, index) = codegen
        .line_to_block
        .iter()
        .rev()
        .find(|(line, _)| *line <= typst_line)?;
    codegen.blocks.get(*index).map(|range| range.start_line)
}

fn metadata_index(content: &Content) -> Option<usize> {
    let value = content.field_by_name("value").ok()?;
    match value {
        Value::Int(value) if value >= 0 => usize::try_from(value).ok(),
        _ => None,
    }
}
