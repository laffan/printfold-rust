# App (`src/components/App.ts`)

The main UI orchestrator. It mounts every component, switches between the
project browser and the editor, runs the reflow loop against the Rust
engine, and routes OS-level events (file associations, the macOS menu bar,
iPad keyboard shortcuts). The open project's file — lifecycle, auto-save,
native file store, cover thumbnail — lives in `projectSession.ts`.

## Components

```
App
├── ProjectBrowser  # Start screen: project library grid (file manager)
├── FileList        # Text / Images / Fonts tabs
├── FilePreview     # CodeMirror markdown editor, image & font preview
├── SpreadEditor    # Konva spread editor
├── PDFPreview      # pdf.js rendering of the generated PDF
└── OptionsPanel    # Selected / Styles / Document / Output tabs
```

Services used: `bridge.ts` (Tauri IPC), `projectIO.ts` (import/save),
`pdfExport.ts` (pre-render + PDF), `projectFile.ts`, `fontService.ts`,
`fileImport.ts` (dropped/picked files), `dialogs.ts`.

## Startup

`index.ts` awaits `bridge.init()` (platform info: `macos` / `ios`, sets
`data-platform` and `.is-mobile` on `<html>`), then `new App().init()`:

1. Mount components and wire header buttons, tabs, resizers.
2. `setupStateListeners` — reflow requests, custom-font registry sync,
   font-load reflows, `navigate-to-page` events.
3. `session.setup()` — auto-save and the native file store (below).
4. `setupEditorShortcuts` (iPad), `setupFileAssociations`, `setupNativeMenu`.
5. Show the project browser.

## Project browser (`components/ProjectBrowser/`)

The start screen and the way back from the editor (**Projects**, ⌘⇧O).
It lists the project library (`library_list`) as cards with cover
thumbnails (`card.ts`) and works as a file manager:

| Operation | How | Rust |
|-----------|-----|------|
| New | New Project, ⌘N | `library_create` |
| Open | double-click / tap / Return | `project_open_path` |
| Select | click, ⌘/Shift-click, arrows, ⌘A; "Select" mode on touch screens | – |
| Rename | double-click the name (inline), menu, toolbar | `library_rename` |
| Duplicate | menu, toolbar, ⌘D | `library_duplicate` |
| Share | menu, toolbar | `library_share` (share sheet / sharing picker) |
| Delete | menu, toolbar, ⌘⌫ / Delete (confirmed) | `library_delete` (Trash on macOS) |
| Show in Finder | menu, toolbar, footer (macOS) | `library_reveal` |
| Import (iPadOS) / Open… (macOS) | header button, ⌘O | `library_import` / `project_open_dialog` |
| Add dropped `.printfold` files | drag onto the browser | `library_import_bytes` |
| Search | ⌘F | – |

Menus: right-click, press and hold (touch), or the ⋯ button on a card.

## Project session (`components/projectSession.ts`)

| Action | What happens |
|--------|--------------|
| Create | close the current project → `library_create` → reset state → bind → reflow → first save (with thumbnail) |
| Open | close the current project → `project_open_path` (Rust decodes the archive and fills the file store) → two-phase static-page restore (`importProject`) → bind → reflow |
| Close | flush the pending save with a fresh thumbnail → `project_close` → reset state; the browser can then rename, delete or share the file safely |
| Rename (header) | prompt → `library_rename` → rebind |
| Opened from Finder / Files | `RunEvent::Opened` → `open-project` event → Open |

## Reflow loop

```
appState.requestReflow()  (setTimeout 0 + rAF, as before)
        │
        ▼
App.performReflow()  ── coalesces: one request in flight, at most one queued
        │
        ├─ syncFilesNow()            images must be in the store (image sizing)
        ├─ bridge.reflow(markdown, snapshot(project))      → Rust
        ├─ stale check: signatures / files / options / blankPages changed
        │  during the round trip? → discard and run again
        └─ appState.updateProject({ signatures }); spreadEditor.render()
```

The stale check matters because reflow is asynchronous now: an item moved
while Rust was laying out must never be overwritten by an older result.

## Auto-save

Every project change schedules a save 600 ms later (`Saving…` / `Saved` /
`Save failed` in the header). `saveNow` syncs files, re-renders the cover
thumbnail if the last one is older than 20 s (always on close), then
`bridge.projectSave(project)` — binary file contents are not sent; Rust
writes the archive atomically (`.tmp` + rename) from its store.

## Native file store

Image and font files (base64 in the UI state) are sent to Rust once via
`file_put`, renamed with `file_rename` and dropped with `files_retain`.
Saves, reflows (image sizing) and PDF exports then reference files by id.

## Export

`Export PDF` → `generatePdf()` (pre-renders pages that carry items with
Konva at 300 DPI, hands them to Rust, Rust assembles the PDF) →
`env.saveFile` (save panel on macOS, share sheet on iPadOS — anchored at
the last pointer press).

## macOS menu bar

`src-tauri/src/menu.rs` builds File (New, Open, Projects…, Add Files,
Export PDF), Edit, View (Toggle Sidebar, Editor, Preview) and Window menus.
Item ids arrive as `menu` events and trigger the same handlers as the
in-window buttons. iPadOS has no menu bar; `setupEditorShortcuts` maps the
same shortcuts in the web view.

## Layout helpers

- Column and panel resizers use pointer events (mouse, trackpad, touch,
  Pencil).
- `Cmd+\` (or the toolbar button) toggles the files sidebar; windows
  narrower than 1000 px start with it collapsed.
- Switching Editor/Preview only toggles the editor-column panels (the
  original also toggled the options panels and blanked them).
