# Tauri Shell (`src-tauri`)

The app crate connects the webview to the engine and to the operating
system. It contains no layout or PDF logic; that lives in
[`printfold-core`](engine.md).

```
src/
  lib.rs          builder: plugins, state, menu, font scan, command list,
                  RunEvent::Opened (file associations)
  state.rs        AppState, FileStore, EngineHandle, Prerendered
  platform.rs     atomic writes, unique names, Documents folder, recents,
                  E2E hooks
  menu.rs         macOS menu bar
  commands/       one module per area (below)
tauri.conf.json   window, bundle, .printfold document type
Info.ios.plist    Files-app sharing, iPad orientations
capabilities/     core, dialog and opener permissions for the main window
```

## State (`state.rs`)

| Field | Purpose |
|-------|---------|
| `engine: EngineHandle` | `FontRegistry` + `Measurer` behind a mutex. The system font scan runs on a background thread at startup; `with()` waits on a condvar until it finishes, `with_now()` does not wait (used by font registration). |
| `files: FileStore` | Binary project files (images, fonts) by id, with name and type. Lets saves, reflows and PDF exports refer to files by id instead of re-sending base64 over IPC. Also provides image pixel sizes for the layout engine. |
| `prerendered` | 300 DPI page PNGs for the current export (overlays and backgrounds). |
| `project_path` | The bound `.printfold` file that auto-save writes to. |
| `pending_opens`, `webview_ready` | Files the OS asked to open before the UI mounted. |

## Commands

All commands are wrapped in `src/services/bridge.ts`; nothing else in the
UI calls `invoke`. Errors are returned as strings and shown by the UI.

| Command | Arguments | Returns | Notes |
|---------|-----------|---------|-------|
| `platform_info` | – | `{ os, mobile }` | Sets `data-platform` on `<html>` |
| `take_pending_opens` | – | paths | Also marks the webview ready |
| `recents_list` / `recents_add` / `recents_remove` | path, name | entries | `recents.json` in app data; max 10; iPadOS also lists every project in Documents |
| `fonts_list` | – | family names | Installed system families (waits for the scan) |
| `fonts_variants` | family | `{ regular, bold, italic, boldItalic }` | Real faces available |
| `fonts_register` / `fonts_unregister` | family, base64 | – | Custom project fonts (TTF/OTF/TTC/WOFF) |
| `reflow` | markdown, project snapshot | `{ signatures, totalPages }` | Image sizes are added from the file store |
| `clear_measurement_cache` | – | – | After font changes |
| `file_put` / `file_rename` / `files_retain` | id, name, type, base64 / ids | – | Keeps the native file store in sync |
| `pick_files` | filters, multiple | `[{ name, type, content, isBase64 }]` | Open panel / document picker |
| `save_file` | **binary body**; headers `x-file-name`, `x-filter-name`, `x-filter-ext` | saved? | macOS save panel; iPadOS writes to Documents and opens the export sheet |
| `project_new` | name | `{ name, path }` or null | macOS save panel; iPadOS `Documents/<name>.printfold` (unique name) |
| `project_open_dialog` | – | opened project or null | Open panel / document picker |
| `project_open_path` | path | opened project | Recents, file associations; decodes the archive in Rust and fills the file store |
| `project_save` | project (without blobs) | – | Builds the archive from the file store; atomic write |
| `project_close` | – | – | Unbinds the file and clears stores |
| `pdf_prerender_plan` | project | `{ overlay, background }` | Which pages Konva must rasterise |
| `pdf_put_prerendered` | **binary PNG**; headers `x-page`, `x-layer` | – | `layer` is `overlay` or `background` |
| `pdf_clear_prerendered` | – | – | Before each export |
| `pdf_generate` | project, file metadata | **binary PDF** | `tauri::ipc::Response`, no base64 |
| `pdf_test_page` | project | **binary PDF** | Duplex calibration page |

Binary bodies use Tauri 2's raw invoke payloads (`InvokeBody::Raw`) with
metadata in headers (percent-encoded so non-ASCII names survive).

## Events

| Event | Payload | Source |
|-------|---------|--------|
| `open-project` | path | A `.printfold` opened from Finder / Files (`RunEvent::Opened`) or passed as an argument (Windows/Linux) |
| `menu` | item id | macOS menu bar (`menu.rs`) |

## Files and projects

- **Atomic writes**: the archive is written to `<file>.tmp` and renamed, so
  a crash during auto-save never truncates a project.
- **File association**: `tauri.conf.json` declares the `printfold`
  extension with the exported UTI `com.printfold.project` (conforms to
  `public.zip-archive`), so Finder opens projects in PrintFold.
- **iPadOS**: projects live in the app's Documents folder, shown in the
  Files app as *On My iPad › PrintFold* (`UIFileSharingEnabled`,
  `LSSupportsOpeningDocumentsInPlace`). Projects picked from elsewhere are
  copied into Documents. See [ipados.md](ipados.md).

## macOS menu bar (`menu.rs`)

| Menu | Items (id) |
|------|-----------|
| PrintFold | About, Services, Hide, Hide Others, Show All, Quit (system items) |
| File | New Project… ⌘N (`new-project`), Open Project… ⌘O (`open-project`), Projects… ⌘⇧O (`projects`), Add Files… ⌘⇧A (`add-files`), Export PDF… ⌘E (`export-pdf`), Close Window |
| Edit | Undo, Redo, Cut, Copy, Paste, Select All (system items) |
| View | Toggle Files Sidebar ⌘\ (`toggle-sidebar`), Editor ⌘1 (`show-editor`), Preview ⌘2 (`show-preview`), Full Screen |
| Window | Minimize, Zoom |

The window uses `titleBarStyle: Overlay`; the in-window header is the drag
region (`data-tauri-drag-region`) and leaves room for the traffic lights.

## End-to-end hooks

For automated tests only, two environment variables bypass native dialogs:

| Variable | Effect |
|----------|--------|
| `PRINTFOLD_E2E_DIR` | New projects and saved files are written to this folder without a dialog |
| `PRINTFOLD_E2E_PICK` | `|`-separated paths returned by the next file picker (requires `PRINTFOLD_E2E_DIR`) |

`e2e/run_e2e.py` drives the debug build on Linux through `tauri-driver`
and WebKitWebDriver (WebKitGTK, the closest available engine to WKWebView
there). Build first with `npx tauri build --debug --no-bundle`; a plain
`cargo build` points the webview at the dev server instead of the bundled
UI.
