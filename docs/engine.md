# Rust Engine (`crates/printfold-core`)

The engine holds everything that does not need a browser: the data model,
Markdown parsing, font discovery and shaping, text flow, signatures and
imposition, the `.printfold` format and the PDF writer. It has no Tauri
dependency; `cargo test -p printfold-core` exercises it directly.

```
src/
  lib.rs            module list
  text.rs           JS-compatible whitespace splitting, text transforms
  model/            serde mirror of src/types (camelCase, round-trip safe)
  markdown/         blocks.rs · inline.rs · footnotes.rs
  fonts/            registry.rs · shaping.rs · woff.rs
  flow/             engine.rs · measure.rs · pagination.rs · slots.rs ·
                    polygon.rs · signatures.rs · imposition.rs
  project_file.rs   .printfold ZIP import/export
  pdf/              mod.rs · sheets.rs · page.rs · items.rs · canvas.rs
```

## Data model (`model/`)

Every struct mirrors a TypeScript type in `src/types/index.ts`, serialised
with `#[serde(rename_all = "camelCase")]`. The model must round-trip
anything the UI sends, including fields the engine never reads:

- `PageItem` is one flat struct with optional fields for every item kind,
  plus `#[serde(flatten)] extra` that keeps unknown keys.
- Option structs use container-level `#[serde(default)]` with the same
  defaults as `services/state.ts`, so partial or older projects load.
- `TextSpan` booleans are skipped when false, matching the JS output.
- `Section` holds the parsed `DocumentSection` fields plus the measured
  ones (`lines`, `richLines`, `measuredHeight`, and for images
  `imageWidth` / `imageHeight`).

`model/project.rs` contains the shared geometry: sheet and booklet sizes,
`page_size`, `page_dimensions` (content box), `margins_for_page`
(inner/outer swap on verso/recto) and `spread_rows_per_sheet`.

## Markdown (`markdown/`)

`parse_markdown_with_footnotes(md)` returns sections plus footnote
definitions.

| Step | Module | Notes |
|------|--------|-------|
| Footnotes | `footnotes.rs` | GFM `[^id]` references are numbered in order of first reference and replaced with `\x01FN<n>\x01` sentinels before parsing, then split back out into `footnoteRef` spans. Also builds endnote sections (per document or per chapter). |
| Blocks | `blocks.rs` | pulldown-cmark top-level walk: headings, paragraphs, block quotes, lists, code blocks, thematic breaks (page breaks), images. `rawMarkdown` keeps the source text, including first-line indentation. Tables and raw HTML are skipped, as in the original. |
| Inline | `inline.rs` | Bold / italic / code / strikethrough / links, plus `==highlight==` (pre-marked with private-use characters because CommonMark has no highlight syntax). Adjacent spans with identical style are merged. |

Behaviour follows the original `marked`-based parser: single newlines are
line breaks, nested list levels use `•`, `◦`, `▪`, and block quotes keep
inline formatting. `tools/parity/` compares both parsers on a corpus; see
[parity.md](parity.md#parser-parity).

## Fonts (`fonts/`)

`FontRegistry` (`registry.rs`):

- Discovers system fonts with `fontdb` (macOS `/System/Library/Fonts`,
  `/Library/Fonts`, `~/Library/Fonts`; iOS system fonts; fontconfig dirs
  on Linux). The Tauri app scans on a background thread at startup;
  commands that need fonts wait for the scan (`EngineHandle::with`).
- Resolves CSS-like requests (family, bold, italic) to a face: exact
  family, then `FAMILY_ALIASES` (for example `Arial` → `Helvetica` /
  `Liberation Sans`), then `DEFAULT_FALLBACKS` (serif first on Apple
  platforms, matching WebKit's default; sans first elsewhere).
- Holds custom fonts uploaded into the project (TTF, OTF, TTC; WOFF 1.0 is
  converted to SFNT by `woff.rs`).
- Reports which real faces a family has (`family_variants`) so the UI can
  show which styles will be synthesised.

`LoadedFace` caches a parsed `rustybuzz::Face` (via `self_cell`) with its
ascent/descent. `shaping.rs` shapes text with per-character fallback (a
glyph missing from the requested face is taken from the next candidate)
and caches measured widths.

### Matching WebKit

The editor draws with Konva on a WebKit canvas, so measurement copies
WebKit's behaviour:

| Behaviour | Engine |
|-----------|--------|
| Synthetic bold (no bold face) | +1 px per glyph advance |
| Synthetic italic | skew 0.249 (≈14°) |
| Baseline for Konva `textBaseline: middle` | `top + size/2 + (ascent − descent)/2 · size` |
| Wrap width | 98% of the column (the original's safety margin) |

## Text flow (`flow/`)

`reflow(measurer, FlowRequest) -> FlowResult` in `engine.rs` is the port of
`TextFlowEngine.reflow`:

1. **Capture static pages** from the current signatures (`static`, or
   `available` pages that carry items). They are kept as-is, minus any
   flowed text left over from when they were text pages.
2. **Parse** the markdown; when footnotes are shown as endnotes, append
   endnote sections (document) or inject them after each chapter.
3. **Flow**:
   - no text-flow items → `pagination.rs`: measure each section
     (`measure.rs`) and fill pages, splitting paragraphs across pages,
     honouring page breaks and H1 recto starts;
   - text-flow items present → `slots.rs`: build an ordered list of slots
     (normal content areas on free pages plus rectangle or polygon
     regions on static pages, `polygon.rs` computes per-line horizontal
     spans), fill them, and write the text back into the regions.
4. **Footnote reservations** (on-page footnotes): measure the footnote
   block each page needs, reserve that height (capped at 70% of the
   content height), re-flow, repeat until stable. Reservations only grow,
   so this terminates (at most 6 passes).
5. **Blank pages** requested by the user are inserted, static pages merged
   back in, and booklets padded to complete signatures.
6. **Signatures and spreads** (`signatures.rs`) are rebuilt, preserving
   existing signature ids.

`imposition.rs` computes the booklet page order on each sheet side (also
used by the PDF writer).

### Measurement (`measure.rs`)

`Measurer` holds the font registry, a width cache, image pixel sizes (from
the app's file store) and the maximum image height.

- `wrap_rich_text` works on *units*: a word is glued across style
  boundaries (`**bold**ly` is one unit), so lines never gain or lose spaces
  at span edges.
- Words longer than the column are broken by character.
- Images use the real aspect ratio, scaled to the column width and capped
  at the content height, with an optional caption (9 px italic, 4 pt gap).
- `measure_section` returns height, plain lines and rich lines; spans keep
  their styles so the editor and the PDF draw the same runs.

## Project files (`project_file.rs`)

`export_project` / `import_project` read and write the `.printfold` ZIP:

```
project.json        ProjectManifest (version 2.1.0)
text/<name>.md      markdown files
images/<name>       image files (binary)
fonts/<name>        font files (binary)
static/page-N.json  StaticPageData: pageState, items, backgroundFill,
                    customBackgroundImageId
preview/thumbnail   cover PNG for the project browser (no extension, so
                    image importers — including the original app's — skip it)
```

Older archives load: a missing `pageState` means `static`, and missing
options take their defaults. Unknown fields in items survive a round trip.

## PDF (`pdf/`)

The writer uses [krilla](https://github.com/LaurenzV/krilla) (top-left
origin, font subsetting, Unicode text, lazy image decoding).

| Module | Role |
|--------|------|
| `mod.rs` | `PdfContext`, `generate_pdf`, `generate_test_page`, `prerender_plan` |
| `sheets.rs` | Booklet sheets (imposition, rows, creep), sequential double-/single-sided layouts (autofill, centre, upper-left), crop and fold marks, duplex offset, test page |
| `page.rs` | One page: background, text sections with rich runs, highlight and strikethrough colours, images with captions, footnotes and rule, header/footer |
| `items.rs` | Vector fallback for items when no pre-render exists (shapes, text, gradients, rotation, text-flow text) |
| `canvas.rs` | `Painter`: font and image caches, glyph drawing with synthetic bold/italic, colour parsing, image validation |

### Pre-rendered pages

Gradients, patterns, shadows, array effects and Google Fonts are drawn by
Konva in the webview. `prerender_plan` lists the pages the UI must
rasterise at 300 DPI:

- **overlay**: every page in "render text as images" mode; otherwise
  non-text pages with content, and text pages with items or with items
  crossing in from the facing page;
- **background**: text pages whose background is a gradient or pattern.
  The background layer goes under the vector text, so text stays real
  text.

Overlays are transparent where nothing is drawn, so vector text underneath
remains visible and selectable.

### Creep

With creep enabled (`creepPerSheet`), each sheet nested inside the
outermost one shifts its pages one more `creepPerSheet` towards the fold
(sheet *n* moves by `(n − 1) × creepPerSheet`); `draw_shifted` applies the
shift to everything on the page and clips at the page box.

## Tests

| Test | What |
|------|------|
| unit tests (`cargo test -p printfold-core`) | parsing, footnotes, wrapping, polygons, slots, imposition, project round trip, WOFF |
| `tests/reflow.rs` | end-to-end reflows: padded signatures, static pages, footnotes, text-flow regions, removing all text, a large-document timing check |
| `tests/pdf.rs` | booklet and sequential PDFs written to `target/test-output/` |
| `examples/parse_dump.rs` | parser output as JSON for `tools/parity` |
