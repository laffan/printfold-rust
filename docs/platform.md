# Tauri Shell (`src-tauri`)

The app crate connects the webview to the engine and to the operating
system. It contains no layout or PDF logic; that lives in
[`printfold-core`](engine.md).

```
src/
  lib.rs          builder: plugins, state, menu, font scan, command list,
                  RunEvent::Opened (file associations)
  state.rs        AppState, FileStore, EngineHandle, Prerendered
  platform.rs     atomic writes, unique names, project library folder,
                  recents, E2E hooks
  menu.rs         macOS menu bar
  commands/       one module per area (below)
tauri.conf.json   window, bundle, .printfold document type
Info.ios.plist    Files-app sharing, iPad orientations
capabilities/     core, dialog and opener permissions for the main window

Plugins: tauri-plugin-dialog (open/save panels, document picker),
tauri-plugin-opener (Show in Finder), tauri-plugin-sharekit (share sheet
on iPadOS, sharing picker on macOS).
```

## State (`state.rs`)

| Field | Purpose |
|-------|---------|
| `engine: EngineHandle` | `FontRegistry` + `Measurer` behind a mutex. The system font scan runs on a background thread at startup; `with()` waits on a condvar until it finishes, `with_now()` does not wait (used by font registration). |
| `files: FileStore` | Binary project files (images, fonts) by id, with name and type. Lets saves, reflows and PDF exports refer to files by id instead of re-sending base64 over IPC. Also provides image pixel sizes for the layout engine. |
| `prerendered` | 300 DPI page PNGs for the current export (overlays and backgrounds). |
| `project_path` | The bound `.printfold` file that auto-save writes to. |
| `thumbnail` | Cover PNG of the open project, embedded on save (`preview/thumbnail`). |
| `pending_opens`, `webview_ready` | Files the OS asked to open before the UI mounted. |

## Commands

All commands are wrapped in `src/services/bridge.ts`; nothing else in the
UI calls `invoke`. Errors are returned as strings and shown by the UI.

| Command | Arguments | Returns | Notes |
|---------|-----------|---------|-------|
| `platform_info` | – | `{ os, mobile }` | Sets `data-platform` on `<html>` |
| `take_pending_opens` | – | paths | Also marks the webview ready |
| `library_info` | – | `{ path, display, canReveal, usesTrash }` | Where projects are stored |
| `library_list` | – | entries | `.printfold` files in the library, plus (macOS) recent projects in other folders; newest first |
| `library_thumbnail` | path | **binary PNG** (empty if none) | Reads only the archive directory and the thumbnail entry |
| `library_create` | name? | `{ name, path }` | New empty project in the library ("Untitled", "Untitled 2" …), bound for auto-save |
| `library_rename` | path, name | `{ name, path }` | Rejects names in use; keeps the bound path and recents in sync |
| `library_duplicate` | path | `{ name, path }` | "<name> copy" in the library |
| `library_delete` | paths | – | macOS: to the Trash; iPadOS: removed (the UI confirms) |
| `library_forget` | path | – | Remove an outside-the-library project from recents |
| `library_import` | – | locations | Document picker; copies `.printfold` files into the library |
| `library_import_bytes` | **binary body**; header `x-file-name` | `{ name, path }` | A project dropped onto the browser; validated before writing |
| `library_share` | path, x, y | – | Share sheet / sharing picker anchored at x, y |
| `library_reveal` | path? | – | Show a project, or the library folder, in Finder |
| `project_set_thumbnail` | **binary PNG** | – | Cover thumbnail for the next save |
| `fonts_list` | – | family names | Installed system families (waits for the scan) |
| `fonts_variants` | family | `{ regular, bold, italic, boldItalic }` | Real faces available |
| `fonts_register` / `fonts_unregister` | family, base64 | – | Custom project fonts (TTF/OTF/TTC/WOFF) |
| `reflow` | markdown, project snapshot | `{ signatures, totalPages }` | Image sizes are added from the file store |
| `clear_measurement_cache` | – | – | After font changes |
| `file_put` / `file_rename` / `files_retain` | id, name, type, base64 / ids | – | Keeps the native file store in sync |
| `pick_files` | filters, multiple | `[{ name, type, content, isBase64 }]` | Open panel / document picker |
| `save_file` | **binary body**; headers `x-file-name`, `x-filter-name`, `x-filter-ext`, optional `x-anchor` | saved? | macOS save panel; iPadOS share sheet (Save to Files, AirDrop, Print …) for a temporary copy |
| `project_open_dialog` | – | opened project or null | macOS: open panel, project stays where it is |
| `project_open_path` | path | opened project | Browser, file associations; decodes the archive in Rust, fills the file store, loads the thumbnail |
| `project_save` | project (without blobs) | – | Builds the archive from the file store (+ thumbnail); atomic write |
| `project_close` | – | – | Unbinds the file |
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
- **Project library**: the folder the project browser shows — iPadOS: the
  app's Documents folder (*On My iPad › PrintFold* in the Files app);
  macOS: `~/Documents/PrintFold`. Projects opened from other folders on
  macOS stay where they are and appear in the browser via recents.
- **File association**: `tauri.conf.json` declares the `printfold`
  extension with the exported UTI `com.printfold.project` (conforms to
  `public.data` and `public.content`). The Tauri CLI (2.11+) writes it to
  the macOS and the iOS `Info.plist`, so Finder and the Files app hand
  `.printfold` files to PrintFold. Conformance to `public.zip-archive` was
  dropped so the Files app does not offer to expand projects as archives.
- **iPadOS**: `UIFileSharingEnabled` and `LSSupportsOpeningDocumentsInPlace`
  show the library in the Files app. Projects arriving from elsewhere are
  moved (Documents/Inbox) or copied into the library. See
  [ipados.md](ipados.md).

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
| `PRINTFOLD_E2E_DIR` | Replaces the project library; saved files are written to this folder without a dialog |
| `PRINTFOLD_E2E_PICK` | `|`-separated paths returned by the next file picker (requires `PRINTFOLD_E2E_DIR`) |

`e2e/run_e2e.py` drives the debug build on Linux through `tauri-driver`
and WebKitWebDriver (WebKitGTK, the closest available engine to WKWebView
there). Build first with `npx tauri build --debug --no-bundle`; a plain
`cargo build` points the webview at the dev server instead of the bundled
UI.
