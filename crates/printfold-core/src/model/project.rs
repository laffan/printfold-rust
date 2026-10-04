//! Project-level types and sheet/page geometry helpers.

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use super::document::{Signature, StaticSpread};
use super::options::{FontOptions, HeaderFooterOptions, LayoutOptions, Margins, OutputOptions};

/// A file attached to the project. Binary content is base64 in `content`
/// when it crosses the IPC boundary (matching the TS `ProjectFile`).
#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFile {
    pub id: String,
    pub name: String,
    /// markdown | image | archive | font | unknown
    #[serde(rename = "type")]
    pub file_type: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub is_base64: bool,
    #[serde(default)]
    pub last_modified: f64,
}

/// The parts of a `BookletProject` the engine needs (layout + PDF). Files are
/// excluded: binary data travels separately through the file store.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectSnapshot {
    pub id: String,
    pub name: String,
    pub output_options: OutputOptions,
    pub layout_options: LayoutOptions,
    pub font_options: FontOptions,
    pub header_footer: HeaderFooterOptions,
    pub signatures: Vec<Signature>,
    pub blank_pages: Vec<u32>,
    pub static_spreads: Option<Vec<StaticSpread>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// Sheet sizes in points, portrait orientation.
pub fn sheet_size(name: &str) -> Option<Size> {
    let (width, height) = match name {
        "letter" => (612.0, 792.0),
        "a4" => (595.28, 841.89),
        "legal" => (612.0, 1008.0),
        "tabloid" => (792.0, 1224.0),
        "a3" => (841.89, 1190.55),
        _ => return None,
    };
    Some(Size { width, height })
}

/// Sheet size with orientation applied (letter if unknown).
pub fn oriented_sheet_size(name: &str, orientation: &str) -> Size {
    let size = sheet_size(name).unwrap_or(Size { width: 612.0, height: 792.0 });
    if orientation == "landscape" {
        Size { width: size.height, height: size.width }
    } else {
        size
    }
}

/// Page size implied by the output options.
pub fn page_size(output: &OutputOptions) -> Size {
    let sheet = oriented_sheet_size(&output.sheet_size, &output.orientation);
    let half_width = sheet.width / 2.0;
    let positive = |v: Option<f64>| v.filter(|v| *v > 0.0);
    match output.booklet_size.as_str() {
        "custom" => Size {
            width: positive(output.custom_width).unwrap_or(half_width),
            height: positive(output.custom_height).unwrap_or(sheet.height),
        },
        "quarter" => Size { width: half_width, height: sheet.height / 2.0 },
        "eighth" => Size { width: half_width, height: sheet.height / 4.0 },
        "sixteenth" => Size { width: half_width, height: sheet.height / 8.0 },
        _ => Size { width: half_width, height: sheet.height },
    }
}

/// Page and content-area dimensions. Header/footer live inside the margin
/// area so they never reduce the content box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageDimensions {
    pub width: f64,
    pub height: f64,
    pub content_width: f64,
    pub content_height: f64,
}

pub fn page_dimensions(output: &OutputOptions, layout: &LayoutOptions) -> PageDimensions {
    let page = page_size(output);
    let m = layout.margins;
    PageDimensions {
        width: page.width,
        height: page.height,
        content_width: page.width - m.inner - m.outer,
        content_height: page.height - m.top - m.bottom,
    }
}

/// Margins for a page with any per-page override applied.
pub fn margins_for_page(page_number: u32, layout: &LayoutOptions) -> Margins {
    let mut m = layout.margins;
    if let Some(o) = layout.margin_overrides.iter().find(|o| o.page_number == page_number) {
        if let Some(v) = o.margins.top {
            m.top = v;
        }
        if let Some(v) = o.margins.bottom {
            m.bottom = v;
        }
        if let Some(v) = o.margins.inner {
            m.inner = v;
        }
        if let Some(v) = o.margins.outer {
            m.outer = v;
        }
    }
    m
}

/// Rows of spreads that fit on one sheet in "fill available space" mode
/// (1 unless at least two rows fit).
pub fn spread_rows_per_sheet(sheet: Size, page_height: f64, fill_enabled: bool) -> usize {
    if !fill_enabled || page_height <= 0.0 {
        return 1;
    }
    let rows = (sheet.height / page_height).floor() as usize;
    if rows >= 2 {
        rows
    } else {
        1
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
