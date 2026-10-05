# iPadOS

PrintFold runs on iPad from the same code as macOS: the Tauri iOS target
hosts the identical UI in WKWebView, and the Rust engine is compiled for
`aarch64-apple-ios`. This page covers what differs on iPad.

## Building

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
npm run ios:init        # once: generates src-tauri/gen/apple (Xcode project)
npm run ios:dev         # simulator or a connected device
npm run ios:build       # .ipa (set your development team in Xcode / tauri.conf.json)
```

`src-tauri/Info.ios.plist` is merged into the generated `Info.plist`. The
minimum system version is iPadOS 15 (`tauri.conf.json`).

### Xcode 27

Xcode 27's SwiftPM internalizes the `@_cdecl` symbols that Tauri's Swift
code exports to Rust, so release builds fail to link with
`Undefined symbols … _init_plugin_dialog, _log_stdout, _retain_object …`.
The repository carries the workaround:

- `rust-toolchain.toml` installs rustup's `llvm-tools` component; swift-rs
  uses its `llvm-objcopy` to make those symbols global again (without it,
  the build prints `swift-rs: llvm-objcopy not found`).
- `Cargo.toml` patches swift-rs to
  [PR #80](https://github.com/Brendonovich/swift-rs/pull/80), and
  `.cargo/config.toml` sets `SWIFT_RS_RUNTIME_ARCHIVE = "Tauri"` so the
  swift-rs runtime shim is exported once, from `libTauri.a`.

Remove the patch and the variable once a swift-rs release contains the
change. If a build still fails after pulling these files, delete
`target/aarch64-apple-ios` so the Swift packages are rebuilt.

## Where files live

| What | Location |
|------|----------|
| Projects (`.printfold`) | The app's Documents folder: **Files › On My iPad › PrintFold** |
| Exported PDFs and PNGs | Written to the same folder, then the system export sheet opens so they can be saved elsewhere (cancelling keeps the copy) |
| Recents | `recents.json` in app data; the welcome screen also lists every project in the Documents folder, so files copied there with the Files app show up |

- **New Project** asks for a name and creates `Documents/<name>.printfold`
  (`Name 2`, `Name 3` … if it exists).
- **Open Project** uses the document picker. Projects picked from iCloud
  Drive or other providers are **copied** into the Documents folder and the
  copy is opened and auto-saved.
- **Add files** (+ in the Files area) uses the document picker for
  markdown, images and fonts.

## Touch input

| Action | Mouse / trackpad | Touch / Pencil |
|--------|------------------|----------------|
| Select, move, resize, rotate items | click / drag | tap / drag |
| Marquee selection | drag on empty canvas | one-finger drag on empty canvas |
| Zoom | toolbar buttons | toolbar buttons, two-finger pinch (also pans) |
| Context menu (arrange, copy, paste, duplicate, delete) | right-click | press and hold an item, or empty page space to paste |
| Edit a text item | double-click | double-tap |
| Polygon vertex: corner ↔ smooth | ⌘-click | double-tap the vertex |
| Polygon vertex: remove | ⌥-click | press and hold the vertex |
| Colour pickers, gradient stops, scrub labels, column resizers | drag | drag |

With a hardware keyboard, the keyboard shortcuts (←/→ between spreads,
Delete, Escape, ⌘C/⌘V) and modifier behaviours (Shift to add to the
selection, Option-drag to duplicate) work as on the Mac. Without one,
Copy, Paste and Duplicate are in the press-and-hold menu.

Implementation: `src/components/SpreadEditor/pointer.ts` normalises Konva
mouse and touch events (`PRESS`, `MOVE`, `RELEASE`, `TAP`, `onLongPress`,
`enablePinchZoom`); DOM controls use pointer events. `styles/modules/
platform.css` disables browser panning on those controls, suppresses the
text-selection callout on the canvas, and on screens without hover keeps
hover-only buttons visible and widens resizer handles.

## Layout

- The page respects safe-area insets (home indicator, rounded corners).
- Below 1100 px width (portrait, Split View, Slide Over) the columns get
  narrower; below 1000 px the files sidebar starts collapsed (toggle with
  the sidebar button or ⌘\).
- The viewport is fixed at scale 1 so pinch gestures reach the canvas
  instead of zooming the whole page.
- All orientations and multitasking are allowed (`UIRequiresFullScreen`
  is false).

## Limitations

- **Opening a project in place from another app or location.** When a
  `.printfold` outside PrintFold's folder is opened from the Files app,
  iPadOS hands over a security-scoped URL, which Tauri's iOS runtime does
  not currently keep access to. PrintFold then shows a message asking to
  use **Open Project**, which imports the file. Projects inside
  *On My iPad › PrintFold* open directly.
- **Drag and drop** from other apps into the Files area relies on WKWebView
  delivering HTML drag-and-drop file data; if it does not, use **+**.
- **Google Fonts** for text items need a network connection (as on the
  Mac and in the original).
- **System fonts**: the engine scans the system font folders, which on
  iPadOS hold a smaller set than macOS. Fonts installed through font apps
  or configuration profiles are not in those folders and are not listed;
  upload the font files to the project to use them.
