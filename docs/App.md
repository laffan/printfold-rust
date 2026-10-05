# App (`src/components/App.ts`)

The main UI orchestrator. It mounts every component, drives the project
lifecycle (welcome screen → editor), runs the reflow loop against the Rust
engine, auto-saves, mirrors binary files to the native store, and routes
OS-level events (file associations, the macOS menu bar).

## Components

```
App
├── WelcomeScreen   # New / Open / Recent projects (file-first gate)
├── FileList        # Text / Images / Fonts tabs
├── FilePreview     # CodeMirror markdown editor, image & font preview
├── SpreadEditor    # Konva spread editor
├── PDFPreview      # pdf.js rendering of the generated PDF
└── OptionsPanel    # Selected / Styles / Document / Output tabs
```

Services used: `bridge.ts` (Tauri IPC), `projectIO.ts` (import/save),
`pdfExport.ts` (pre-render + PDF), `projectFile.ts`, `recentProjects.ts`,
`fontService.ts`, `dialogs.ts`.

## Startup

`index.ts` awaits `bridge.init()` (platform info: `macos` / `ios`, sets
`data-platform` and `.is-mobile` on `<html>`), then `new App().init()`:

1. Mount components and wire header buttons, tabs, resizers.
2. `setupStateListeners` — reflow requests, custom-font registry sync,
   font-load reflows, `navigate-to-page` events.
3. `setupNativeFileStore` — mirror image/font files to Rust (below).
4. `setupAutoSave`, `setupFileAssociations`, `setupNativeMenu`.
5. Show the welcome screen.

## Project lifecycle

| Action | macOS | iPadOS |
|--------|-------|--------|
| New Project | Save panel → empty `.printfold` created | Name prompt → `Documents/<name>.printfold` |
| Open Project | Open panel | Document picker (imports a copy into Documents) |
| Recent | `recents.json` in app data | recents + every project in Documents |
| Opened from Finder / Files | `RunEvent::Opened` → `open-project` event | same (files outside PrintFold's folder must be imported via Open Project) |

`loadOpenedProject` resets state, marks the decoded binary files as already
synced (Rust loaded them while decoding), runs `importProject` (two-phase
static-page restore, see `projectIO.ts`), binds the file and reflows.

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
`Save failed` in the header). `saveNow` syncs files, then
`bridge.projectSave(project)` — binary file contents are not sent; Rust
writes the archive atomically (`.tmp` + rename) from its store.

## Native file store

Image and font files (base64 in the UI state) are sent to Rust once via
`file_put`, renamed with `file_rename` and dropped with `files_retain`.
Saves, reflows (image sizing) and PDF exports then reference files by id.

## Export

`Export PDF` → `generatePdf()` (pre-renders pages that carry items with
Konva at 300 DPI, hands them to Rust, Rust assembles the PDF) →
`env.saveFile` (save panel on macOS, "Save to Files" on iPadOS).

## macOS menu bar

`src-tauri/src/menu.rs` builds File (New, Open, Projects…, Add Files,
Export PDF), Edit, View (Toggle Sidebar, Editor, Preview) and Window menus.
Item ids arrive as `menu` events and trigger the same handlers as the
in-window buttons.

## Layout helpers

- Column and panel resizers use pointer events (mouse, trackpad, touch,
  Pencil).
- `Cmd+\` (or the toolbar button) toggles the files sidebar; windows
  narrower than 1000 px start with it collapsed.
- Switching Editor/Preview only toggles the editor-column panels (the
  original also toggled the options panels and blanked them).
