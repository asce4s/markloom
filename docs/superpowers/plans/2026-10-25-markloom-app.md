# Markloom App and DOCX (Plans 2 and 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open a folder of Markdown in a gpui-kit window, edit it, see the same paged Typst preview the PDF export writes, change the style, and export DOCX with the same stylesheet.

**Architecture:** Headless modules own the workspace, settings, preview policy, style session, and search. `markloom-docx` walks the same `Doc` the Typst codegen walks. `markloom-app` is the `markloom` binary: a Dock with the file tree on the left, the editor in the centre, the preview on the right, and a collapsible style panel on the bottom. One `Engine::compile` result feeds both `render_page` and `write_pdf`.

**Tech Stack:** gpui-kit 0.8.0 (`tree-sitter-markdown`, `tree-sitter-markdown-inline`), docx-rs 0.4.22, resvg 0.45.1, notify 8.2.1, trash 5.2.9. The engine crates stay on the pins from Plan 1.

**Spec:** `docs/superpowers/specs/2026-10-25-markloom-design.md`

## Global Constraints

- Linux first. No macOS or Windows packaging.
- Markdown text never becomes Typst markup. DOCX text goes through docx-rs, which escapes XML.
- PDF export writes the already compiled document. A stale buffer is recompiled first.
- Recompile of a 20-page document stays under about 150 ms. The app debounces edits by 120 ms.
- Source lines are 1-based in Markloom and 0-based in the gpui editor. Convert at the boundary.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` pass after every task.
- Do not invent a gpui-kit API. The spike findings and the installed 0.8.0 source are the authority.

## Review Focus

1. A dirty buffer must not be replaced when the file changes on disk. Test: Task 2 `external_edit_of_dirty_buffer_is_a_conflict`.
2. Delete must go through the trash callback, not `std::fs::remove_file` directly. Test: Task 2 `delete_calls_trash_and_drops_the_path`.
3. Export while the buffer is newer than the compile must recompile before writing. Test: Task 3 `stale_export_needs_recompile`.
4. A missing, remote, or non-image link becomes placeholder text in the DOCX, and the rest of the file is still written. Test: Task 1 `missing_image_is_placeholder_text`.
5. An unknown preset name keeps the previous resolved sheet out of the session and returns the style error. Test: Task 4 `unknown_preset_is_an_error`.

---

### Task 1: DOCX export

**Files:**
- Create: `crates/markloom-docx/src/lib.rs`
- Test: `crates/markloom-docx/tests/docx.rs`

**Interfaces:**
- Consumes: `markloom_core::{parse, Doc}`, `markloom_style::StyleSheet`, `markloom_mermaid::DiagramService`.
- Produces: `export_docx(doc, style, base_dir, diagrams, dest) -> Result<(), DocxError>`.

- [ ] **Step 1: Write the failing test** that unzips `word/document.xml` and `word/styles.xml`.
- [ ] **Step 2: Run** `cargo test -p markloom-docx`. Expected: FAIL, `export_docx` missing.
- [ ] **Step 3: Implement** named styles, lists, tables, images, diagrams as PNG, page size and margins.
- [ ] **Step 4: Run** `cargo test -p markloom-docx`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(docx): styled Doc AST export`.

### Task 2: Workspace, settings, trash, watch reducer

**Files:**
- Create: `crates/markloom-app/src/{lib.rs,settings.rs,workspace.rs}`
- Test: `crates/markloom-app/tests/workspace.rs`

**Interfaces:**
- Produces: `Settings`, `Workspace`, `FileTrash`, `DiskEvent`.

- [ ] **Step 1: Failing tests** for scan, dirty save, rename, trash delete, conflict, settings round-trip.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p markloom-app --lib` and the integration test. Expected: PASS.
- [ ] **Step 5: Commit** `feat(app): workspace, settings, and file watch reducer`.

### Task 3: Preview policy

**Files:**
- Create: `crates/markloom-app/src/preview.rs`
- Test: `crates/markloom-app/tests/preview.rs`

**Interfaces:**
- Produces: `Zoom`, `pixels_per_pt`, `pages_to_rasterize`, `PageCache`, `DEBOUNCE`, `ExportGate`.

- [ ] **Step 1–4:** failing test, fail, implement, pass.
- [ ] **Step 5: Commit** `feat(app): preview zoom, page cache, and stale export`.

### Task 4: Style session, quick-open, search

**Files:**
- Create: `crates/markloom-app/src/{style_session.rs,search.rs}`
- Test: `crates/markloom-app/tests/session.rs`

- [ ] **Step 1–4:** failing test, fail, implement, pass.
- [ ] **Step 5: Commit** `feat(app): style session, quick-open, and search`.

### Task 5: gpui shell

**Files:**
- Create: `crates/markloom-app/src/{main.rs,shell.rs}`

**Interfaces:**
- Consumes: every headless module plus `Engine`, `export_docx`.
- Produces: `cargo run -p markloom-app -- [folder]`.

- [ ] **Step 1: Implement** the Dock, editor, preview, style panel, export, keybindings, theme.
- [ ] **Step 2: Run** `cargo build -p markloom-app` and `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`. Expected: PASS.
- [ ] **Step 3: Commit** `feat(app): gpui shell with preview, style, and export`.
