# Markloom Engine (Plan 1 of 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the headless rendering engine. Given a Markdown file and a style preset, it produces Typst pages that can be rasterised for the preview, or written as a PDF that is byte-for-byte the same document. It runs from a CLI example, with no UI yet.

**Architecture:** `markloom-core` parses Markdown into an owned AST. `markloom-style` resolves TOML presets into a `StyleSheet`. `markloom-mermaid` turns diagrams into SVG. `markloom-typeset` generates Typst source, compiles it in-process, and serves page bitmaps, a source map and PDF output from one compiled `PagedDocument`. Milestone 0 spikes come first and confirm the crate choices and latency before the engine is built.

**Tech Stack:** Rust (edition 2024), comrak 0.57, serde + toml 1.x, serde-saphyr 2.0 (YAML front matter), typst / typst-pdf / typst-render / typst-kit / typst-ide 0.15.1, mermaid-rs-renderer 0.3.1 (pending the Task 0 decision), insta 1.49. gpui-kit 0.8.0 is used in the spike only.

**Spec:** `docs/superpowers/specs/2026-10-25-markloom-design.md`

**Plan series:** Plan 1 = engine (Milestones 0–2 + Mermaid). Plan 2 = app shell + paged preview + style panel (Milestones 3–5). Plan 3 = DOCX + polish (Milestones 6–7). Plans 2 and 3 are written after this one ships, using the spike findings.

## Global Constraints

- Linux (Arch) is the only platform that has to work. Avoid OS-specific APIs outside `markloom-app`.
- Pin exact versions of the Typst family (`=0.15.1` for typst, typst-pdf, typst-render, typst-kit and typst-ide). Use typst-kit with `default-features = false, features = ["fonts", "embed-fonts"]`, which means **no network and no Typst packages**.
- Use `mermaid-rs-renderer` with `default-features = false`.
- **Text taken from Markdown must only ever reach Typst as a string literal** (`#"..."`, `raw("...")`, or a string argument). It is never spliced in as markup.
- One compiled `PagedDocument` feeds both `render_page` and `write_pdf`. Nothing re-lays out the document for export.
- Recompile time for a 20-page document must stay under about 150 ms. This is measured in Task 0 and Task 9.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` must pass after every task.
- Source line numbers are **1-based** everywhere.

## Review Focus

1. **Invalid Mermaid syntax** must render as an error box containing the source, and the rest of the document must still compile. Test: Task 7 `invalid_mermaid_renders_error_box`.
2. **Images that are missing, remote (`http…`), or not images** (for example `![x](notes.txt)`) must become a labelled placeholder rather than a failed compile. Test: Task 7 `bad_images_become_placeholders`.
3. **A malformed user preset file** must be skipped and reported, while built-in presets keep working. An unknown font family falls back silently. Test: Task 4 `malformed_user_preset_is_reported_not_fatal`.
4. **An empty document, or one with only front matter,** must compile to one blank page without panicking. Test: Task 7 `empty_document_compiles_to_one_page`.
5. **CRLF line endings** must not shift source ranges. Test: Task 2 `crlf_source_ranges_match_lf`.

---

## File Structure

```
Cargo.toml                         workspace (members = crates/*, exclude = spikes/*), [workspace.dependencies] pins
rust-toolchain.toml                channel chosen in Task 0
crates/markloom-core/src/{lib.rs, ast.rs, parse.rs, inline.rs, front_matter.rs}
crates/markloom-style/src/{lib.rs, sheet.rs, values.rs, registry.rs, merge.rs}
crates/markloom-style/presets/{github,compact,academic,minimal,print-friendly}.toml
crates/markloom-mermaid/src/{lib.rs, rust_backend.rs, mmdc.rs, service.rs}
crates/markloom-typeset/src/{lib.rs, codegen/{mod.rs, preamble.rs, blocks.rs, escape.rs}, world.rs, engine.rs, source_map.rs, output.rs}
crates/markloom-typeset/examples/render.rs
crates/markloom-typeset/benches/recompile.rs
fixtures/mermaid/*.mmd             diagram corpus (Task 0)
fixtures/showcase/*.md + images/   feature showcase (Task 9)
spikes/{preview-latency, mermaid-compare}/   throwaway, outside the workspace
docs/spikes/2026-10-25-m0-findings.md
```

---

### Task 0: Milestone 0 spike (throwaway code, kept findings)

**Files:**
- Create: `spikes/preview-latency/` (a standalone crate with its own `[workspace]` table)
- Create: `spikes/mermaid-compare/` (a standalone crate)
- Create: `fixtures/mermaid/{flowchart,sequence,class,state,er,pie,gantt,timeline,journey,mindmap,gitgraph,quadrant,xychart}.mmd`
- Create: `docs/spikes/2026-10-25-m0-findings.md`

This task has no TDD cycle. Its output is answers. Each step's pass criterion goes into the findings doc together with the measured numbers.

- [ ] **Step 1: Toolchain.** Run `rustup update stable`. Then `cargo new` a scratch crate that depends on `gpui-kit = "0.8.0"` and opens an empty window. Record the rustc version that works and any Arch system packages it needed (Vulkan, wayland, libxkbcommon, …). Write `rust-toolchain.toml` with `channel = "<that version>"`.
- [ ] **Step 2: Preview latency.** `spikes/preview-latency` is a gpui-kit window with two panes:
  - Left: a gpui-kit multiline code editor.
  - Right: a page bitmap.

  On every edit (debounced by 120 ms on the background executor), the right pane does the following:
  1. Wrap the text in `#set page(paper: "a4")` + `#"…"` paragraphs.
  2. Compile it with a minimal `World` that keeps one `Source` alive and calls `Source::replace`.
  3. Render page 1 with `typst_render::render(page, 2.0)`.
  4. Show it as a GPUI image.

  Log compile time and render time. Pass if the median is under 150 ms for a 20-page input (generate 20 pages of lorem text with headings, one table and one code block).
- [ ] **Step 3: Mermaid comparison.** `spikes/mermaid-compare` renders every `fixtures/mermaid/*.mmd` with both `mermaid-rs-renderer` and `rusty-mermaid` (`svg` feature). For each output it checks two things: whether `usvg` parses it, and whether Typst accepts it via `image(bytes(..), format: "svg")` and draws it with readable text (look at the PNG). Fill in a table of type × crate × {ok, wrong, error}. Choose the crate with the most "ok" results. On a tie, choose `mermaid-rs-renderer` because its theme API is richer. Also record the theme/config knobs the winner exposes for background, primary colour and font.
- [ ] **Step 4: Write `docs/spikes/2026-10-25-m0-findings.md`.** It must cover: the toolchain version, system packages, the latency table, the Mermaid matrix, the chosen crate and its theme API, and any Typst 0.15.1 API names that differ from this plan (`World` methods, `Library` builder, `typst::compile` return type, introspector `position`). **If the latency target fails, stop and report to the user before Task 1.**
- [ ] **Step 5: Commit.**
```bash
git add rust-toolchain.toml fixtures/mermaid spikes docs/spikes
git commit -m "spike: validate gpui-kit, typst latency and mermaid crates"
```

---

### Task 1: Workspace scaffold + core block AST

**Files:**
- Create: `Cargo.toml` (workspace), `crates/markloom-core/Cargo.toml`, `src/lib.rs`, `src/ast.rs`, `src/parse.rs`
- Test: `crates/markloom-core/tests/blocks.rs`

**Interfaces:**
- Produces (`markloom_core`):
  - `pub fn parse(markdown: &str) -> Doc`. It never fails: anything unparseable becomes text.
  - `pub struct Doc { pub front_matter: FrontMatter, pub blocks: Vec<Block>, pub footnotes: BTreeMap<String, Vec<Block>> }`
  - `pub struct SourceRange { pub start_line: u32, pub end_line: u32 }` (1-based, inclusive)
  - `pub enum Block`:
    - `Heading { level: u8, content: Vec<Inline>, src }`
    - `Paragraph { content, src }`
    - `Code { lang: Option<String>, code: String, src }`
    - `Diagram { source: String, src }`
    - `Quote { blocks: Vec<Block>, src }`
    - `List { ordered: bool, start: u32, items: Vec<ListItem>, src }`
    - `Table { align: Vec<Align>, header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>>, src }`
    - `Rule { src }`
  - `impl Block { pub fn src(&self) -> SourceRange }`
  - `pub struct ListItem { pub checked: Option<bool>, pub blocks: Vec<Block> }`
  - `pub enum Align { None, Left, Center, Right }`
  - In this task, `Inline` has only `Text(String)` (the node's plain text), and `FrontMatter` is an empty `#[derive(Default)]` struct. Task 2 fills in both.

- [ ] **Step 1: Write the failing tests** in `tests/blocks.rs`:
  - `heading_levels_and_ranges`: `"# A\n\n### B\n"` gives `Heading{level:1, src:1..=1}` and `Heading{level:3, src:3..=3}`.
  - `mermaid_fence_becomes_diagram`: ```` "```mermaid\ngraph TD; A-->B\n```\n" ```` gives `Diagram{source:"graph TD; A-->B\n", src:1..=3}`. A fence tagged `rust` gives `Code{lang:Some("rust")}`.
  - `task_list_items`: `"- [ ] a\n- [x] b\n"` gives `List{ordered:false}` whose items have `checked == Some(false), Some(true)`.
  - `ordered_list_start`: `"3. x\n4. y\n"` gives `start == 3`.
  - `table_alignment`: `"|a|b|c|\n|:-|:-:|-:|\n|1|2|3|\n"` gives `align == [Left, Center, Right]`, a header with 3 cells and 1 row.
  - `nested_quote_contains_list`
  - `thematic_break`
- [ ] **Step 2: Run** `cargo test -p markloom-core`. Expected: compile failure (`parse` not found).
- [ ] **Step 3: Implement.**
  - Set up the workspace `Cargo.toml` with `[workspace.dependencies]` pins from the Tech Stack, `resolver = "3"` and `edition = "2024"`.
  - Implement `parse` in `parse.rs`. Use comrak with these extension options: `table`, `tasklist`, `strikethrough`, `autolink`, `footnotes`, `front_matter_delimiter = Some("---")`. Turn on `sourcepos`. Walk the arena recursively and map node source positions to `SourceRange`. Raw HTML blocks are dropped in this task, and Task 2 handles them.
- [ ] **Step 4: Run** `cargo test -p markloom-core`. Expected: all PASS.
- [ ] **Step 5: Commit** with the message `feat(core): workspace scaffold and Markdown block AST`.

---

### Task 2: Core inlines, footnotes, front matter, HTML, CRLF

**Files:**
- Create: `crates/markloom-core/src/inline.rs`, `src/front_matter.rs`
- Modify: `crates/markloom-core/src/parse.rs`, `src/ast.rs`
- Test: `crates/markloom-core/tests/inlines.rs`

**Interfaces:**
- Consumes: Task 1 `Doc`/`Block`.
- Produces:
  - `pub enum Inline`:
    - `Text(String)`, `Code(String)`
    - `Emph(Vec<Inline>)`, `Strong(Vec<Inline>)`, `Strike(Vec<Inline>)`
    - `Link { url: String, content: Vec<Inline> }`
    - `Image { url: String, alt: String }`
    - `SoftBreak`, `HardBreak`
    - `FootnoteRef(String)`
  - `pub struct FrontMatter { pub title: Option<String>, pub author: Option<String>, pub preset: Option<String>, pub style: Option<toml::Table> }`. This is parsed from YAML: top-level `title` and `author`, plus `markloom.preset` and `markloom.style`. The style mapping is transcoded to a `toml::Table`.
  - `pub fn parse_front_matter(yaml: &str) -> Result<FrontMatter, FrontMatterError>`
  - `Doc.footnotes` maps each footnote label to its definition blocks. Definitions are not included in `Doc.blocks`.

- [ ] **Step 1: Write the failing tests** in `tests/inlines.rs`:
  - `nested_emphasis_and_code`: `"**a _b_ `c`**"` gives `Strong[Text "a ", Emph[Text "b"], Text " ", Code "c"]`.
  - `link_and_image`
  - `hard_and_soft_breaks`
  - `footnote_ref_and_definition`: `"x[^n]\n\n[^n]: note\n"` gives `FootnoteRef("n")` in the paragraph, `footnotes["n"]` holds one paragraph, and `blocks.len() == 1`.
  - `inline_br_html_is_hard_break`: `"a<br>b"` gives `[Text a, HardBreak, Text b]`. Every other inline HTML tag is dropped, but its text content stays.
  - `html_block_dropped`: `"<details>\n<summary>x</summary>\n</details>\n\npara"` gives only the Paragraph.
  - `front_matter_fields`: `"---\ntitle: T\nauthor: A\nmarkloom:\n  preset: compact\n  style:\n    typography:\n      base_size: 10pt\n---\nbody"` gives `title==Some("T")`, `preset==Some("compact")`, `style["typography"]["base_size"] == "10pt"`, and the first block is on line 10.
  - `invalid_front_matter_is_ignored`: malformed YAML gives `FrontMatter::default()` and the body still parses.
  - `crlf_source_ranges_match_lf` (Review Focus 5): the same document with `\r\n` line endings gives identical `src` values and identical text, with no `\r` left in any `Text`.
- [ ] **Step 2: Run** `cargo test -p markloom-core --test inlines`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `inline.rs` maps comrak inline nodes to `Inline`.
  - `front_matter.rs` uses `serde-saphyr` to read the YAML into an intermediate `serde_json::Value`, then converts it with `toml::Value::try_from`.
  - Normalise line endings with `markdown.replace("\r\n", "\n")` before parsing. This keeps the line count the same.
- [ ] **Step 4: Run** `cargo test -p markloom-core`. Expected: all PASS.
- [ ] **Step 5: Commit** with the message `feat(core): inlines, footnotes, front matter`.

---

### Task 3: Style sheet types and value parsing

**Files:**
- Create: `crates/markloom-style/Cargo.toml`, `src/lib.rs`, `src/sheet.rs`, `src/values.rs`
- Test: `crates/markloom-style/tests/values.rs`

**Interfaces:**
- Produces (`markloom_style`). Every struct has `#[serde(default, deny_unknown_fields)]`, and its `Default` is the **github** look.
  - `pub struct Length(pub f32)`, measured in points. It deserialises from `"11pt" | "2cm" | "20mm" | "1in" | "1.5em"`. `em` is resolved later, so store `enum Length { Pt(f32), Em(f32) }`. Use `fn to_typst(&self) -> String`, which produces `"11pt"` or `"1.5em"`.
  - `pub struct Color { r, g, b, a: u8 }` from `"#rgb" | "#rrggbb" | "#rrggbbaa"`, with `fn to_typst(&self) -> String` producing `rgb("#rrggbbaa")`.
  - `pub struct StyleSheet { page: PageStyle, typography: Typography, colors: Colors, code: CodeStyle, table: TableStyle, blockquote: QuoteStyle, lists: ListStyle, diagram: DiagramStyle }` (all fields `pub`)
  - Field lists and github defaults:

| Section | Field: type = github default |
|---|---|
| `PageStyle` | `size: PaperSize{A4,Letter,A5,Legal}=A4`, `margin: Margins{top,right,bottom,left: Length}` = 2cm each (TOML also accepts the shorthand `margin = "2cm"`), `header: String=""`, `footer: String=""`, `page_numbers: PageNumbers{None,BottomCenter,BottomRight}=BottomCenter`, `toc: bool=false` |
| `Typography` | `body_font: Vec<String>=["Inter","Liberation Sans","DejaVu Sans"]`, `heading_font` = same, `code_font=["JetBrains Mono","DejaVu Sans Mono"]`, `base_size=11pt`, `line_height: f32=1.6`, `paragraph_spacing=10pt`, `heading_scale: [f32;6]=[2.0,1.5,1.25,1.0,0.875,0.85]`, `heading_weight: Weight{Regular,Medium,Semibold,Bold}=Semibold`, `justify=false` |
| `Colors` | `text=#1f2328`, `heading=#1f2328`, `link=#0969da`, `muted=#59636e`, `page_bg=#ffffff` |
| `CodeStyle` | `highlight=true`, `background=#f6f8fa`, `border: Option<Color>=None`, `font_size=0.85em`, `radius=6pt`, `inset=10pt` |
| `TableStyle` | `borders: TableBorders{Grid,Horizontal,HeaderOnly,None}=Grid`, `border_color=#d1d9e0`, `border_width=0.75pt`, `header_bg: Option<Color>=Some(#f6f8fa)`, `header_bold=true`, `zebra: Option<Color>=Some(#f6f8fa)`, `cell_padding=6pt`, `font_size=1em`, `full_width=false` |
| `QuoteStyle` | `bar_color=#d1d9e0`, `bar_width=3pt`, `text_color=#59636e`, `italic=false` |
| `ListStyle` | `indent=1.2em`, `bullet="•"` |
| `DiagramStyle` | `theme: MermaidTheme{Default,Neutral,Dark,Forest}=Default`, `scale: f32=1.0`, `max_width: f32=1.0` (fraction of text width, 0.1–1.0), `align: HAlign{Left,Center,Right}=Center`, `background: Option<Color>=None`, `border: Option<Color>=None` |

  - `pub fn StyleSheet::validate(&self) -> Result<(), StyleError>` checks these ranges: `line_height` in 1.0–3.0, `diagram.max_width` in 0.1–1.0, `diagram.scale` in 0.25–4.0, and `base_size` in 6–32pt.
- [ ] **Step 1: Write the failing tests:**
  - `length_units`: `"2cm"` gives `Pt(56.6929)` (±0.01), `"1in"` gives `Pt(72)`, `"1.5em"` gives `Em(1.5)`, and `"12px"` is an error.
  - `color_forms`: `"#fff"` gives `(255,255,255,255)` and `"#11223380"` gives `a=0x80`; `"red"` is an error.
  - `margin_shorthand`: `margin = "1cm"` sets all four sides.
  - `unknown_field_rejected`: `[typography]\nbase_sise = "10pt"` errors, and the error message contains `base_sise`.
  - `default_is_github`: checks three representative defaults from the table.
  - `validate_ranges`: `line_height = 5.0` is an `Err`.
- [ ] **Step 2: Run** `cargo test -p markloom-style`. Expected: FAIL.
- [ ] **Step 3: Implement** the types with `serde` and `toml`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(style): StyleSheet types and value parsing`.

---

### Task 4: Preset registry, inheritance, overrides

**Files:**
- Create: `crates/markloom-style/src/merge.rs`, `src/registry.rs`, `presets/*.toml`
- Test: `crates/markloom-style/tests/registry.rs`

**Interfaces:**
- Consumes: Task 3 `StyleSheet`. Also Task 2 `FrontMatter.style: Option<toml::Table>`, which the caller passes in as an override.
- Produces:
  - `pub struct PresetRegistry`, with:
    - `fn builtin() -> Self`
    - `fn load_user_dir(&mut self, dir: &Path) -> Vec<PresetLoadError>`. This never fails as a whole: bad files are returned in the error list and skipped.
    - `fn names(&self) -> Vec<String>`, with built-ins first, in the order github, compact, academic, minimal, print-friendly.
    - `fn resolve(&self, name: &str, overrides: &[toml::Table]) -> Result<StyleSheet, StyleError>`
  - `pub fn deep_merge(base: &mut toml::Table, over: &toml::Table)`: a key in the overriding table replaces the base value, and nested tables merge recursively.
  - Resolution order:
    1. Build the chain by following `extends` (a top-level string key) from `name` up to the root. Error on a cycle or an unknown name.
    2. Merge the chain from the root down onto an empty table, then apply each override in order.
    3. Remove `extends`, deserialise with serde (missing keys fall back to the github defaults), then call `validate`.
  - A user preset with the same name as a built-in shadows it.
- Preset contents (`github.toml` is just `# defaults`; the others set only what differs):
  - **compact**: `base_size=9.5pt`, `line_height=1.3`, `paragraph_spacing=4pt`, margins 1.2cm, `heading_scale=[1.6,1.35,1.15,1.0,0.9,0.85]`, `cell_padding=3pt`, `code.inset=6pt`.
  - **academic**:
    - fonts `["Libertinus Serif","New Computer Modern"]` for both body and heading;
    - `base_size=11pt`, `justify=true`, margins 2.5cm, `toc=true`;
    - tables `Horizontal` with no `header_bg` and no zebra;
    - `heading_weight=Bold`, `link=#1f2328`, `diagram.theme=Neutral`.
  - **minimal**: tables `HeaderOnly`, `header_bg` and zebra off, `code.background=#ffffff` with `border=#d1d9e0`, quote `italic=true`.
  - **print-friendly**: all colours `#000000` (except `page_bg=#ffffff`), no backgrounds anywhere, tables `Grid` with `border_color=#000000`, `diagram.theme=Neutral`, `page_numbers=BottomRight`.
- [ ] **Step 1: Write the failing tests:**
  - `all_builtins_resolve_and_validate`, which runs over `names()`.
  - `extends_chain`: a user preset `extends="compact"` with `colors.link="#ff0000"` gives `base_size==9.5pt` and `link==#ff0000`.
  - `cycle_detected`: user presets `a extends b` and `b extends a` give `StyleError::Cycle`.
  - `overrides_apply_last`: `resolve("github", [table{typography.base_size="10pt"}])` gives `10pt`.
  - `user_shadows_builtin`.
  - `malformed_user_preset_is_reported_not_fatal` (Review Focus 3): a temp dir with one good file and one file containing invalid TOML gives one error naming the bad file, while `resolve("github")` and the good preset still resolve.
  - `deep_merge_nested`.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.** Built-in presets are embedded with `include_str!`.
- [ ] **Step 4: Run** `cargo test -p markloom-style`. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(style): preset registry with inheritance and built-in presets`.

---

### Task 5: Mermaid service

**Files:**
- Create: `crates/markloom-mermaid/Cargo.toml`, `src/{lib.rs, rust_backend.rs, mmdc.rs, service.rs}`
- Test: `crates/markloom-mermaid/tests/service.rs`

**Interfaces:**
- Consumes: `markloom_style::DiagramStyle`, and the crate chosen in the Task 0 findings.
- Produces:
  - `pub trait DiagramRenderer: Send + Sync { fn render_svg(&self, source: &str, style: &DiagramStyle) -> Result<String, DiagramError>; }`
  - `pub struct RustBackend;`. It maps `MermaidTheme` and `background` onto the crate's theme API, using the knobs recorded in the findings.
  - `pub struct MmdcBackend { bin: PathBuf }`:
    - `fn detect() -> Option<Self>` looks up `mmdc` on `PATH`.
    - `fn new(bin: PathBuf) -> Self`.
    - Rendering runs `bin -i <tmp.mmd> -o <tmp.svg> -t <theme> -b <bg|transparent>` with a 15 s timeout.
  - `pub enum DiagramError { Syntax(String), Unsupported(String), Backend(String) }`
  - `pub struct DiagramService`:
    - `fn new(primary: Box<dyn DiagramRenderer>, fallback: Option<Box<dyn DiagramRenderer>>) -> Self`
    - `fn with_defaults() -> Self`, which uses `RustBackend` plus `MmdcBackend::detect()`.
    - `fn render(&self, source: &str, style: &DiagramStyle) -> Result<Arc<str>, DiagramError>`
  - Fallback rule: try the primary first, and use the fallback only if the primary fails. If both fail, return the primary's error.
  - Cache: `Mutex<HashMap<u64, Result<Arc<str>, DiagramError>>>`, keyed by a hash of `(source, theme, background)`. Errors are cached too.
- [ ] **Step 1: Write the failing tests:**
  - `corpus_renders_valid_svg`: every `fixtures/mermaid/*.mmd` that the findings marked "ok" renders, and `usvg::Tree::from_str` accepts the output.
  - `syntax_error_is_err`: `"graph TD; A-->"` gives an `Err`.
  - `cache_hits`: a counting fake renderer is called once for two identical requests.
  - `fallback_used_when_primary_fails`: a failing fake primary and an OK fake fallback give the fallback's SVG.
  - `mmdc_backend_invokes_binary`: a temp shell script stands in for mmdc and writes `<svg xmlns="http://www.w3.org/2000/svg"/>` to its `-o` argument. `MmdcBackend::new(script)` returns that SVG.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p markloom-mermaid`. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(mermaid): diagram service with cache and mmdc fallback`.

---

### Task 6: Typst code generation

**Files:**
- Create: `crates/markloom-typeset/Cargo.toml`, `src/lib.rs`, `src/codegen/{mod.rs, escape.rs, preamble.rs, blocks.rs}`
- Test: `crates/markloom-typeset/tests/codegen.rs` + `tests/snapshots/`

**Interfaces:**
- Consumes: `markloom_core::{Doc, Block, Inline}` and `markloom_style::StyleSheet`.
- Produces:
  - `pub struct Assets { pub diagrams: Vec<(String /*vpath*/, DiagramRef)>, pub images: Vec<(String /*vpath*/, PathBuf)> }`. Here `pub enum DiagramRef { Svg(Arc<str>), Error { message: String } }`.
  - `pub struct CodegenOutput { pub source: String, pub blocks: Vec<SourceRange>, pub line_to_block: Vec<(u32 /*typst line*/, usize /*block idx*/)>, pub assets: Assets }`
  - `pub fn generate(doc: &Doc, style: &StyleSheet, base_dir: &Path, diagrams: &DiagramService) -> CodegenOutput`
  - `escape::str_lit(s: &str) -> String`. It returns a quoted Typst string, escaping `\`, `"`, newline (`\n`), carriage return and tab.
- Rules:
  - **Preamble** (`preamble.rs`): one `#set`/`#show` rule per style field, using these mappings:
    - `par(leading: {line_height - 1.0}em, spacing: paragraph_spacing, justify)`
    - heading sizes: `#show heading.where(level: n): set text(size: base_size * heading_scale[n-1])`
    - `#set raw(theme: none)` when `!highlight`
    - Table strokes:
      - Grid → `stroke: border_width + border_color`
      - Horizontal → a rule under every row and above the first row
      - HeaderOnly → a rule under row 0 only
      - None → `stroke: none`
      - Fill is `header_bg` for y = 0, `zebra` for even body rows, and none otherwise.
    - Page numbers:
      - BottomCenter → `numbering: "1"` + `number-align: center`
      - BottomRight → `number-align: right`
    - A non-empty `header`/`footer` → `#set page(header: align(center, text(fill: muted, {str_lit})))`, and the same for `footer`. If `page_numbers` is set and the footer is not empty, the footer replaces the number. These strings are always passed as `str_lit`.
    - PNG rasterisation of diagrams for DOCX is deferred to Plan 3.
    - `#outline()` is emitted after the preamble when `toc`.
  - **Blocks**: before each top-level block `i`, emit `#metadata({i}) <ml-b>` on its own line and push the block's `src` to `blocks`. Record `line_to_block` at that line. Block mappings:

| Block | Typst |
|---|---|
| `Heading` | `#heading(level: n)[…]` |
| `Code` | `#raw(block: true, lang: …, {str_lit})` |
| `Quote` | `#quote(block: true)[…]` |
| `List` | `#list(…)` / `#enum(start: n, …)`; for task items prefix `☐ ` or `☑ ` |
| `Table` | `#table(columns: n, align: (…), table.header(…), …)` |
| `Rule` | `#line(length: 100%, stroke: border_color)` |
| `Diagram` (ok) | `#align({align}, block(fill: background, stroke: border, image("/d/{hash}.svg", width: {max_width*100}%)))` |
| `Diagram` (error) | `#block(stroke: red, inset: 8pt)[#text(fill: red)[Diagram error: …] #raw(block: true, {source})]` |

  - **Inlines**:
    - `Text` → `#{str_lit}`
    - `Code` → `#raw({str_lit})`
    - `Emph`/`Strong`/`Strike` → `#emph[…]` / `#strong[…]` / `#strike[…]`
    - `Link` → `#link({str_lit(url)})[…]`
    - `HardBreak` → `#linebreak()`
    - `SoftBreak` → `#" "`
    - `FootnoteRef(l)` → `#footnote[…definition inlines…]`, or `#"[^l]"` if the definition is missing
  - **Images**: a relative `url` is joined to `base_dir` and canonicalised.
    - If the file exists and its extension is one of png/jpg/jpeg/gif/svg/webp, register `/i/{n}.{ext}` and emit `#image("/i/{n}.{ext}", alt: {str_lit(alt)})`.
    - Otherwise (missing file, `http(s)://` URL, or another extension), emit a placeholder: `#box(stroke: muted, inset: 4pt)[#text(fill: muted)[Image: {alt or url}]]`.
- [ ] **Step 1: Write the failing tests**, as `insta` snapshots:
  - `snapshot_preamble_<preset>` for each of the 5 built-in presets
  - `snapshot_blocks_showcase`, which generates code for a fixed small Markdown file covering every block type with the github preset
  - `hostile_text_is_literal`: a paragraph containing `` #set page(width: 1pt) $x$ @ref <lbl> ] [ \ " ` `` produces a source in which this text appears only inside one `str_lit`. Assert that the source does not contain the substring `#set page(width: 1pt)` outside a quoted string, by checking that `str_lit(text)` is present and that the source has exactly one `#set page(` (from the preamble).
  - `block_markers_match_blocks`: the number of `<ml-b>` markers equals `blocks.len()` equals `doc.blocks.len()`.
  - `missing_image_is_placeholder`: `![alt](nope.png)` gives a source containing `Image: alt` and no asset entry.
- [ ] **Step 2: Run** `cargo test -p markloom-typeset --test codegen`. Expected: FAIL.
- [ ] **Step 3: Implement.** Assets are collected here, and Task 7's `World` serves them.
- [ ] **Step 4: Run** the tests, review the new snapshots by hand (`cargo insta review`), then run again. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(typeset): Markdown+style to Typst code generation`.

---

### Task 7: World, Engine::compile, page rendering, PDF

**Files:**
- Create: `crates/markloom-typeset/src/{world.rs, engine.rs, output.rs}`
- Test: `crates/markloom-typeset/tests/engine.rs`

**Interfaces:**
- Consumes: Task 6 `generate` / `CodegenOutput`, and Task 5 `DiagramService`.
- Produces:
  - `pub struct EngineOptions { pub system_fonts: bool }`. The default is `true`. Tests use `false`, so they depend only on the embedded fonts.
  - `pub struct Engine`:
    - `fn new(opts: EngineOptions, diagrams: Arc<DiagramService>) -> Self`
    - `fn compile(&mut self, doc: &Doc, style: &StyleSheet, base_dir: &Path) -> Result<Compiled, CompileFailure>`
    - `fn font_families(&self) -> Vec<String>`, which Plan 2 uses for the font picker
  - `pub struct Compiled { pub document: Arc<typst_layout::PagedDocument>, pub source_map: SourceMap, pub warnings: Vec<Diag> }`. The `source_map` is the Task 8 type; in this task it holds a stub.
  - `impl Compiled`:
    - `fn page_count(&self) -> usize`
    - `fn page_size_pt(&self, i: usize) -> (f32, f32)`
    - `fn page_hash(&self, i: usize) -> u128`, computed with `typst::utils::hash128(&frame)`
    - `fn render_page(&self, i: usize, pixels_per_pt: f32) -> PageBitmap`
    - `fn pdf_bytes(&self, meta: &PdfMeta) -> Result<Vec<u8>, ExportError>`
    - `fn write_pdf(&self, path: &Path, meta: &PdfMeta) -> Result<(), ExportError>`. It writes to `path.with_extension("pdf.tmp")` and then renames.
  - `pub struct PageBitmap { pub width: u32, pub height: u32, pub rgba: Vec<u8> }` (premultiplied alpha is converted to straight alpha)
  - `pub struct PdfMeta { pub title: Option<String>, pub author: Option<String> }`
  - `pub struct CompileFailure { pub diagnostics: Vec<Diag> }`
  - `pub struct Diag { pub message: String, pub md_line: Option<u32>, pub severity: Severity }`. In this task `md_line` is `None`, and Task 8 fills it in.
- World:
  - `MarkloomWorld` owns `LazyHash<Library>`, `LazyHash<FontBook>` and the fonts. The fonts come from typst-kit's embedded set, plus the system fonts when enabled.
  - It holds one main `Source`, which is updated with `Source::replace` on each compile so incremental reuse works.
  - Its file map `HashMap<FileId, Bytes>` is rebuilt from `Assets` on each compile. Diagram SVGs are stored as bytes, and images are read from disk.
  - `today()` returns `None`.
  - Any package import returns `FileError::Other(Some("packages disabled"))`.
  - After each compile, call `comemo::evict(30)`.
- [ ] **Step 1: Write the failing tests** (all with `system_fonts: false`):
  - `compiles_simple_doc`: a heading plus a paragraph gives `page_count() == 1`.
  - `pdf_starts_with_magic`: `pdf_bytes()` starts with `%PDF-`.
  - `render_page_dimensions`: an A4 page at 2.0 px/pt gives 1191×1684 (±1).
  - `preset_changes_layout`: the same 3-page Markdown under `compact` has fewer pages than under `github`.
  - `hostile_markdown_compiles`: the Task 6 hostile string, plus `$`, `#`, `@x`, `<y>` and unbalanced `[` / `]` / backticks in headings, table cells, links and footnotes, all compile, and the text is extractable from the PDF via `pdf_bytes` containing the escaped words. As a simpler check, `Compiled` has `page_count() >= 1` and no error.
  - `invalid_mermaid_renders_error_box` (Review Focus 1): a doc with a bad mermaid fence plus a paragraph compiles successfully, has one page, and has no `Diag` of severity Error.
  - `bad_images_become_placeholders` (Review Focus 2): `![a](missing.png) ![b](https://x/y.png) ![c](notes.txt)` (with `notes.txt` existing in a temp dir) compiles successfully.
  - `empty_document_compiles_to_one_page` (Review Focus 4): both `""` and `"---\ntitle: x\n---\n"` give `page_count() == 1`.
  - `incremental_recompile_is_fast`: in a release-only test (`#[cfg_attr(debug_assertions, ignore)]`), the second compile after a one-word edit of a 20-page doc takes under 150 ms.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement** `world.rs`, `engine.rs` and `output.rs`, adapting the Typst API names according to the Task 0 findings.
- [ ] **Step 4: Run** `cargo test -p markloom-typeset` and `cargo test -p markloom-typeset --release -- --ignored`. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(typeset): in-process Typst engine with page render and PDF output`.

---

### Task 8: Source map and diagnostic mapping

**Files:**
- Create: `crates/markloom-typeset/src/source_map.rs`
- Modify: `crates/markloom-typeset/src/engine.rs`
- Test: `crates/markloom-typeset/tests/source_map.rs`

**Interfaces:**
- Consumes: `CodegenOutput.blocks` / `line_to_block`, and the compiled document's introspector.
- Produces:
  - `pub struct Location { pub page: usize /*0-based*/, pub y_pt: f32 }`
  - `pub struct SourceMap { entries: Vec<(SourceRange, Location)> }`, ordered by the reading order of the document.
  - `impl SourceMap`:
    - `fn locate_line(&self, md_line: u32) -> Option<Location>`
    - `fn line_at(&self, page: usize, y_pt: f32) -> Option<u32>`
- Algorithm:
  1. Query the introspector for `Selector::Label("ml-b")`. For each element, read its `metadata` value `i` and `introspector.position(loc)`, then pair `blocks[i]` with that position.
  2. `locate_line` returns the location of the entry whose range contains the line. If no range contains it, it returns the location of the last entry that starts before the line.
  3. `line_at` returns the `start_line` of the last entry at or before `(page, y)`.
  4. Diagnostics: take the Typst span, find its line in the generated source, find the greatest `line_to_block` entry whose Typst line is at or before that line, and use `blocks[idx].start_line`. This sets `Diag.md_line`.
- [ ] **Step 1: Write the failing tests:**
  - `headings_map_to_pages`: a doc where `# One` is on line 1, a forced page break follows (a paragraph long enough to fill 2 pages), and `# Two` comes after it. Then `locate_line(1).page == 0`, `locate_line(<line of # Two>).page >= 1`, and `y_pt` is monotonic within a page.
  - `line_at_roundtrip`: for every block start line `L`, `line_at(locate_line(L))` returns `L`.
  - `line_inside_code_block_maps_to_block`.
  - `diag_maps_to_markdown_line`. Compile errors cannot come from user text (escaping prevents them), so test the mapping function directly with a synthetic Typst line number: call `source_map::md_line_for_typst_line(&codegen, typst_line) -> Option<u32>` (made `pub(crate)` and tested through a `#[cfg(test)]` re-export), and check that it returns the start line of the enclosing block.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p markloom-typeset`. Expected: PASS.
- [ ] **Step 5: Commit** with the message `feat(typeset): Markdown↔page source map and diagnostic lines`.

---

### Task 9: Showcase fixture, render CLI, exactness and golden tests

**Files:**
- Create: `fixtures/showcase/{index.md, tables.md, diagrams.md, images/logo.png, images/chart.svg}`
- Create: `crates/markloom-typeset/examples/render.rs`, `benches/recompile.rs`
- Test: `crates/markloom-typeset/tests/exactness.rs`, `tests/golden/<preset>-p1.png`

**Interfaces:**
- Consumes: everything above.
- Produces: the CLI `cargo run -p markloom-typeset --example render -- <file.md> [--preset NAME] [--pdf OUT] [--png-dir DIR] [--scale 2.0]`.
  - The style comes from the preset (default `github`), then from the front matter `preset`, then from the front matter `style` overrides.
  - With `--pdf`, it writes the PDF. With `--png-dir`, it writes `page-{n}.png`.
  - It prints the page count and the compile time.
- [ ] **Step 1: Write the showcase fixture.** It covers every block and inline type, all 13 corpus diagram types, front matter, footnotes, a task list, a wide table, a long code block and both images, and fills at least 3 pages under github.
- [ ] **Step 2: Write the failing tests:**
  - `preview_equals_pdf_<preset>` (5 tests). Compile `fixtures/showcase/index.md` and render every page at 1.0 px/pt. Write the PDF to a temp dir and run `pdftoppm -r 72 -png`. Assert that the page counts are equal and that each page pair differs in at most 0.5 % of pixels by more than 32/255 in any channel. If `pdftoppm` is missing, skip with a printed notice.
  - `golden_first_page_<preset>`: page 0 at 1.0 px/pt matches `tests/golden/<preset>-p1.png` within the same tolerance. If the env var `MARKLOOM_BLESS=1` is set, the test rewrites the golden image instead.
  - The bench `recompile` measures a one-character edit of a 20-page doc, from compile through `render_page(0, 2.0)`.
- [ ] **Step 3: Implement** the example and the bench. Use `png` (dev-dependency) to encode and decode images.
- [ ] **Step 4: Run** `MARKLOOM_BLESS=1 cargo test -p markloom-typeset --test exactness`, then inspect the 5 golden PNGs by eye. Then run `cargo test --workspace`, `cargo bench -p markloom-typeset` (median under 150 ms), `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`. Expected: all PASS.
- [ ] **Step 5: Manual check.** Run `cargo run -p markloom-typeset --example render -- fixtures/showcase/index.md --preset academic --pdf /tmp/claude-1000/showcase.pdf` and open the PDF in a viewer: there should be a TOC, serif fonts, horizontal table rules and rendered diagrams.
- [ ] **Step 6: Commit** with the message `test(typeset): showcase fixture, render CLI, exactness and golden tests`.
