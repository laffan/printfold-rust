# PrintFold

Create printable, signature-based booklets from markdown documents — for
**macOS** and **iPadOS**.

![screenshot](screenshot.png)

This is a Rust + [Tauri 2](https://tauri.app) port of
[laffan/printfold](https://github.com/laffan/printfold). It has the same
features and reads and writes the same `.printfold` project files; the
layout engine, project format and PDF writer are native Rust, and the
editor UI runs in the system WebView. See [docs/parity.md](docs/parity.md)
for the feature-by-feature comparison.

## Getting Started

PrintFold opens on the **project browser**: a grid of cover thumbnails of
your projects. Create a project with **New Project** and it opens in the
editor; every change is auto-saved. Back in the browser you can open,
rename, duplicate, share, delete and import projects, or drop
`.printfold` files onto it.

Projects are `.printfold` files (a ZIP archive with a custom extension),
kept in a projects folder:

- **macOS**: `~/Documents/PrintFold`. **Open…** also opens projects from
  any other folder, and PrintFold owns the `.printfold` type, so
  double-clicking one in Finder opens it.
- **iPadOS**: *Files › On My iPad › PrintFold*. Exports and sharing go
  through the system share sheet (Save to Files, AirDrop, Print …).
  Touch, Pencil, mouse, trackpad and keyboard shortcuts are supported.
  See [docs/ipados.md](docs/ipados.md).

## Building

Requirements: Rust (stable), Node.js 20+, Xcode (command-line tools for
macOS; full Xcode for iPadOS).

```bash
npm install
npm run tauri:dev       # run on macOS with a live-reloading UI
npm run tauri:build     # PrintFold.app and .dmg in target/release/bundle

rustup target add aarch64-apple-ios aarch64-apple-ios-sim
npm run ios:init        # once, generates the Xcode project
npm run ios:dev         # simulator or device
```

Tests: `cargo test --workspace` (engine and app), `npm run typecheck`,
and an end-to-end WebDriver run on Linux (`e2e/run_e2e.py`).

## Documentation

Technical documentation starts at [docs/README.md](docs/README.md).

## License

MIT
