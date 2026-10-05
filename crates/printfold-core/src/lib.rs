//! PrintFold engine.
//!
//! Everything that does not need a browser lives here: the data model,
//! markdown parsing, font metrics, the text-flow/pagination engine, booklet
//! imposition, the `.printfold` project format and PDF generation. The Tauri
//! app (`src-tauri`) exposes it to the webview UI through commands.

// Drawing and layout helpers take position, size and style explicitly.
#![allow(clippy::too_many_arguments)]

pub mod flow;
pub mod fonts;
pub mod markdown;
pub mod model;
pub mod pdf;
pub mod project_file;
pub mod text;
