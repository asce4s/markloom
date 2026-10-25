# Markloom Mockup Layout and Smooth Editing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the `markloom` window to match the user's mockup (top bar, document-theme bar, editor | exact preview | document settings, status bars, collapsible files sidebar), and remove every measured cause of roughness.

**Architecture:**
- The headless pieces come first: new style fields, codegen, `StyleSession` paths, `DiskChange`, `plan_raster`, text stats and snippets. Each is unit-tested.
- Then the GPUI side is rebuilt as small entities (`Document`, `StyleState`, `PreviewState`) and per-region view files. These replace `shell.rs` and the `DockArea`.
- Page images become `RenderImage`s built off-thread, kept until replaced and then dropped.

**Tech Stack:** Rust 1.99.1, gpui-kit 0.8.0 (`resizable`, `accordion`, `select`, `switch`, `color_picker`, `menu`, `tree`, `Editor`, `notification`), and the Typst 0.15.1 engine crates already in the workspace.

**Spec:** `docs/superpowers/specs/2026-10-25-markloom-layout-design.md` (read it first; it holds the layout drawing, the settings table and the measured problems).

## Global Constraints

- Work on branch `feat/engine`. Commit locally. Do not push.
- Read `.claude/skills/gpui-kit/SKILL.md`, its `references/coding-guides.md` and `.claude/skills/gpui-kit-design-guides/SKILL.md` before any UI task. **Never invent a gpui-kit API**: check the source under `~/.cargo/registry/src/*/gpui-component-0.8.0`, `gpui-base-0.8.0` and `gpui-pre-0.3.9`.
- UI colours come only from `cx.theme()` tokens (no raw hex in view code). Hex values belong only in presets and document styles.
- Markdown text reaches Typst only as string literals. The single intended exception is `custom.typst` (user-authored code), and it is emitted only between `// custom:start` and `// custom:end`.
- None of these may run on the UI thread: Typst compile, page rasterising, PDF/DOCX export, font scan, find-in-files.
- Editor lines are 0-based in gpui and 1-based in Markloom. Convert at the boundary.
- Keystroke-to-updated-preview latency must be at most 150 ms in the **dev** profile on `fixtures/showcase/index.md`.
- Each view file stays under about 400 lines.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` must pass after every task.

## Review Focus

1. **Unsaved edits when opening another folder or closing the window.** A reasonable person expects a "Save changes to X?" prompt (Save / Discard / Cancel), never silent loss. Test: Task 7 `open_folder_with_dirty_buffer_prompts`.
2. **A compile result arriving after a newer edit or preset change.** It must be ignored, and only the newest result may reach the screen. Test: Task 6 `stale_compile_result_is_ignored`.
3. **Very long documents (300+ pages).** Memory stays bounded to the visible pages ±3. Test: Task 5 `large_document_cache_is_bounded`.
4. **A Custom Typst snippet that loops forever or is huge.** It must not hang the app: Typst's loop limit produces a `Custom Typst:` error and the last good pages stay. Test: Task 2 `custom_typst_infinite_loop_is_an_error`.
5. **Opening a non-UTF-8 or unreadable `.md` file.** It produces an error notification, not a panic, and other buffers are unaffected. Test: Task 4 `non_utf8_file_is_an_error`.

---

## File Structure

```
Cargo.toml                                  + [profile.dev] opt-level 1, deps opt-level 3        (Task 1)
crates/markloom-style/src/sheet.rs          + Orientation, ImageStyle, CustomStyle, new fields  (Task 1)
crates/markloom-style/presets/corporate.toml  new preset                                         (Task 1)
crates/markloom-typeset/src/codegen/{preamble,blocks,mod}.rs  new field output + custom region   (Task 2)
crates/markloom-typeset/src/engine.rs       Custom Typst diagnostic prefix                       (Task 2)
crates/markloom-app/src/style_session.rs    set/clear/clear_all by dotted path                   (Task 3)
crates/markloom-app/src/workspace.rs        DiskChange, Workspace::open_path                     (Task 4)
crates/markloom-app/src/text.rs             TextStats, insert snippets                           (Task 4)
crates/markloom-app/src/preview_pipeline.rs RasterPlan, zoom steps, to_bgra                      (Task 5)
crates/markloom-app/src/{document,style_state,preview_state}.rs  GPUI entities                   (Task 6)
crates/markloom-app/src/app.rs              bootstrap, actions, AppRoot layout                   (Task 7)
crates/markloom-app/src/views/{mod,top_bar,sub_bar,files,editor,preview,status}.rs               (Task 7)
crates/markloom-app/src/views/settings.rs   document settings panel                              (Task 8)
crates/markloom-app/src/jobs.rs             background export, font scan, search                 (Task 9)
crates/markloom-app/src/shell.rs            deleted                                              (Task 7)
```

---

### Task 1: Dev build profile, new style fields, corporate preset

**Files:**
- Modify: `Cargo.toml`, `crates/markloom-style/src/sheet.rs`, `crates/markloom-style/src/registry.rs` (builtin list)
- Create: `crates/markloom-style/presets/corporate.toml`
- Test: `crates/markloom-style/tests/values.rs`, `tests/registry.rs`

**Interfaces:**
- Produces, all with serde defaults:
  - `pub enum Orientation { #[default] Portrait, Landscape }` (serde lowercase)
  - `PageStyle.orientation: Orientation`
  - `PageStyle.toc_depth: u8` (default 3)
  - `PageStyle.break_before_h1: bool`
  - `Typography.number_headings: bool`
  - `TableStyle.header_text: Option<Color>`
  - `pub struct ImageStyle { pub max_width: f32 /*1.0*/, pub align: HAlign /*Center*/ }`
  - `pub struct CustomStyle { pub typst: String /*""*/ }`
  - `StyleSheet.images: ImageStyle`, `StyleSheet.custom: CustomStyle`
  - `validate()` adds: `toc_depth` in 1–6, `images.max_width` in 0.1–1.0
  - `names()` order: github, compact, academic, minimal, print-friendly, corporate
- Corporate preset values come from the spec's "New preset `corporate`" bullet.

- [ ] **Step 1: Write the failing tests:**
  - `new_field_defaults`: `orientation==Portrait`, `toc_depth==3`, `!break_before_h1`, `!number_headings`, `header_text==None`, `images.max_width==1.0`, `images.align==Center`, `custom.typst==""`.
  - `orientation_parses`: `page.orientation="landscape"` gives `Landscape`.
  - `toc_depth_range`: 0 and 7 give `Err`.
  - `image_width_range`: 0.05 gives `Err`.
  - `corporate_resolves`: `header_bg==#1d4ed8`, `header_text==Some(#ffffff)`, `borders==Horizontal`, `margin.left==18mm`.
  - Extend `all_builtins_resolve_and_validate` to expect 6 names.
- [ ] **Step 2: Run** `cargo test -p markloom-style`. Expected: FAIL (unknown fields/types).
- [ ] **Step 3: Implement** the fields, the validation, the preset, and the profile block from the spec's "Build profile" section in the root `Cargo.toml`.
- [ ] **Step 4: Run** `cargo test -p markloom-style`. Expected: PASS. Then run `cargo build -p markloom-app`. Expected: builds with the new profile.
- [ ] **Step 5: Commit** with the message `feat(style): orientation, toc depth, numbering, images, custom typst, corporate preset; optimise dev deps`.

---

### Task 2: Codegen for the new fields and the Custom Typst region

**Files:**
- Modify: `crates/markloom-typeset/src/codegen/{mod.rs, preamble.rs, blocks.rs}`, `src/engine.rs`
- Test: `crates/markloom-typeset/tests/codegen.rs` (+ snapshots), `tests/engine.rs`

**Interfaces:**
- Consumes: Task 1 fields.
- Produces:
  - `CodegenOutput.custom_lines: Option<std::ops::Range<u32>>`: the 1-based Typst lines between the custom markers, exclusive of the markers.
  - `Diag.message` starts with `"Custom Typst: "` when the diagnostic's Typst line is in `custom_lines`. Its `md_line` is `None`.
  - `Compiled.diagram_count: usize`: the number of `Block::Diagram`s in the document, including error boxes. It feeds "Preview ready · N diagrams".
  - `Compiled::render_page_premultiplied(&self, index: usize, pixels_per_pt: f32) -> PageBitmap`: the same as `render_page`, but `rgba` holds tiny-skia's raw premultiplied data, with no `unpremultiply`.
- Output follows the codegen table in the spec's "Style model changes" section.
  - Order: generated preamble, then the custom block, then `#outline(depth: n)` (if `toc`), then the content.
  - `header_text` is emitted only when it is `Some`.
  - The images rule applies to Markdown images only, not diagrams.

- [ ] **Step 1: Write the failing tests:**
  - Snapshots `snapshot_preamble_orientation_landscape`, `…_toc_depth_2`, `…_number_headings`, `…_break_before_h1`, `…_header_text`, and `…_custom_typst` (custom = `#set text(tracking: 1pt)`).
  - `image_uses_images_style`: `max_width=0.5`, `align=right` produces `#align(right, image(` and `width: 50%`.
  - `custom_lines_cover_custom_block`: the source lines in `custom_lines` join to exactly the custom text.
  - `landscape_page_is_wide` (engine): `page_size_pt(0).0 > .1`.
  - `custom_typst_error_is_prefixed` (engine): custom `#let x = ` gives `CompileFailure` whose first message starts with `Custom Typst: `.
  - `custom_typst_infinite_loop_is_an_error` (Review Focus 4): custom `#while true {}` returns `Err` within 10 s (assert elapsed under 10 s) with the `Custom Typst: ` prefix.
- [ ] **Step 2: Run** `cargo test -p markloom-typeset`. Expected: FAIL.
- [ ] **Step 3: Implement.** In `engine.rs::diag_from`, check `custom_lines` before calling `md_line_for_typst_line`.
- [ ] **Step 4: Run** `cargo insta review`, accept after reading, then `cargo test -p markloom-typeset`. Expected: PASS, and the existing exactness tests still pass.
- [ ] **Step 5: Commit** with the message `feat(typeset): page orientation, toc depth, numbering, images and custom typst output`.

---

### Task 3: StyleSession path API

**Files:**
- Modify: `crates/markloom-app/src/style_session.rs`
- Test: `crates/markloom-app/tests/session.rs`

**Interfaces:**
- Produces:
  - `pub fn set(&mut self, path: &str, value: toml::Value)`: a dotted path, with numeric segments for array indices (`"typography.heading_scale.0"`, `"typography.body_font.0"`). Missing tables are created. Setting index 0 of a missing array creates a one-element array, filled from the current resolved value by the caller where needed.
  - `pub fn clear(&mut self, prefix: &str)`: removes the override at `prefix` and every override under it, and prunes empty tables.
  - `pub fn clear_all(&mut self)`.
  - `pub fn is_overridden(&self, prefix: &str) -> bool`.
  - Remove the per-field setters (`set_base_size_pt`, `set_text_color`, …) and update their callers in `shell.rs` to `set`. `shell.rs` keeps compiling until Task 7 deletes it.

- [ ] **Step 1: Write the failing tests:**
  - `set_nested_path`: `set("page.margin.top", "20mm")` resolves to `margin.top==20mm`.
  - `set_array_index`: `set("typography.heading_scale.0", 2.4)` with the other entries taken from the preset gives `heading_scale[0]==2.4`.
  - `clear_prefix`: setting `table.zebra` and `table.border_color` then `clear("table")` leaves `!is_overridden("table")` and `overrides()` empty.
  - `clear_all`.
  - `bad_value_reports_error`: `set("page.toc_depth", 9)` makes `resolve` return `Err`, and after `clear("page.toc_depth")` it resolves.
- [ ] **Step 2: Run** `cargo test -p markloom-app --test session`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p markloom-app`. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(app): style overrides by dotted path`.

---

### Task 4: DiskChange, open-by-path, text stats, insert snippets

**Files:**
- Modify: `crates/markloom-app/src/workspace.rs`
- Create: `crates/markloom-app/src/text.rs`
- Test: `crates/markloom-app/tests/workspace.rs`, `tests/text.rs`

**Interfaces:**
- Produces:
  - `pub enum DiskChange { None, Reloaded, Conflict, Removed, Rescanned }`
  - `Workspace::apply_disk(&mut self, event: DiskEvent) -> Result<DiskChange, String>`. A `Modified` event whose content equals `buffer.disk` returns `None`, with no state change.
  - `Workspace::open_path(path: &Path) -> Result<(Workspace, Option<PathBuf /*relative*/>), String>`: a folder opens as a workspace; a file opens its parent folder and returns the file's relative path.
  - `Workspace::has_unsaved(&self) -> Vec<PathBuf>`.
  - `Workspace::open_file` returns `Err("{path} is not UTF-8 text")` for invalid UTF-8.
  - `text::TextStats { pub words: usize, pub lines: usize }`
  - `text::stats(text: &str) -> TextStats`: words are whitespace-separated tokens; lines count `\n` + 1, and an empty text has 1 line.
  - `pub enum Snippet { Table, CodeBlock, Mermaid, Image, Quote, Rule }`
  - `text::snippet(kind: Snippet) -> (String /*text*/, std::ops::Range<usize> /*placeholder byte range*/)`. Each snippet starts and ends with a blank line. Texts:
    - Table: `| Column | Column |\n|---|---|\n| Cell | Cell |\n| Cell | Cell |`, placeholder `Column`
    - CodeBlock: ```` ```text\ncode\n``` ````, placeholder `code`
    - Mermaid: ```` ```mermaid\nflowchart LR\n  A[Start] --> B[End]\n``` ````, placeholder `Start`
    - Image: `![alt text](path/to/image.png)`, placeholder `alt text`
    - Quote: `> Quote`, placeholder `Quote`
    - Rule: `---`, empty placeholder

- [ ] **Step 1: Write the failing tests:**
  - `own_save_is_not_a_change`: edit, `save_active`, then `apply_disk(Modified)` gives `DiskChange::None`, and the buffer is not a conflict.
  - Update `external_edit_of_clean_buffer_reloads` to expect `Reloaded`, and `…dirty_buffer_is_a_conflict` to expect `Conflict`.
  - `open_path_file_selects_it`.
  - `has_unsaved_lists_dirty`.
  - `non_utf8_file_is_an_error` (Review Focus 5): write bytes `[0xff, 0xfe, 0x00]` to `bad.md`; `open_file` returns an `Err` containing `not UTF-8`, and the active buffer is unchanged.
  - `stats_counts`: `"a b\n\nc"` gives words 3, lines 3; `""` gives 0, 1.
  - `snippets_placeholders`: for each `Snippet`, `&text[range]` equals the documented placeholder.
- [ ] **Step 2: Run** `cargo test -p markloom-app`. Expected: FAIL.
- [ ] **Step 3: Implement.** Update `shell.rs::drain_watch` to set `pending_sync` only on `Reloaded`, which fixes the save reset immediately.
- [ ] **Step 4: Run** `cargo test -p markloom-app`. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(app): ignore own saves, open by path, text stats and snippets`.

---

### Task 5: Headless preview pipeline

**Files:**
- Create: `crates/markloom-app/src/preview_pipeline.rs` (it absorbs `preview.rs`'s `Zoom`, `pixels_per_pt`, `visible_span` and `DEBOUNCE`; delete `preview.rs` and `PageCache`, and update imports)
- Test: `crates/markloom-app/tests/preview.rs` (rewrite)

**Interfaces:**
- Produces:
  - `pub const ZOOM_STEPS: [f32; 9] = [50., 67., 75., 90., 100., 110., 125., 150., 200.]`
  - `pub fn zoom_in(z: Zoom, fit_percent: f32) -> Zoom` and `zoom_out`, which step from the current effective percent to the next step, clamped at the ends.
  - `pub struct PageKey { pub index: usize, pub hash: u128, pub scale_bits: u32 }`
  - `pub struct RasterPlan { pub draw: Vec<PageKey>, pub drop: Vec<PageKey>, pub show: Vec<(usize, PageKey)> }`
  - `pub fn plan_raster(held: &[PageKey], hashes: &[u128], visible: (usize, usize), scale_bits: u32) -> RasterPlan`
  - `pub fn to_bgra(rgba_premultiplied: &[u8]) -> Vec<u8>`: swaps R and B and stays premultiplied (GPUI `RenderImage` expects premultiplied BGRA). Its input comes from Task 2's `Compiled::render_page_premultiplied`.
- Rules for `plan_raster`:
  1. Let `want` be the visible pages ±1 (clamped to the page count). Each `i` in `want` needs `PageKey{i, hashes[i], scale_bits}`.
  2. `draw` lists each needed key that is not held.
  3. `show[i]` is the exact key if it is held. Otherwise it is any held key for page `i`, preferring the one with the same hash (scaled while waiting). Pages with nothing held are omitted.
  4. `drop` lists held keys for pages outside the visible pages ±3, plus held keys for page `i` that are superseded. A key is superseded only once the exact needed key for `i` is held. Never drop the only image shown for a page.

- [ ] **Step 1: Write the failing tests:**
  - `zoom_steps`: 100 in gives 110; 200 in gives 200; 50 out gives 50; FitWidth at an effective 83 % gives 90 in and 75 out.
  - `first_paint_draws_visible_plus_one`.
  - `edit_keeps_old_until_new` (page 2's hash changes): draw contains page 2, show[2] is the old key, drop is empty.
  - `new_image_drops_old`: holding both old and new keys for page 2 drops the old.
  - `scale_change_shows_old_scaled`.
  - `scroll_drops_far_pages`.
  - `large_document_cache_is_bounded` (Review Focus 3): 400 pages, visible (200,201), holding keys for pages 0..400 gives held − dropped ≤ 8.
  - `bgra_swaps_channels`: `[1,2,3,4]` gives `[3,2,1,4]`.
- [ ] **Step 2: Run** `cargo test -p markloom-app --test preview`. Expected: FAIL.
- [ ] **Step 3: Implement.** `shell.rs` still compiles: keep the PNG path there untouched until Task 6.
- [ ] **Step 4: Run** `cargo test -p markloom-app`. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(app): pure raster plan, zoom steps and BGRA conversion`.

---

### Task 6: GPUI entities: Document, StyleState, PreviewState

**Files:**
- Create: `crates/markloom-app/src/{document.rs, style_state.rs, preview_state.rs}`
- Test: `crates/markloom-app/tests/entities.rs` (`#[gpui_kit::test]`; read `.claude/skills/gpui-kit/references/gpui/test.md` first)

**Interfaces:**
- Consumes: Tasks 3–5.
- Produces:
  - `Document` (Entity):
    - owns `Workspace`, the editor `Entity<EditorState>` (configured with `line_number(true)`, `folding(true)`, `soft_wrap(true)`, `searchable(true)` and the markdown highlighter), the watcher, `stats: TextStats`, `cursor: (u32 line, u32 col)` (1-based) and `generation: u64`;
    - events `DocumentEvent::{TextChanged, CursorLineChanged(u32), Saved, Reloaded, Conflict, Error(String)}`;
    - methods `save`, `open_path`, `select_buffer(usize)` and `insert_snippet(Snippet)`. `insert_snippet` inserts at the cursor and selects the placeholder range;
    - `CursorLineChanged` is emitted only when the line differs from the last emitted one, which keeps the blink fix out of view code.
  - `StyleState` (Entity): owns `PresetRegistry`, `StyleSession` and a cached `Arc<StyleSheet>` (re-resolved only in `set`/`clear`/`clear_all`/`set_preset`). It emits `StyleEvent::Changed`, and on a resolve error it keeps the last good sheet and emits `StyleEvent::Error(String)`.
  - `PreviewState` (Entity):
    - owns `Option<Arc<Mutex<Engine>>>` (None until fonts load, see Task 9), `compiled: Option<Compiled>`, `held: HashMap<PageKey, Arc<RenderImage>>`, `zoom`, `sync: bool`, `visible`, `status: PreviewStatus::{LoadingFonts, Compiling, Ready{pages, diagrams}, Error(String)}` and `generation`;
    - subscribes to `Document::TextChanged` and `StyleEvent::Changed` and schedules a compile after `DEBOUNCE` on `background_spawn`;
    - applies a result only when its generation equals the current one;
    - executes `plan_raster`: drawing each page in its own `background_spawn`, building `RenderImage::new([Frame::new(image::ImageBuffer<Bgra..>)])` off-thread, and calling `cx.drop_image(old, None)` for `plan.drop` in the same update that installs new images;
    - `pub fn page_image(&self, index) -> Option<Arc<RenderImage>>` (from `plan.show`);
    - `pub fn scroll_target(&self, line: u32) -> Option<usize>` (source map).
  - `Document` text changes never trigger a `StyleState` re-resolve.

- [ ] **Step 1: Write the failing tests:**
  - `typing_compiles_once_after_debounce`: 5 rapid `set_value` edits and then advancing the clock 200 ms give a compile count of 1.
  - `stale_compile_result_is_ignored` (Review Focus 2): start a compile, edit before it finishes, and the final `compiled` matches the latest text (check `page_count` or status for a text whose page count differs).
  - `cursor_blink_does_not_emit`: advancing the clock 2 s with no cursor movement gives 0 `CursorLineChanged`.
  - `save_keeps_cursor`: put the cursor at (5,3), `save()`, drain watcher events, and the cursor is still (5,3) with no `Reloaded`.
  - `paper_size_change_recompiles_once`: `style.set("page.size","letter")` gives one compile and a page width of 612 pt.
  - `replaced_images_are_dropped`: after edit → raster → edit → raster, `held.len() <= visible+2` and the first image's id is no longer held.
- [ ] **Step 2: Run** `cargo test -p markloom-app --test entities`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Tests construct the engine with `system_fonts: false` through `PreviewState::with_engine(engine)`.
  - Add the `image` crate at exactly the version `gpui-pre 0.3.9` resolves to in `Cargo.lock`, so `Frame` and `ImageBuffer` types match.
  - Confirm the `RenderImage` / `Frame` constructors in `gpui-pre-0.3.9/src/assets.rs`.
- [ ] **Step 4: Run** `cargo test -p markloom-app`. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(app): document, style and preview entities with retained RenderImages`.

---

### Task 7: AppRoot layout and views (replaces shell.rs)

**Files:**
- Create: `crates/markloom-app/src/app.rs`, `src/views/{mod.rs, top_bar.rs, sub_bar.rs, files.rs, editor.rs, preview.rs, status.rs}`
- Modify: `src/main.rs` (calls `app::run()`), `src/lib.rs`, `src/settings.rs` (+ `files_open: bool` (false), `settings_open: bool` (true), `sync_scroll: bool` (true), `column_widths: Option<[f32; 4]>`, theme default `Dark`)
- Delete: `src/shell.rs`
- Test: `crates/markloom-app/tests/layout.rs` (`#[gpui_kit::test]`)

**Interfaces:**
- Consumes: the Task 6 entities.
- Produces:
  - Actions bound in `app.rs`:
    - `ToggleFiles` (ctrl-b)
    - `ToggleSettings` (ctrl-,)
    - `QuickOpen` (ctrl-p)
    - `FindInFiles` (ctrl-shift-f)
    - `SaveFile` (ctrl-s)
    - `ExportPdf` (ctrl-e)
    - `ExportDocx` (ctrl-shift-e)
    - `ZoomIn` (ctrl-=)
    - `ZoomOut` (ctrl--)
    - `OpenPath` (ctrl-o)
  - `AppRoot::render` follows the spec drawing: `top_bar`, then `sub_bar`, then `h_resizable("columns")` with panels files (conditional), editor, preview and settings (conditional; Task 8 fills it, and here it is an empty placeholder with the header).
  - Each column's status bar sits at the bottom of that column.
  - Copy is exact from the spec: "MARKDOWN", "+ Insert", "DOCUMENT SETTINGS", "Document theme", "Settings", "Export PDF", "Open", "Saved", "Edited", "Changed on disk", the `{words} words · {lines} lines` pattern, and so on.
  - Preview view:
    - a canvas using `cx.theme().muted` as the background;
    - pages centred with a `shadow_md`, a 24 px gap and `uniform_list`;
    - `img(ImageSource::Render(image))` sized to the page's current display size, so scaled old images are drawn at the new size;
    - on scroll, `PreviewState::set_visible`;
    - click → `Document` jumps to the line;
    - on `CursorLineChanged` while `sync` is on, `scroll_to_item(page, ScrollStrategy::Nearest)`, and never otherwise.
  - Window close and `OpenPath` with `has_unsaved()` non-empty show an alert dialog: "Save changes to {n} file(s)?" with Save / Discard / Cancel buttons.
  - If the watcher fails to start, the files sidebar shows muted "File watching unavailable" text. Column widths are saved to `settings.column_widths` on resize.

- [ ] **Step 1: Write the failing tests:**
  - `default_layout_hides_files`: the files panel is not rendered and the settings panel is.
  - `ctrl_b_toggles_files`.
  - `insert_table_at_cursor`: Insert → Table puts `| Column |` at the cursor, and the selection equals `Column`.
  - `status_shows_word_count`: text "a b c" gives the status text `3 words · 1 lines`.
  - `open_folder_with_dirty_buffer_prompts` (Review Focus 1): dirty buffer + `OpenPath` gives a dialog open with the root unchanged; "Cancel" keeps the buffer.
  - `preview_scroll_not_overridden`: scroll the preview to page 5, advance the clock 2 s, and the scroll offset is unchanged.
- [ ] **Step 2: Run** `cargo test -p markloom-app --test layout`. Expected: FAIL.
- [ ] **Step 3: Implement** the views, following the design guides (tokens, `Button` ghost/outline for quiet actions, `Escape` closes overlays). Delete `shell.rs`.
- [ ] **Step 4: Run** `cargo test -p markloom-app`. Expected: PASS. Then `cargo run -p markloom-app -- fixtures/showcase` and compare against the mockup by eye.
- [ ] **Step 5: Commit** with the message `feat(app): mockup layout with collapsible files and per-column status`.

---

### Task 8: Document settings panel

**Files:**
- Create: `crates/markloom-app/src/views/settings.rs` (split it into `views/settings/{mod.rs, page.rs, text.rs, blocks.rs}` if it passes 400 lines)
- Test: `crates/markloom-app/tests/settings_panel.rs`

**Interfaces:**
- Consumes: `StyleState::{set, clear, clear_all, sheet()}`, `Engine::font_families()` (via `PreviewState`).
- Produces: the sections and the path mapping exactly as in the spec's settings table.
  - Controls:
    - numbers → `NumberInput`
    - choices → `Select`
    - booleans → `Switch`
    - colours → `ColorPicker` plus a hex field
    - fonts → `Select` over the font families
    - Custom Typst → a multi-line `Editor` (no highlighter), applied 400 ms after the last keystroke
  - Margin inputs are in mm and written as `"{v}mm"`.
  - Each section's "Reset to preset" calls `clear` on that section's prefixes. The header ↺ calls `clear_all`.
  - Controls show the **resolved** value (so a preset's value is visible).
  - A control whose path `is_overridden` shows a small accent dot.
  - "Save style as preset…" (⋯ menu, wired in `top_bar.rs`) opens a dialog with a name input.
    - It writes `StyleSession::save_preset(config_dir/presets, name, &sheet)` with `extends` set to the current preset, reloads the registry, and selects the new preset.
    - Names that clash with a built-in show "Choose a name that isn't a built-in preset".

- [ ] **Step 1: Write the failing tests:**
  - `margin_input_writes_mm`: entering 25 in Top gives the override `page.margin.top == "25mm"`.
  - `orientation_select_updates_status`: Landscape gives the preview status `A4 · Landscape`.
  - `section_reset_clears_only_section`: tables overrides plus a page override, then Tables → Reset, leaves only the page override.
  - `header_reset_clears_all`.
  - `custom_typst_error_shows_in_status`: entering `#let x =` gives a preview status starting `Custom Typst:`, with the pages still present.
  - `save_as_preset_selects_it`: saving "mine" with a margin override gives `names()` containing `mine`, the selected preset `mine`, and no overrides left, with the same resolved margin.
  - `builtin_name_rejected`: saving as "github" shows the error copy and writes no file.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p markloom-app`. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(app): document settings panel`.

---

### Task 9: Background jobs, performance check, final verification

**Files:**
- Create: `crates/markloom-app/src/jobs.rs`
- Modify: `views/top_bar.rs`, `app.rs`, `preview_state.rs`
- Create: `crates/markloom-app/examples/keystroke_latency.rs`
- Test: `crates/markloom-app/tests/jobs.rs`

**Interfaces:**
- Produces:
  - `jobs::export_pdf(compiled_or_recompile, dest, meta) -> Task<Result<PathBuf, String>>` and `jobs::export_docx(...)`, both on `background_spawn`. The outcome is shown as a gpui-kit notification (`window.push_notification`): "Exported {file name}" or the error.
  - `jobs::load_engine() -> Task<Engine>`: font scan off-thread. `PreviewState` stays `LoadingFonts` until it resolves, then compiles.
  - `jobs::search(query, buffers_snapshot, root) -> Task<Vec<SearchHit>>`: debounced 150 ms, keyed by query. Results are cached in `AppRoot` until the query or the workspace changes.
  - The quick-open ranking is computed only while the palette is open.
  - The example `keystroke_latency` builds the dev-profile pipeline headlessly: `Engine` with system fonts, the showcase, then 11 one-character edits measuring compile + `plan_raster` + draw of 3 pages + `to_bgra`. It prints the median and exits non-zero if it is ≥ 150 ms.

- [ ] **Step 1: Write the failing tests:**
  - `export_runs_off_thread`: start `export_pdf` and assert the UI-thread update loop keeps running (advance the clock and the task is still pending, then completes); the file exists with the `%PDF-` prefix.
  - `fonts_load_then_compile`: the status goes `LoadingFonts` → `Ready`.
  - `search_is_cached`: the same query twice runs the search job once.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify:**
  - `cargo test --workspace`. Expected: PASS.
  - `cargo run -p markloom-app --example keystroke_latency` (**dev profile**). Expected: median ≤ 150 ms, exit 0.
  - Clippy and fmt pass.
  - Manually, run `cargo run -p markloom-app -- fixtures/showcase` and check:
    - typing never flashes pages;
    - scrolling the preview while the caret blinks stays put;
    - zoom − / + shows scaled pages immediately and they then sharpen;
    - Ctrl+S keeps the cursor and shows "Saved";
    - Export PDF and Export DOCX don't freeze the window and notify;
    - the RSS reported by `ps -o rss= -p $(pgrep markloom)` at 1 and 5 minutes of continuous edits differs by less than 100 MB.
- [ ] **Step 5: Commit** with the message `feat(app): background exports, font loading and search; latency check`.
