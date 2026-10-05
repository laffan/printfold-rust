# PrintFold Technical Documentation

PrintFold creates printable, signature-based booklets from Markdown. This
is the Rust + Tauri port of [laffan/printfold](https://github.com/laffan/printfold)
(Electron/Web), targeting **macOS** and **iPadOS**.

## Contents

1. [Architecture](#architecture)
2. [Repository layout](#repository-layout)
3. [Data flow](#data-flow)
4. [Feature guide](#feature-guide)
5. [Development guide](#development-guide)

Module documentation:

| Area | Document |
|------|----------|
| Rust engine: parsing, fonts, text flow, imposition, project format, PDF | [engine.md](engine.md) |
| Tauri shell: commands (IPC), file handling, fonts, platforms | [platform.md](platform.md) |
| UI orchestration, project browser, project session | [App.md](App.md) |
| Spread editor (Konva) | [SpreadEditor.md](SpreadEditor.md) |
| Options panel | [OptionsPanel.md](OptionsPanel.md) |
| UI state | [state.md](state.md) |
| Data model (TypeScript) | [types.md](types.md) |
| Fonts in the UI | [fontService.md](fontService.md) |
| iPadOS specifics | [ipados.md](ipados.md) |
| Feature parity with the original | [parity.md](parity.md) |

---

## Architecture

```
┌──────────────────────────── WebView (TypeScript, Vite) ───────────────────────────┐
│  ProjectBrowser ◀▶ App ┬─ FileList / FilePreview (CodeMirror)                       │
│                        ├─ SpreadEditor (Konva)      ─┐                              │
│                        ├─ OptionsPanel               ├─ AppState (event emitter)    │
│                        └─ PDFPreview (pdf.js)       ─┘                              │
│  services: bridge.ts · projectIO.ts · pdfExport.ts · pageRenderer.ts (Konva raster) │
└──────────────────────────────────────┬─────────────────────────────────────────────┘
                                        │ Tauri IPC (invoke / events, binary bodies)
┌──────────────────────────── src-tauri (Rust) ──────────────────────────────────────┐
│  commands: reflow · pdf_* · project_* · library_* · file_* · fonts_* · save_file    │
│  state: font registry (system scan on startup) · binary file store · project path  │
│  platform: atomic writes, project library, recents, file associations, macOS menu, │
│            dialogs, share sheet (tauri-plugin-dialog / -opener / -sharekit)        │
└──────────────────────────────────────┬─────────────────────────────────────────────┘
                                        │
┌──────────────────────────── crates/printfold-core (Rust) ──────────────────────────┐
│  model      serde mirror of the TS types (round-trip safe)                          │
│  markdown   pulldown-cmark blocks/inline, ==highlight==, GFM footnotes              │
│  fonts      fontdb discovery + CSS-like resolution, rustybuzz shaping, WOFF         │
│  flow       measurement, pagination, footnote reservations, text-flow regions,      │
│             signatures, imposition                                                  │
│  project_file  .printfold ZIP (manifest 2.1.0, legacy formats)                      │
│  pdf        krilla: imposition, sequential layouts, text, footnotes, headers,      │
│             pre-rendered overlays, print marks, duplex test page, creep             │
└────────────────────────────────────────────────────────────────────────────────────┘
```

**Why this split.** Everything that needs a browser stays in the webview:
the Konva canvas editor, CodeMirror, the colour/gradient pickers, and the
rasterisation of decorated items for the PDF (so gradients, patterns,
shadows and Google Fonts match the canvas pixel for pixel). Everything else
is Rust: the layout engine, the project format and the PDF writer. The
layout engine measures text with the **same font files** the webview
renders with and the PDF embeds, which keeps editor, preview and print in
agreement.

### Key technologies

| Concern | Technology |
|---------|-----------|
| App shell | Tauri 2 (WKWebView on macOS/iPadOS) |
| Layout engine | Rust: pulldown-cmark, fontdb, rustybuzz |
| PDF | krilla (font subsetting, Unicode text, gradients, images) |
| Project files | zip crate (`.printfold` = ZIP) |
| Canvas editor | Konva |
| Markdown editor | CodeMirror 6 |
| PDF preview | pdf.js (legacy build for older WebKit) |
| Build | Vite + TypeScript, Cargo workspace |

---

## Repository layout

```
crates/printfold-core/     Rust engine (no Tauri dependency, fully unit-tested)
  src/model/               document, items, fills, options, project geometry
  src/markdown/            blocks.rs, inline.rs, footnotes.rs
  src/fonts/               registry.rs, shaping.rs, woff.rs
  src/flow/                engine.rs, measure.rs, pagination.rs, slots.rs,
                           polygon.rs, signatures.rs, imposition.rs
  src/project_file.rs      .printfold read/write
  src/pdf/                 mod.rs, sheets.rs, page.rs, items.rs, canvas.rs
  tests/                   reflow.rs, pdf.rs (integration)
  examples/parse_dump.rs   parser output for parity checks
src-tauri/                 Tauri app
  src/commands/            files, fonts, layout, pdf, project, system
  src/platform.rs          atomic writes, library folder, recents, E2E hooks
  src/menu.rs              macOS menu bar
  src/state.rs             AppState, FileStore, engine handle
  tauri.conf.json          window, bundle, .printfold file association
  Info.ios.plist           iPadOS Files-app sharing, orientations
src/                       UI (TypeScript)
  components/              App, projectSession, ProjectBrowser/, FileList,
                           FilePreview, SpreadEditor/, OptionsPanel/,
                           FillPicker/, PDFPreview
  services/                bridge, environment, projectIO, pdfExport,
                           pageRenderer, pageExport, pageGeometry, fontService,
                           fileImport, projectFile, dialogs, state/, text/
  styles/modules/          CSS (platform.css: macOS title bar, iPad touch)
e2e/                       WebDriver end-to-end test (Linux/WebKitGTK)
tools/parity/              parser parity check against the original app
```

---

## Data flow

### Reflow

```
User action (edit markdown, change an option, add a static page …)
      │
      ▼
appState.requestReflow()                         (setTimeout 0 + rAF)
      │
      ▼
App.performReflow()   — one in flight, one queued
      │
      ▼
invoke('reflow', { markdown, project })          → src-tauri/commands/layout.rs
      │
      ▼
printfold_core::flow::reflow
      ├─ capture static pages (static, or available with items)
      ├─ parse markdown (+ footnotes; endnote sections if enabled)
      ├─ flow: plain pagination, or slot flow when text-flow regions exist
      ├─ footnote reservations: iterate until stable (monotonic)
      ├─ insert user blank pages, merge static pages, pad to signatures
      └─ build spreads + signatures
      │
      ▼
App: stale check → appState.updateProject({ signatures }) → SpreadEditor.render()
```

### PDF export

```
generatePdf()  (src/services/pdfExport.ts)
  ├─ invoke('pdf_prerender_plan')        which pages need a raster overlay /
  │                                      background (decided in Rust)
  ├─ Konva renders those pages at 300 DPI (pageRenderer.ts)
  ├─ invoke('pdf_put_prerendered', png)  binary body, per page
  └─ invoke('pdf_generate')              Rust assembles the PDF → bytes
         └─ save panel / "Save to Files"  or pdf.js preview
```

### Saving

```
project change → 600 ms debounce → (cover thumbnail, at most every 20 s)
  → invoke('project_save', project-without-blobs)
  → printfold_core::project_file::export_project_with_thumbnail
    (blobs from the file store)
  → atomic write to the bound .printfold
```

---

## Feature guide

### Projects and the project browser

PrintFold starts in the **project browser**, a grid of cover thumbnails of
the projects in its library — `~/Documents/PrintFold` on macOS, the app's
Documents folder (*On My iPad › PrintFold* in Files) on iPadOS. It works
as a file manager: create, open, select (⌘/Shift-click, arrows), rename
(inline), duplicate, share (share sheet / sharing picker), delete (Trash
on macOS), import, search, and drop `.printfold` files onto it. On macOS,
**Open…** opens projects from any folder in place; they appear in the
browser afterwards. In the editor, click the project name to rename it;
**Projects** (⌘⇧O) saves, closes and returns to the browser.

Every change auto-saves (atomic writes). On macOS the app owns the
`.printfold` type, so double-clicking a project opens it; on iPadOS the
Files app hands `.printfold` files to PrintFold.

A `.printfold` is a ZIP archive: `project.json` (manifest), `text/*.md`,
`images/*`, `fonts/*`, `static/*.json` (per-page state, items,
backgrounds) and `preview/thumbnail` (cover PNG, stored without an
extension so the original app ignores it). Files written by the original
app open unchanged and files written by the port open in the original
(same manifest version 2.1.0).

### Files area

| Tab | Accepted files | Behaviour |
|-----|----------------|-----------|
| **Text** | `.md` (also `.markdown`, `.txt`) | Concatenated in tab order; drag rows to reorder |
| **Images** | `.png`, `.jpg`, `.jpeg`, `.webp` (HEIC, TIFF, BMP, GIF converted to JPEG) | Drag onto static pages (or use the image tool); page backgrounds; pattern fills; `![]()` images in the markdown |
| **Fonts** | `.ttf`, `.otf`, `.woff` | Registered with WebKit and the Rust engine; listed first in every font menu |

Files can be dropped onto the panel (from Finder, Files, Photos …) or
added with **+**; images can also be dropped straight onto a static or
blank page. Rows have edit and remove actions (always visible on touch
screens). The preview pane edits
markdown (CodeMirror) and previews images and fonts.

### Static pages, items and text-flow regions

Unchanged from the original: `+ Signature`, `+ Static Page`, text/shape/
image/text-flow items on any page, items spanning and crossing pages (also
across signatures), alignment and distribution, Option-drag duplicate,
copy/paste, array effects, fill/stroke offsets, shadows, page backgrounds
and custom background images, PNG export and image replacement of pages and
spreads, blank page/spread templates. Text-flow regions (rectangles or
polygons with smooth points) receive a slice of the markdown flow.

### Markdown

Headings (H1 can force a recto start), paragraphs (single newlines are line
breaks), emphasis, strong, code, strikethrough, `==highlight==`, links,
block quotes, lists (nested levels marked • ◦ ▪), code blocks, `---` page
breaks, images (laid out at their real aspect ratio with captions) and GFM
footnotes (on-page with reflow-aware reservations, or endnotes per document
or per chapter). Tables and raw HTML blocks are skipped, as before.

### PDF output

Booklet imposition (with multi-row fill), double-sided and single-sided
sequential layouts (autofill / centre / upper-left), crop marks (colour,
thickness), fold marks, duplex offset with a calibration test page, creep
compensation, and "render text as images". Text is real, selectable text
with embedded, subset fonts and full Unicode.

---

## Development guide

### Prerequisites

- Rust (stable, 1.80+), Node.js 20+.
- macOS: Xcode command-line tools.
- iPadOS: Xcode, an Apple developer team for device builds,
  `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`.
- Linux (CI / E2E only): `libwebkit2gtk-4.1-dev` and friends.

### Commands

```bash
npm install
npm run tauri:dev          # macOS: run the app with hot-reloading UI
npm run tauri:build        # macOS: release .app / .dmg in target/release/bundle
npm run ios:init           # once: generate the Xcode project (src-tauri/gen/apple)
npm run ios:dev            # run on a simulator or device
npm run ios:build          # build an .ipa (needs signing)

cargo test --workspace     # Rust unit + integration tests
npm run typecheck          # TypeScript
npm run build              # typecheck + production UI build
python3 e2e/run_e2e.py     # end-to-end (Linux; see e2e/run_e2e.py header)
ORIG=../printfold tools/parity/run.sh   # parser parity vs the original
```

### Conventions

- Code files stay under ~700 lines; keep modules focused.
- The engine (`printfold-core`) has no Tauri dependency — test it with
  `cargo test` directly.
- Every native capability the UI uses goes through `src/services/bridge.ts`.
- Units are PDF points (72 per inch) everywhere; coordinates are
  top-left-origin in both the editor and the Rust PDF writer.

### Important constants

| Constant | Value | Location |
|----------|-------|----------|
| Points per inch | 72 | everywhere |
| Wrap safety margin | 98% of width | `flow/measure.rs` |
| Footnote reservation cap | 70% of content height | `flow/engine.rs` |
| Pre-render resolution | 300 DPI | `services/pageRenderer.ts` |
| Auto-save debounce | 600 ms | `components/projectSession.ts` |
| Thumbnail refresh / width | 20 s / 360 px | `components/projectSession.ts` |
| Long press (touch context menu) | 500 ms | `SpreadEditor/pointer.ts` |
