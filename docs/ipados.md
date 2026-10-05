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

## Projects

PrintFold starts in the **project browser**: a grid of cover thumbnails
of every project in its library — the app's Documents folder, visible in
the Files app as **On My iPad › PrintFold**.

| Action | Mouse / trackpad | Touch | Keyboard |
|--------|------------------|-------|----------|
| New project ("Untitled", opened right away) | New Project | New Project | ⌘N |
| Open | double-click | tap | Return |
| Select | click; ⌘-click / Shift-click to extend | Select, then tap | arrows, ⌘A |
| Rename | double-click the name, or menu › Rename | menu › Rename | – |
| Duplicate, Share…, Delete | right-click menu or the selection toolbar | press and hold, or ⋯ on the card | ⌘D, ⌘⌫ |
| Import from Files / iCloud Drive | Import… | Import… | ⌘O |
| Add a project from another app | drag `.printfold` files onto the browser | same | – |
| Search by name | – | – | ⌘F |

- **Share…** opens the system share sheet (AirDrop, Mail, Messages, Save to
  Files …) with the `.printfold` file.
- **Delete** asks for confirmation and removes the file (iPadOS has no
  Trash for app documents).
- In the editor, the project name in the header renames the file;
  **Projects** (⌘⇧O) saves, closes the project and returns to the browser.
- Thumbnails are the project's first page, stored inside the `.printfold`
  file (`preview/thumbnail`), refreshed while editing and on close.
- Projects copied into *On My iPad › PrintFold* with the Files app appear
  in the browser; projects shared to PrintFold from other apps are moved
  from `Documents/Inbox` into the library.

## Files, images and fonts in a project

| Way in | Notes |
|--------|-------|
| **+** in the Files panel | Document picker for Markdown (`.md`, `.markdown`, `.txt`), images and fonts (`.ttf`, `.otf`, `.woff`) |
| Drag onto the Files panel | From Files, Photos, Safari or any app that drags files |
| Drag an image onto a static or blank page | Added to the project and placed on that page |
| Image tool on the canvas | System photo/file picker |

Images in formats the PDF engine cannot embed (HEIC from Photos, TIFF,
BMP, GIF) are converted to JPEG on the way in, using WebKit's decoder.
Files that can't be used are listed in a message instead of being dropped
silently.

## Exports

**Export PDF**, PNG page/spread exports, blank templates, the duplex test
page and file downloads open the system share sheet, anchored where you
tapped: **Save to Files**, **Print**, AirDrop, Mail and other apps.

## Touch input

| Action | Mouse / trackpad | Touch / Pencil |
|--------|------------------|----------------|
| Select, move, resize, rotate items | click / drag | tap / drag |
| Marquee selection | drag on empty canvas | one-finger drag on empty canvas |
| Zoom | mouse wheel, trackpad pinch, ⌘/Ctrl + scroll, toolbar | two-finger pinch (also pans), toolbar |
| Pan | trackpad two-finger scroll, Shift-drag, middle-drag | two-finger drag |
| Context menu (arrange, copy, paste, duplicate, delete) | right-click | press and hold an item, or empty page space to paste |
| Edit a text item | double-click | double-tap |
| Polygon vertex: corner ↔ smooth | ⌘-click | double-tap the vertex |
| Polygon vertex: remove | ⌥-click | press and hold the vertex |
| Colour pickers, gradient stops, scrub labels, column resizers | drag | drag |

With a hardware keyboard, the editor shortcuts (←/→ between spreads,
Delete, Escape, ⌘C/⌘V/⌘D) and modifier behaviours (Shift to add to the
selection, Option-drag to duplicate) work as on the Mac. The macOS menu
bar shortcuts are handled in the web view on iPad: ⌘N new project, ⌘⇧O
projects, ⌘⇧A add files, ⌘E export PDF, ⌘1/⌘2 editor/preview, ⌘\ files
sidebar. Without a keyboard, Copy, Paste and Duplicate are in the
press-and-hold menu.

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

- **Opening a project in place from another location.** When a
  `.printfold` stored outside PrintFold's folder (iCloud Drive, another
  app's folder) is opened from the Files app, iPadOS hands over a
  security-scoped URL, which Tauri's iOS runtime does not keep access to.
  PrintFold then asks you to use **Import…** (or drag the file onto the
  browser). Projects inside *On My iPad › PrintFold*, and files shared to
  PrintFold via the share sheet or AirDrop, open directly.
- **Drag and drop** from other apps relies on WKWebView delivering file
  data to the page. The paths are covered by the end-to-end test on
  WebKitGTK; on iPad they still need confirming on a device. **+** and
  **Import…** always work.
- **Google Fonts** for text items need a network connection (as on the
  Mac and in the original).
- **System fonts**: the engine scans the system font folders, which on
  iPadOS hold a smaller set than macOS. Fonts installed through font apps
  or configuration profiles are not in those folders and are not listed;
  upload the font files to the project to use them.
