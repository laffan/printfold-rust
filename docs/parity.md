# Feature Parity with the Original

This port aims for a 1:1 feature match with
[laffan/printfold](https://github.com/laffan/printfold) (Electron/web).
The UI code is carried over from the original and re-wired to Rust; the
layout engine, project format and PDF writer were rewritten in Rust. This
page lists every feature area of the original, how it is provided here,
and where behaviour intentionally differs.

## Checklist

Legend: **=** same behaviour · **+** same feature, improved · **≠** platform
difference (see notes).

### Projects

| Feature | Port | Notes |
|---------|------|-------|
| Welcome screen: New, Open, Recent | + | Replaced by the project browser: thumbnail grid of the library with create, open, select, rename, duplicate, share, delete, import, search, drag-in (see [App.md](App.md#project-browser-componentsprojectbrowser)) |
| New Project picks a save location | ≠ | New projects go to the library (`~/Documents/PrintFold`, iPadOS Documents) as "Untitled" and are renamed from the header or the browser; Open… still opens projects anywhere on macOS |
| File-first editing, 600 ms auto-save, Saving/Saved indicator | = | Rust writes the archive from its file store |
| Atomic writes | = | `.tmp` + rename (`platform::atomic_write`) |
| `.printfold` ZIP format (manifest 2.1.0, `text/`, `images/`, `fonts/`, `static/`) | = | Files open in both apps in both directions; the added `preview/thumbnail` entry has no extension, so the original ignores it |
| Legacy archives (no `pageState`, missing options) | = | |
| Projects… button to switch | = | Now also saves and closes the project; File › Projects… (⌘⇧O) |
| Double-click a `.printfold` to open (file association) | = / ≠ | macOS: `RunEvent::Opened`. iPadOS: see [ipados.md](ipados.md#limitations) |
| Single instance, second launch forwards the file | = | macOS apps are single-instance by design; opens go to the running app |
| Recents | = | `recents.json`; projects outside the library appear in the browser (macOS) |
| Web: File System Access handles, Safari/Firefox download fallback, manual Save button | ≠ | Not applicable: there is no browser build. The Save button and the web banner are removed |

### Files area

| Feature | Port | Notes |
|---------|------|-------|
| Text / Images / Fonts tabs, accepted types | + | `.md` (+ `.markdown`, `.txt`); `.png .jpg .jpeg .webp` (+ HEIC/TIFF/BMP/GIF converted to JPEG); `.ttf .otf .woff` |
| Drag files in, `+ Files` button | + | Native open panel / document picker; unusable files are reported instead of skipped silently; images can be dropped straight onto a page |
| Reorder markdown files by dragging | = | Pointer events (works with touch) |
| Edit / remove icons on hover | = | Always visible on touch screens |
| Preview pane: CodeMirror markdown editor, image preview, font sample | = | |
| Download a file from the preview | = | Save panel / Save to Files |
| Sidebar toggle (button, ⌘\) | = | Also in the View menu; starts collapsed below 1000 px |

### Markdown and text flow

| Feature | Port | Notes |
|---------|------|-------|
| Headings, paragraphs, line breaks, emphasis, strong, code, strikethrough, links | = | pulldown-cmark instead of marked; see [Parser parity](#parser-parity) |
| `==highlight==` | = | |
| Block quotes | + | Inline formatting is kept (the original flattened it) |
| Lists | + | Nested levels are marked • ◦ ▪ and indented |
| Code blocks, `---` page breaks, H1 recto start | = | |
| Images in markdown | + | Laid out at their real aspect ratio with captions (the original reserved a fixed box) |
| Tables, raw HTML blocks | = | Skipped in both |
| Pagination across signatures, padding, blank pages | = | |
| Text-flow regions (rectangles, polygons, smooth points) | + | Lines spilled from one slot into a polygon slot are no longer lost |
| Removing all markdown | + | Text pages become available pages (the original kept stale text) |
| Text transform, letter spacing, per-style alignment and colours | = | |
| Justified alignment | = | Behaves like left alignment in the editor and PDF, as in the original (Konva justifies only wrapped lines, and lines are pre-wrapped) |

### Footnotes

| Feature | Port | Notes |
|---------|------|-------|
| GFM `[^id]` references and definitions, sequential numbering | = | |
| On-page footnotes with iterative, monotonic reservations | = | 70% cap |
| Endnotes per document or per H1 chapter, page break before each group | = | |
| Footnote style: font, size, line height, number colour, gap | = | |

### Spread editor

| Feature | Port | Notes |
|---------|------|-------|
| Spread view, thumbnails grouped by signature, zoom, fit | = | Plus pinch zoom on iPad |
| `+ Signature`, `+ Page`, delete static pages | = | |
| Text, shape (rect, ellipse, circle, line, arrow), image, text-flow items | = | |
| Spanning and cross-page items, also across signatures | = | |
| Multi-select, marquee, align and distribute | = | |
| Option-drag duplicate, copy/paste, duplicate, delete, z-order | = | Context menu also offers Paste (needed on iPad without a keyboard) |
| Fill (colour, linear, radial, pattern) with offset; stroke with offset; shadow; array | = | |
| Vertex editing: drag, insert on edge, ⌥-click remove, ⌘-click smooth | = | Touch: double-tap toggles smooth, press-and-hold removes |
| Page backgrounds and custom background images | = | |
| Margin guides with drag-to-adjust | = | |
| Drag images from the Files area onto pages | = | |

### Options panel

| Feature | Port | Notes |
|---------|------|-------|
| Selected tab (item properties, page export/replace) | = | |
| Styles tab (body, headings, code, quote, footnote, header/footer fonts) | = | System fonts come from the Rust font registry |
| Document tab (margins, spacing, footnotes, header/footer, units) | = | |
| Output tab (sheet, booklet type, signatures, marks, duplex offset, creep, render as images, templates) | + | Creep compensation is now applied to the PDF |
| Tab switching (Editor/Preview) | + | No longer blanks the options panel |

### Export

| Feature | Port | Notes |
|---------|------|-------|
| Booklet imposition with multi-row fill | = | |
| Double-sided and single-sided sequential layouts; autofill / centre / upper left | = | |
| Crop marks (colour, thickness), fold marks | = | |
| Duplex offset and calibration test page | = | |
| Render text as images | = | Konva rasterises every page at 300 DPI |
| Pre-rendered items (gradients, shadows, Google Fonts) | + | Pages with gradient/pattern backgrounds keep vector text on top of a background layer |
| Real text with embedded fonts | + | Subset, full Unicode, TTC; same faces as the editor (the original used standard PDF fonts on the web and dropped non-WinAnsi characters) |
| PDF preview | = | pdf.js |
| Page/spread PNG export (300 DPI), replace page/spread with image, blank templates | = | |

## Intentional improvements

Summarised from the tables above:

1. **WYSIWYG measurement.** The engine shapes text with the same font
   files WebKit draws with, mirroring WebKit's synthetic bold/italic and
   baseline placement, so line breaks match between editor, preview and
   print.
2. **Word gluing across styles.** `**bold**ly` stays one word; lines no
   longer gain spaces at style boundaries.
3. **Real image layout** in the markdown flow.
4. **Creep compensation** implemented in the PDF.
5. **Unicode PDF text** with subset fonts and per-character fallback.
6. **Background layers** for gradient/pattern page backgrounds under real
   text, and custom background images included in pre-renders.
7. **Bug fixes**: spilled lines into polygon slots, stale text after
   removing all markdown, block-quote formatting, nested list markers, the
   Editor/Preview switch blanking the options panel, pasted/duplicated
   text-flow regions showing the source's text until the next reflow,
   `==highlight==` markers printed literally in polygon regions.
8. **Native integration**: project browser, macOS menu bar, document
   type, iPadOS Files integration and share sheet, touch and Pencil input,
   trackpad scrolling and pinch on the canvas.
9. **Fonts**: TrueType collections and WOFF in the engine.

## Behaviour differences

| Area | Difference | Reason |
|------|------------|--------|
| Platforms | macOS and iPadOS (Linux/Windows build but are not targets); no web build | Scope of the port |
| Reflow | Asynchronous (IPC); results computed for outdated inputs are discarded | Keeps the UI responsive (a 400-page document reflows in about 35 ms in a release build on the dev machine) |
| Plain `content` of sections | No leftover markdown markers or trailing spaces | pulldown-cmark output; both apps render from the inline spans, so visible text is the same |
| HTML entities | All named entities decoded (`&copy;` → ©) | The original's decoder knew only a few entities |
| Fonts on iPadOS | Fewer system fonts | Platform; upload fonts to the project |
| Mouse wheel / trackpad on the canvas | Mouse wheel zooms (as before); trackpad scroll pans; pinch and ⌘/Ctrl + scroll zoom smoothly | The original zoomed on every wheel event, which made trackpads unusable |

## Parser parity

`tools/parity/run.sh` runs the original `textFlow` parser (marked 11.1.0)
and the Rust parser over `tools/parity/corpus.json` (30 snippets covering
headings, emphasis nesting, links, lists, quotes, code, footnotes,
highlight, images, entities and edge cases) and compares sections,
footnotes and inline spans.

Result: **25 of 30 identical.** The 5 differences are all in the plain
`content` field, which the renderers do not draw from (they use the inline
spans):

| Case | Original | Port |
|------|----------|------|
| `***both***`, `**bold *nested* text**` | stray `**` / `*` left in `content` | markers removed |
| `&copy;` | left encoded | decoded to `©` (also in the inline spans) |
| Trailing spaces before a line break | kept in `content` | trimmed |

```bash
ORIG=/path/to/laffan/printfold tools/parity/run.sh
```

## Known limitations (shared with the original)

- `showOnFirstPage` for headers/footers exists in the data model but has
  never had a UI control or effect; it is unused here too.
- Justified text renders as left-aligned (see above).
- Tables and raw HTML in markdown are skipped.
- Google Fonts for text items need a network connection.

## Port-specific limitations

- macOS and iPadOS builds have not been produced in the Linux container
  this port was developed in. The iOS project must be generated with
  `npm run ios:init` on a Mac.
- iPadOS: opening a `.printfold` in place from outside PrintFold's folder
  asks the user to use Open Project (security-scoped URL limitation).
- iPadOS: drag-and-drop from other apps depends on WKWebView's HTML
  drag-and-drop support; the + button always works.
