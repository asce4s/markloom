//! In-memory Typst world.
//!
//! One [`Source`] stays alive and is updated with [`Source::replace`] so
//! incremental compilation can reuse the previous document. Package imports
//! are rejected. `today()` is unset so compiles do not depend on the clock.

use std::collections::HashMap;
use std::path::PathBuf;

use typst::LibraryExt;
use typst::diag::{FileError, FileResult};
use typst::ecow::EcoString;
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_kit::fonts::{FontStore, embedded};

use crate::codegen::{Assets, DiagramRef};

pub(crate) struct MarkloomWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    source: Source,
    main: FileId,
    files: HashMap<FileId, Bytes>,
}

impl MarkloomWorld {
    pub(crate) fn new(system_fonts: bool) -> Self {
        let mut fonts = FontStore::new();
        fonts.extend(embedded());
        if system_fonts {
            fonts.extend(typst_kit::fonts::system());
        }
        let main = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("main.typ").expect("main.typ is a valid virtual path"),
        )
        .intern();
        Self {
            library: LazyHash::new(Library::builder().build()),
            fonts,
            source: Source::new(main, String::new()),
            main,
            files: HashMap::new(),
        }
    }

    pub(crate) fn font_families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .fonts
            .book()
            .families()
            .map(|(name, _indices)| name.to_string())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub(crate) fn replace_document(&mut self, source: &str, assets: &Assets) {
        self.source.replace(source);
        self.files.clear();
        for (path, diagram) in &assets.diagrams {
            let DiagramRef::Svg(svg) = diagram else {
                continue;
            };
            let Some(id) = project_file(path) else {
                continue;
            };
            self.files
                .insert(id, Bytes::from_string(svg.as_ref().to_string()));
        }
        for (path, disk) in &assets.images {
            let Some(id) = project_file(path) else {
                continue;
            };
            let Ok(bytes) = std::fs::read(disk) else {
                continue;
            };
            self.files.insert(id, Bytes::new(bytes));
        }
    }
}

impl World for MarkloomWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if is_package(id) {
            return Err(packages_disabled());
        }
        if id == self.main {
            return Ok(self.source.clone());
        }
        if self.files.contains_key(&id) {
            return Err(FileError::NotSource);
        }
        Err(missing(id))
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if is_package(id) {
            return Err(packages_disabled());
        }
        if id == self.main {
            return Ok(Bytes::from_string(self.source.text().to_string()));
        }
        self.files.get(&id).cloned().ok_or_else(|| missing(id))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

fn project_file(path: &str) -> Option<FileId> {
    let virtual_path = VirtualPath::new(path).ok()?;
    Some(RootedPath::new(VirtualRoot::Project, virtual_path).intern())
}

fn is_package(id: FileId) -> bool {
    matches!(id.root(), VirtualRoot::Package(_))
}

fn packages_disabled() -> FileError {
    FileError::Other(Some(EcoString::from("packages disabled")))
}

fn missing(id: FileId) -> FileError {
    FileError::NotFound(PathBuf::from(id.vpath().get_without_slash()))
}
