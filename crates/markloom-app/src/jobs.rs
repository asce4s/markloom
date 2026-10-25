//! Export, font scan, and file search. Each one runs on the background executor.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gpui_kit::{AppContext, Task};
use markloom_core::Doc;
use markloom_mermaid::DiagramService;
use markloom_style::StyleSheet;
use markloom_typeset::{Compiled, Engine, EngineOptions, PdfMeta};

use crate::search::SearchHit;

/// Write an already compiled document. The caller compiles first when the preview is stale.
pub fn export_pdf(
    compiled: Compiled,
    dest: PathBuf,
    meta: PdfMeta,
    cx: &impl AppContext,
) -> Task<Result<PathBuf, String>> {
    cx.background_spawn(async move {
        compiled
            .write_pdf(&dest, &meta)
            .map(|()| dest)
            .map_err(|err| err.to_string())
    })
}

/// Compile the current Markdown, then write the PDF. Both steps stay off the UI thread.
pub fn export_fresh_pdf(
    engine: Arc<Mutex<Engine>>,
    markdown: String,
    style: StyleSheet,
    base: PathBuf,
    dest: PathBuf,
    meta: PdfMeta,
    cx: &impl AppContext,
) -> Task<Result<PathBuf, String>> {
    cx.background_spawn(
        async move { write_fresh_pdf(engine, &markdown, &style, &base, &dest, &meta) },
    )
}

fn write_fresh_pdf(
    engine: Arc<Mutex<Engine>>,
    markdown: &str,
    style: &StyleSheet,
    base: &Path,
    dest: &Path,
    meta: &PdfMeta,
) -> Result<PathBuf, String> {
    let doc = markloom_core::parse(markdown);
    let compiled = compile_locked(engine, &doc, style, base)?;
    compiled
        .write_pdf(dest, meta)
        .map(|()| dest.to_path_buf())
        .map_err(|err| err.to_string())
}

fn compile_locked(
    engine: Arc<Mutex<Engine>>,
    doc: &Doc,
    style: &StyleSheet,
    base: &Path,
) -> Result<Compiled, String> {
    let mut engine = engine.lock().map_err(|err| err.to_string())?;
    engine
        .compile(doc, style, base)
        .map_err(|err| err.to_string())
}

/// Parse Markdown and write a DOCX. Parsing stays on the background executor.
pub fn export_docx(
    markdown: String,
    style: StyleSheet,
    base: PathBuf,
    dest: PathBuf,
    cx: &impl AppContext,
) -> Task<Result<PathBuf, String>> {
    cx.background_spawn(async move {
        let doc = markloom_core::parse(&markdown);
        let diagrams = Arc::new(DiagramService::with_defaults());
        markloom_docx::export_docx(&doc, &style, &base, &diagrams, &dest)
            .map(|()| dest)
            .map_err(|err| err.to_string())
    })
}

/// Scan system fonts and build an engine. The preview stays on "Loading fonts…" until this resolves.
pub fn load_engine(cx: &impl AppContext) -> Task<Engine> {
    cx.background_spawn(async move {
        Engine::new(
            EngineOptions { system_fonts: true },
            Arc::new(DiagramService::with_defaults()),
        )
    })
}

/// Search open buffers plus any workspace files read here, not on the UI thread.
pub fn search(
    query: String,
    open: Vec<(String, String)>,
    root: PathBuf,
    extra: Vec<PathBuf>,
    cx: &impl AppContext,
) -> Task<Vec<SearchHit>> {
    cx.background_spawn(async move {
        let mut files = open;
        for relative in extra {
            let path = root.join(&relative);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            files.push((relative.display().to_string(), text));
        }
        crate::search::search_files(&query, &files)
    })
}
