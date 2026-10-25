# Markloom — native Rust Markdown editor with exact-match PDF preview, Mermaid, and styled PDF/DOCX export

## Context
`/home/supun/projects/markloom` is empty (not a git repo; Rust 1.91.1 installed). The goal is a Rust desktop app for viewing, editing and managing Markdown files. It needs Mermaid diagrams and a live preview, plus PDF/DOCX export with customisable styling (font, size, colours, table style, diagram style) and built-in presets such as "github" and "compact".

Decisions confirmed with the user:
- **Native GPUI** using `gpui-kit` 0.7 from crates.io. No browser or WebView.
- **Split editor**: source editor with syntax highlighting, plus a scroll-synced live preview.
- **The PDF must match the preview exactly.** The preview is a **paged preview rendered by the same Typst layout engine** that produces the PDF. There is no separate flowing view.
- **Workspace folder management**: file tree, tabs, file operations, quick-open, search and file watching.
- **Personal use, Linux (Arch) first.** Keep the code portable, but no macOS/Windows packaging work yet.
- DOCX matches the styling (fonts, sizes, colours, tables) but not the exact line or page breaks, because Word re-lays out the text itself. This limitation was explained to the user and accepted.

## Core idea: one pipeline from Markdown to Typst to pages
```
Markdown ──comrak──▶ Doc AST ──(+ StyleSheet)──▶ Typst source ──typst compile (incremental)──▶ PagedDocument
                                                                     ├─▶ typst-render → page bitmaps → GPUI preview
                                                                     └─▶ typst-pdf    → PDF export (identical)
Doc AST ──(+ StyleSheet)──▶ docx-rs ──▶ DOCX export (same styling, Word does layout)
```
A single `StyleSheet` (stored as TOML) is the source of truth. Presets are just `StyleSheet` files, and a preset can extend another one. The preview and the PDF are made from the same compiled document object, so they are identical by construction.

## Cargo workspace layout
```
markloom/
  Cargo.toml                 # workspace; pin exact versions of gpui-kit, typst*, mermaid crate
  crates/
    markloom-core/           # comrak parse (GFM, footnotes, front matter, sourcepos) → owned Doc AST
    markloom-style/          # StyleSheet (serde), preset loading/inheritance, validation
      presets/*.toml         # github, compact, academic, minimal, print-friendly (include_str!)
    markloom-mermaid/        # DiagramRenderer trait → SVG; cache by hash(src+theme); resvg → PNG for DOCX
    markloom-typeset/        # Doc+Style → Typst source (with source-map), custom World, compile, render pages, write PDF
    markloom-docx/           # Doc+Style → docx-rs
    markloom-app/            # bin `markloom`: gpui-kit UI
  fixtures/showcase/         # sample workspace exercising every feature
```

### markloom-core
- `comrak` gives the AST with source positions and covers GFM tables, task lists, strikethrough, autolinks, footnotes and YAML front matter. This is converted into an owned `Doc { blocks: Vec<Block> }` so the other crates don't depend on comrak.
- ` ```mermaid ` fences become `Block::Diagram`. Front matter can override style per document, for example `markloom: { preset: compact }`.
- Pure functions; unit-tested.

### markloom-style
- The `StyleSheet` has these sections:
  - `page`: size, margins, header/footer, page numbers, optional table of contents
  - `typography`: body/heading/code fonts, base size, line height, heading scale and weights, paragraph spacing
  - `colors`: text, headings, links, accent, code background, blockquote bar, page background
  - `code`: syntax theme, background, border, font size
  - `table`: border style (none/horizontal/grid), border colour/width, header background/bold, zebra striping, cell padding, alignment
  - `blockquote`, `lists`
  - `diagram`: Mermaid theme (default/neutral/dark/forest/custom colours), scale, max width, alignment, background, border
- Every field is optional in TOML. The resolution order is defaults → `extends` preset → user preset → document front matter.
- Built-in presets are embedded in the binary. User presets live in `~/.config/markloom/presets/*.toml`.

### markloom-mermaid
- Defines a `DiagramRenderer` trait whose main backend is a pure-Rust crate. Candidates are `mermaid-rs-renderer` 0.3.x (13 diagram types, MIT) and `rusty-mermaid` 0.2. Milestone 0 picks one by rendering the fixture corpus.
- Optional `mmdc` (mermaid-cli) fallback when it is on PATH, for the diagram types the crate doesn't support. Otherwise the diagram is shown as a styled error box containing its source.
- Diagram style maps to the crate's theme. Output is SVG, cached by content hash. Typst embeds the vector SVG, so it is identical in the preview and the PDF. DOCX gets a 2× PNG rasterised with resvg.

### markloom-typeset (the shared layout engine)
- **Codegen**: `Doc` plus `StyleSheet` produce Typst source.
  - The style becomes a preamble of `set`/`show` rules covering page, text, headings, `table` (stroke, fill with zebra striping, inset), `raw` (code theme), `quote` and `link`.
  - Blocks become content. All Markdown text is escaped as string literals or `[...]` content, so user text can never inject Typst code. This is covered by fuzz-style tests.
  - Every block is wrapped in a label or position marker that records its Markdown source range, which makes the source map possible.
- **World**: an in-memory `World` holding the main source, a virtual file system (diagram SVGs, local images resolved relative to the document) and fonts. Fonts are system fonts discovered via `typst-kit`, plus a small bundled font set so presets render the same everywhere. The `World` is kept alive between compiles so `comemo` can reuse previous work. No network access: Typst packages are disabled in v1.
- **Compile**: an `Engine::compile(doc, style) -> Result<Compiled, Diagnostics>` call.
  - On error, the last good pages stay on screen and the diagnostics are mapped back to Markdown lines.
  - `Compiled` contains the `PagedDocument` and a source map that converts Markdown line ↔ (page, y).
- **Outputs**:
  - `render_page(i, scale) -> RgbaImage` via `typst-render`, used by the preview;
  - `write_pdf(path)` via `typst-pdf`, which takes the same `PagedDocument` that is on screen;
  - optional PDF metadata (title and author from front matter).
- **Exactness**: the export writes the already-compiled document. If the source is stale, export recompiles first, waits, and writes exactly what the preview then shows.

### markloom-docx
- Uses `docx-rs` 0.4.
- Named paragraph styles for H1–H6, body, code and quote. Tables get borders and shading from `table`, and lists use numbering definitions. Also handles hyperlinks, images, diagrams as PNG, and page size and margins.
- Walks the same `Doc` AST as the Typst codegen, so both handle every node type.

### markloom-app (gpui-kit)
- **Layout**: a gpui-kit Dock with the file tree on the left, editor tabs in the centre, the paged preview on the right and a Style panel that can be collapsed. View modes are editor-only, preview-only and split.
- **Editor**: the gpui-kit code editor with Tree-sitter Markdown highlighting. It tracks the dirty state, Ctrl+S saves, and compile errors are shown as underlines in the editor.
- **Paged preview**:
  - Pages are drawn as GPUI images in a virtualised vertical list with a gap and shadow, like a PDF viewer. It has zoom (fit width, fit page, or a percentage) and HiDPI-aware render scale.
  - Recompiles 120 ms after the last edit, on the background executor. Only visible pages (± 1) are rasterised, and each page bitmap is cached by its frame hash so unchanged pages aren't redrawn.
  - Shows a page counter and a compile-status indicator.
  - **Sync**: moving the editor cursor scrolls the preview to (page, y) through the source map. Clicking the preview jumps the editor to the matching Markdown line, using the source map or, if needed, `typst-ide::jump_from_click` mapped back through the codegen source map.
- **Workspace**:
  - Opening a folder builds the file tree, filtered to `.md`/`.markdown`.
  - File operations: new, rename, delete (moves to trash via the `trash` crate), and move by drag-and-drop.
  - Also provides fuzzy quick-open, workspace search, recent workspaces, and file watching with `notify` (reload, or prompt if the file has unsaved changes).
- **Style panel**: preset dropdown, font pickers (from the font set the `World` sees, so only fonts that will actually render are offered), size sliders, colour pickers, table style options and diagram theme/scale. Changes recompile the preview live. It has "Save as preset" and "Reset".
- **Export**: "Export PDF" writes the current compiled document with no separate settings, because the preview already reflects the page setup. "Export DOCX" uses the same `StyleSheet`.
- **Settings**: `~/.config/markloom/settings.toml` stores the app theme, editor font, default preset, autosave and recent workspaces.

## Milestones (each ends runnable + tested)
0. **Spike** to validate the riskiest parts, with code kept in `spikes/`:
   - a gpui-kit window showing a Typst-rendered page bitmap that updates while typing, with measured compile and render latency;
   - compare the Mermaid crates, embedding the SVG in Typst;
   - a styled table written with `docx-rs`.

   This milestone fixes the exact crate versions and confirms the Rust toolchain. gpui-kit may need a newer toolchain than 1.91, so it may require `rustup update`.
1. **Core and style crates** with presets, plus unit and snapshot tests.
2. **markloom-typeset**: codegen, `World`, compile, render and PDF output, plus the source map.
3. **App shell**: workspace tree, tabs, editor, save and dirty tracking, file watcher, settings.
4. **Paged preview**: live recompile, page cache, zoom, editor ↔ preview sync, error display, Mermaid integration.
5. **Style panel** with custom presets, then Export PDF.
6. **DOCX export**.
7. **Polish**: quick-open, search, keybindings, light/dark app theme, release build.

## Verification
- `cargo test --workspace`:
  - parser and style-resolution tests;
  - snapshot tests of the generated Typst source per preset (`insta`);
  - escaping tests in which hostile Markdown (`#`, `$`, `@`, `<`, backticks, `]`) must compile and print literally;
  - every Mermaid fixture must produce an SVG that `usvg` can parse and Typst can embed;
  - DOCX tests that unzip the file and snapshot `document.xml` and `styles.xml`.
- **Exactness test**: for each preset, compile `fixtures/showcase`, render the preview bitmaps, then write the PDF and rasterise it independently with `pdftoppm` (poppler). Page count and per-page images must match within a small anti-aliasing tolerance.
- Golden images: the first page per preset is compared against stored PNGs, with a tolerance.
- Performance check: on a 20-page fixture, the time from an edit to the updated preview stays under about 150 ms. This is measured in the spike and in a bench.
- Manual end-to-end check with `cargo run -p markloom-app -- fixtures/showcase`:
  - edit a file and confirm the pages update;
  - check cursor and click sync;
  - switch presets and change styles;
  - export a PDF and compare it with the preview in a PDF viewer;
  - export a DOCX and check it opens with `soffice --headless --convert-to pdf`.
- `cargo clippy --workspace -- -D warnings` and `cargo fmt --check`.

## How execution proceeds
After approval:
1. `git init` the project.
2. Save this design as `docs/superpowers/specs/2026-10-25-markloom-design.md`.
3. Write a detailed implementation plan using the writing-plans skill, starting with Milestone 0, and get the user's choice of execution method.
