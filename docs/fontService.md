# Font Service (`src/services/fontService.ts`)

The font service keeps the font lists for the UI and makes sure the
webview (editor canvas, previews) and the Rust engine (layout and PDF) use
the **same font files**. Font discovery, resolution, shaping and embedding
live in Rust (`crates/printfold-core/src/fonts`, see
[engine.md](engine.md#fonts-fonts)); this service is the UI side.

## Font categories

### Style fonts (body, headings, code, block quotes, header, footer)

Used for the markdown flow. `getStyleFonts()` returns the installed system
families reported by the engine (`bridge.fontsList()`, which waits for the
startup font scan). Until that list arrives, or if it is empty, the
web-safe list is used:

| Category | Fonts |
|----------|-------|
| Serif | Georgia, Times New Roman, Palatino, Garamond, Baskerville, Book Antiqua, Cambria |
| Sans-serif | Arial, Helvetica, Verdana, Tahoma, Trebuchet MS, Lucida Sans, Segoe UI, Calibri, Candara, Optima, Futura, Gill Sans, Century Gothic |
| Monospace | Courier New, Courier, Lucida Console, Monaco, Consolas, Menlo |

Because the engine measures and the PDF embeds the very face WebKit draws
with, line breaks in the editor, the preview and the printed PDF agree.
Names WebKit resolves through aliases (for example `Arial` on a system
without Arial) are resolved the same way by the engine's alias table.

### Item fonts (text items on static pages)

`getItemFonts()` returns 40+ Google Fonts plus the web-safe fonts. Items
are rasterised by Konva for the PDF, so any font WebKit can render works.
Google Fonts are loaded from `fonts.googleapis.com` on demand
(`loadGoogleFont`, `loadGoogleFonts`, `preloadAllGoogleFonts`) and need a
network connection, as in the original.

### Custom fonts (uploaded to the project)

Dropping a `.ttf`, `.otf` or `.woff` into the Files area calls
`registerCustomFont(fileName, base64)`:

1. Stores the bytes and injects an `@font-face` rule (managed `<style>`
   element), so the family is usable in the editor, canvas and previews.
2. Registers the same bytes with the engine (`bridge.fontsRegister`).
   WOFF is unpacked to SFNT in Rust; TrueType collections are accepted.
3. Broadcasts `onFontLoaded` only after both `document.fonts.load()` and
   the engine registration finish, so the follow-up reflow measures with
   the real typeface, not a fallback.

Custom families are listed first in every font menu (`FontDropdown`).
`unregisterCustomFont` removes the family from both sides. Uploaded fonts
are single-face: bold and italic are synthesised (as WebKit does).

## Methods

| Method | Description |
|--------|-------------|
| `getStyleFonts()` | System families, or web-safe fonts until they load |
| `getItemFonts()` | Google + web-safe fonts |
| `getCustomFonts()` / `isCustomFont(name)` | Uploaded families |
| `registerCustomFont(fileName, base64)` | Install in WebKit and the engine; returns the family (file name without extension) |
| `unregisterCustomFont(family)` | Remove from both |
| `onCustomFontsChanged(cb)` | Additions/removals (font menus rebuild) |
| `getFontVariants(family)` | Which real faces exist (`regular`, `bold`, `italic`, `boldItalic`), from `bridge.fontsVariants`; cached per family. Font menus use it for the B / I / BI availability indicators. |
| `loadSystemFonts()` / `hasSystemFonts()` / `onSystemFontsLoaded(cb)` | System family list |
| `loadGoogleFont(s)`, `isGoogleFont`, `isGoogleFontLoaded` | Google Fonts for items |
| `onFontLoaded(cb)` | Any font became available (App reflows) |
| `getFontFamily(name)` | CSS `font-family` with a category fallback, e.g. `"Georgia", serif` |
| `checkFontAvailable(family)` | `document.fonts.check` for previews |

The legacy `googleFonts` export is kept for older call sites.

## Differences from the original

| Aspect | Original (Electron / web) | Port |
|--------|---------------------------|------|
| System font discovery | `system_profiler` / PowerShell / `fc-list` via shell | `fontdb` scan in Rust, no shell |
| PDF fonts | pdf-lib + fontkit, full embedding (subsetter unreliable); web build used standard PDF fonts | krilla, subset embedding of the exact faces, full Unicode |
| Font collections (`.ttc`) | Skipped | Supported |
| Missing glyphs | Characters outside WinAnsi dropped with standard fonts | Per-character fallback to another installed face |
| Measurement | Canvas `measureText` in the webview | rustybuzz shaping of the same files, matching WebKit's synthetic bold/italic |
