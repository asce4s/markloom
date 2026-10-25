# Markloom — mockup layout and smooth editing (design)

## Context
The app Cursor built (`crates/markloom-app`, on branch `feat/engine`) works, but it doesn't feel smooth, and its layout (a gpui-kit dock: file tree, editor, preview, style panel at the bottom) doesn't match the layout the user wants. The user gave a mockup with a dark window frame, a top bar, a "Document theme" bar, three columns (editor | exact page preview | document settings), and per-column status bars.

A review on 2026-10-25 measured these causes of the roughness:

| Problem | Evidence |
|---|---|
| Typst and the page drawing run unoptimised in the default dev build | about 2.7 s per keystroke before the preview updates in debug vs about 50 ms in release (17-page showcase), and an 8.3 s font scan at startup |
| Pages go raster → PNG encode → asynchronous PNG decode in GPUI | changed pages flash blank, and decoded images are never freed (memory grows with every edit) |
| A zoom or scale change clears every page image | the whole preview goes blank until it is redrawn |
| The cursor-follow observer runs on every editor notify, including cursor blink | the preview snaps back to the cursor's page about twice a second |
| The file watcher treats the app's own saves as external edits | after Ctrl+S, `set_value` resets the editor; typing within 0.4 s of a save raises a false "changed on disk" |
| Exports, the font scan, and find-in-files run on the UI thread, and every session change redraws every view | the window freezes on these actions, and redraws do needless work |

Decisions the user confirmed:
- Follow the mockup layout.
- Workspace files go in a **collapsible sidebar**, hidden by default and toggled with Ctrl+B.
- **One exact preview.** No "Live preview / PDF" tabs.
- The mockup's "Custom CSS" section becomes **Custom Typst**.
- **All smoothness fixes ship in this redesign.**

## Goals and success criteria
1. The window matches the mockup's structure (described below). The default view shows editor, preview, and settings, with no file tree.
2. In a dev build (`cargo run -p markloom-app`), the time from a keystroke to the updated preview stays at about 150 ms or less on the 17-page showcase.
3. Updated pages never flash blank. A changed page keeps showing its previous image until the new one is ready, and zoom changes show the scaled old images until the new ones are drawn.
4. After an hour of editing, memory does not keep growing: replaced page images are freed.
5. Scrolling the preview is never overridden, except when the cursor moves to a different line while sync is on.
6. Saving does not touch the editor (cursor, scroll, and undo history stay intact) and never raises a conflict.
7. The UI thread never does an export, the font scan, a file search, or a page drawing.

## Approach
Replace the gpui-kit `DockArea` with a **fixed layout** built from gpui-kit's resizable split panes (`resizable::h_resizable` / `resizable_panel`) plus custom top and bottom bars, using gpui-kit theme tokens only (Design Guides: no raw hex in the UI). Split the 1,882-line `shell.rs` into focused modules. The headless modules that already exist (`workspace`, `settings`, `style_session`, `search`, `preview`) stay and gain the logic described below.

Restyling the dock was rejected: its tabs, headers, and drag-to-dock behaviour can't produce the mockup, and the user doesn't need rearrangeable panels.

## Window layout

```
┌ TopBar: [logo markloom] · [file name · Saved|Edited] ·········· [Open] [⋯] [☀/☾] [⤓ Export PDF] ┐
├ SubBar: · tagline ·································· Document theme [preset ▾]  [⚙ Settings]   ┤
├──────────┬────────────────────┬──────────────────────────────────────┬─────────────────────────┤
│ Files    │ MARKDOWN  [🔍][+ Insert ▾] │ [sync⇅] ······ [−] [Fit width ▾] [+] │ DOCUMENT SETTINGS   [↺] │
│ (hidden  │ editor: line numbers,  │ grey canvas, centred white pages,    │ accordion sections      │
│ default, │ folding, soft wrap,    │ shadow, 24 px gap; click → jump to   │ (see below)             │
│ Ctrl+B)  │ markdown highlight     │ source line                          │                         │
├──────────┼────────────────────┼──────────────────────────────────────┤                         │
│          │ 4722 words · 387 lines │ UTF-8 │ Ln 1, Col 1 │ Preview ready · 4 diagrams │ A4 · Portrait │
└──────────┴────────────────────┴──────────────────────────────────────┴─────────────────────────┘
```

- **TopBar**
  - "Open" opens a native folder or file picker. A picked file opens its parent folder as the workspace and makes that file active.
  - The ⋯ menu holds: New file, Quick open (Ctrl+P), Find in files (Ctrl+Shift+F), Export DOCX, Save style as preset….
  - The sun/moon icon switches the app theme (persisted in settings, default **Dark**).
  - "Export PDF" is the primary button (Ctrl+E).
  - The file name shows "Saved" or "Edited", and "Changed on disk" when there is a conflict.
- **SubBar**
  - A short muted tagline.
  - The "Document theme" select lists `PresetRegistry::names()`; the user presets follow the built-ins.
  - The "Settings" toggle shows or hides the settings column (Ctrl+,).
- **Columns**, all resizable, with widths persisted in settings:
  - files: 220 px, hidden by default
  - editor: 32 % of the remaining width
  - preview: flexible width
  - settings: 300 px
- **Files sidebar:** the existing workspace tree and its operations (open, new, rename, move by drag, delete to trash), built with gpui-kit `tree`.
- **Editor column:** the gpui-kit `Editor` with `line_number(true)`, `folding(true)`, `soft_wrap(true)`, `searchable(true)` and the markdown highlighter.
  - The magnifier opens the editor's own find.
  - "+ Insert" is a dropdown that inserts a snippet at the cursor and selects the snippet's first placeholder word: Table (3×2), Code block, Mermaid diagram (flowchart), Image, Quote, Horizontal rule.
- **Preview column:**
  - A sync toggle (cursor ↔ page, on by default, persisted).
  - Zoom: − / + step through 50, 67, 75, 90, 100, 110, 125, 150, 200 %, and a select offers Fit width, Fit page, and those percentages.
  - A grey canvas with centred white pages and a shadow.
  - Clicking a page moves the editor to the source line (existing source map).
- **Status bars:**
  - editor: `{words} words · {lines} lines`, `UTF-8`, `Ln {l}, Col {c}`
  - preview: `Compiling…` / `Preview ready · {n} diagrams` / `{first error message}` (in the danger colour), and `{paper} · {orientation}`

## Document settings panel
- Header: "DOCUMENT SETTINGS" with a reset icon that clears **all** overrides back to the selected preset.
- Each section is an `accordion` item. Only "Page" is open by default.
- Every control writes one override through `StyleSession::set(path, value)` and triggers a debounced recompile.
- Each section ends with "Reset to preset", which clears the overrides under that section's paths.
- Fields marked **(new)** are new `StyleSheet` fields.

| Section | Controls → style path |
|---|---|
| Page | paper size `page.size`; orientation **(new)** `page.orientation` (portrait\|landscape); margins in mm `page.margin.{top,right,bottom,left}`; background `colors.page_bg`; table of contents `page.toc`; TOC depth **(new)** `page.toc_depth` (1–6, default 3); number headings **(new)** `typography.number_headings`; new page before H1 **(new)** `page.break_before_h1` |
| Typography | body font `typography.body_font[0]`; size `typography.base_size`; line height; paragraph spacing; justify; text colour `colors.text`; link colour `colors.link` |
| Headings | heading font; weight; scale H1–H3 `typography.heading_scale[0..3]`; colour `colors.heading` |
| Tables | borders; border colour; header background; header text colour **(new)** `table.header_text`; header bold; zebra (on/off + colour); cell padding |
| Code blocks | highlight; background; border; font size; code font `typography.code_font[0]` |
| Diagrams | theme; scale; max width; alignment |
| Images **(new)** | max width `images.max_width` (fraction 0.1–1.0, default 1.0); alignment `images.align` (default center) |
| Header & footer | header text; footer text; page numbers (none / bottom center / bottom right) |
| Custom Typst **(new)** | multi-line editor `custom.typst` (default empty), applied after the generated preamble |

- **Custom Typst** is the user's own code, not Markdown text, so it is the one intended exception to "Markdown text only reaches Typst as string literals".
  - It is emitted between `// custom:start` and `// custom:end`.
  - A diagnostic whose Typst line falls inside that region is shown as `Custom Typst: {message}` in the preview status.
  - When the compile fails, the last good pages stay on screen.
- "Save style as preset…" (in the ⋯ menu) asks for a name. It writes the resolved sheet as `{name}.toml` to the user presets folder, with `extends` set to the current preset, and then selects it.
- **New preset `corporate`:**
  - Table: `borders = "horizontal"`, `header_bg = "#1d4ed8"`, `header_text = "#ffffff"`, `header_bold = true`, `zebra = "#f8fafc"`.
  - Colours: `heading = "#0f172a"`, `link = "#1d4ed8"`.
  - Margins 20/18/20/18 mm.

## Smoothness architecture

### Preview pipeline (`preview_pipeline.rs`, mostly headless)
- **Compile** runs on the background executor 120 ms after the last edit, as before.
  - The resolved `StyleSheet` is cached in the session and re-resolved only when the preset or overrides change, never per keystroke.
  - Only the newest generation's result is applied.
- **Raster:** for each visible page ±1 whose `(frame hash, scale)` is not cached, draw it on the background executor (one task per page). Convert the premultiplied RGBA to BGRA and build `Arc<RenderImage>` there, so no PNG is involved. Pass it to `img(ImageSource::Render(..))`.
- **Retention:** a page keeps its previous `RenderImage` until its replacement arrives.
  - On a scale change, old images stay, are drawn at the new page size, and are replaced as new ones arrive.
  - A replaced or evicted image goes to `cx.drop_image(old, Some(window))`, in the same update that installs the new one.
  - The cache holds at most the visible range ±3 pages per scale.
- The decision of what to keep, draw, or drop is pure (`plan_raster(cache, visible, hashes, scale) -> RasterPlan { draw, drop, keep }`) and unit-tested. The GPUI side only executes the plan.

### Build profile
In the root `Cargo.toml`:
```toml
[profile.dev]
opt-level = 1
[profile.dev.package."*"]
opt-level = 3
```

### Cursor sync
The editor observer compares the cursor **line** with the last synced line. It scrolls the preview only when the line changed **and** sync is on. Blinks and selection-only changes do nothing.

### Self-write suppression
`Workspace::save_active` already sets the buffer's `disk` text to what it wrote. `apply_disk(Modified)` reads the file and returns a `DiskChange`. If the content equals the buffer's `disk` text (the app's own write, or a no-op), it returns `DiskChange::None`. The app reloads the editor only on `DiskChange::Reloaded`, and shows a conflict only on `DiskChange::Conflict`.

### Off the UI thread
- PDF export: compile if stale, then write, as a background task.
- DOCX export: background.
- Font scan: the engine is created on the background executor. The preview shows "Loading fonts…" until it is ready.
- Find in files: runs when the query changes (debounced 150 ms) on a background task, over the open buffers plus files read off-thread. Results are cached until the query or a file changes.
- Quick-open fuzzy ranking: runs only while the palette is open.

### Narrow redraws
- The session is split into small entities: `Document` (buffers and text stats), `Preview` (pages, images, status), and `StyleState` (session and resolved sheet).
- Each view observes only the entities it renders. The settings panel does not redraw on keystrokes, and the files sidebar redraws only on workspace changes.

## Module layout (`crates/markloom-app/src`)
```
lib.rs              re-exports for tests
main.rs             fn main → app::run
app.rs              window bootstrap, actions + keybindings, AppRoot view (bars + resizable columns)
document.rs         Document entity: workspace, active buffer, save, watcher, text stats
preview_pipeline.rs headless: RasterPlan, zoom steps, RenderImage conversion; Preview entity glue
style_state.rs      StyleState entity: StyleSession + cached StyleSheet + preset list
views/top_bar.rs, views/sub_bar.rs, views/files.rs, views/editor.rs,
views/preview.rs, views/settings.rs, views/status.rs, views/insert.rs
workspace.rs settings.rs style_session.rs search.rs preview.rs (existing, extended)
```
Each view file should stay under about 400 lines.

## Style model changes (`markloom-style`, `markloom-typeset`)

### New `StyleSheet` fields
- `page.orientation: Orientation { Portrait, Landscape }`, default Portrait
- `page.toc_depth: u8` (1–6), default 3
- `page.break_before_h1: bool`, default false
- `typography.number_headings: bool`, default false
- `table.header_text: Option<Color>`, default None (inherit)
- `images: ImageStyle { max_width: f32 = 1.0, align: HAlign = Center }`
- `custom: CustomStyle { typst: String = "" }`

### Codegen
| Field | Typst output |
|---|---|
| orientation | `#set page(flipped: true)` for landscape |
| toc_depth | `#outline(depth: n)` |
| number_headings | `#set heading(numbering: "1.1")` |
| break_before_h1 | `#show heading.where(level: 1): it => pagebreak(weak: true) + it` |
| header_text | `#show table.cell.where(y: 0): set text(fill: …)` |
| images | `#image(..., width: {max_width*100}%)` wrapped in `#align({align}, …)` |
| custom | the custom block |

### `StyleSession` API
`set(path, toml::Value)`, `clear(path_prefix)`, `clear_all()`. These replace the per-field setters, which are kept as thin wrappers until the views no longer use them, then removed.

## Error handling
- **Compile failure:** the last good pages stay on screen, and the preview status shows the first error (the Custom Typst prefix when it applies).
- **Raster failure:** the page keeps its old image.
- **Export failure:** a gpui-kit notification with the message. The window never blocks.
- **Unknown preset in settings:** fall back to `github` and notify once.
- **Watcher failure:** the file sidebar shows "File watching unavailable" muted text. Editing still works.

## Testing
- `markloom-style`: defaults and validation for the new fields (toc_depth range, images.max_width range); `corporate` resolves.
- `markloom-typeset`:
  - insta snapshots of the preamble for orientation, toc depth, numbering, break before H1, header text, images, and custom Typst;
  - an error in Custom Typst maps to a `Custom Typst:` diagnostic;
  - a landscape page gives width > height.
- `markloom-app` headless:
  - `plan_raster` keeps, draws, and drops correctly across edits, scrolls, and scale changes, and never drops a page that has no replacement;
  - the zoom step sequence;
  - `apply_disk` after `save_active` returns `DiskChange::None`, and an external edit of a clean buffer returns `Reloaded`;
  - `StyleSession::set/clear/clear_all`;
  - insert snippets;
  - word count.
- `markloom-app` UI integration (`#[gpui_kit::test]`):
  - changing paper size triggers one recompile;
  - Ctrl+B toggles the sidebar;
  - Insert → Table inserts at the cursor;
  - a cursor-blink notify does not scroll the preview;
  - saving does not change the editor's cursor.
- Performance: the existing bench, plus a dev-profile check — `cargo build` then a timed one-keystroke compile+raster on the showcase at ≤ 150 ms.
- Manual: `cargo run -p markloom-app -- fixtures/showcase`; compare the window with the mockup; type, scroll the preview, zoom, save, export PDF/DOCX; watch memory over 5 minutes of edits (`ps -o rss`).

## Out of scope
- A flowing (non-paged) reading view.
- CSS.
- Page-break syntax in Markdown.
- Multi-window support.
- macOS/Windows packaging.
