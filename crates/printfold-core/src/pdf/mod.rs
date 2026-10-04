//! Print-ready PDF generation (port of `services/pdfGenerator`).
//!
//! Text is drawn as real text with the same font files the layout engine
//! measured with (embedded and subset by krilla, full Unicode — the original
//! pdf-lib output dropped anything outside WinAnsi). Pages carrying items,
//! backgrounds or crossing items arrive pre-rendered from the editor as
//! 300 DPI PNGs (so gradients, patterns, shadows and web fonts match the
//! canvas exactly) and are placed as images.

pub mod canvas;
pub mod items;
pub mod page;
pub mod sheets;

use std::collections::HashMap;

use krilla::metadata::Metadata;
use krilla::Document;
use serde::{Deserialize, Serialize};

use crate::fonts::FontRegistry;
use crate::model::ProjectSnapshot;
use canvas::Painter;

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("PDF serialisation failed: {0}")]
    Krilla(String),
}

/// File metadata the PDF needs to resolve images by id or markdown name.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMeta {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub file_type: String,
}

/// Inputs for one PDF: the project, its files' metadata, pre-rendered page
/// images (PNG, keyed by page number) and a lookup for binary file bytes.
pub struct PdfContext<'a> {
    pub project: &'a ProjectSnapshot,
    pub files: &'a [FileMeta],
    /// Full-page overlays (items, backgrounds of non-text pages, or the
    /// whole page in "render text as images" mode).
    pub pre_rendered: &'a HashMap<u32, Vec<u8>>,
    /// Background-only images for text pages with gradient/pattern fills,
    /// drawn beneath the vector text.
    pub pre_rendered_backgrounds: &'a HashMap<u32, Vec<u8>>,
    pub file_bytes: &'a dyn Fn(&str) -> Option<Vec<u8>>,
}

impl PdfContext<'_> {
    pub(crate) fn image_by_id(&self, p: &mut Painter, id: &str) -> Option<krilla::image::Image> {
        p.image(&format!("file:{id}"), || (self.file_bytes)(id))
    }

    pub(crate) fn pre_rendered_image(&self, p: &mut Painter, page: u32) -> Option<krilla::image::Image> {
        let bytes = self.pre_rendered.get(&page)?;
        p.image(&format!("page:{page}"), || Some(bytes.clone()))
    }

    pub(crate) fn background_image(&self, p: &mut Painter, page: u32) -> Option<krilla::image::Image> {
        let bytes = self.pre_rendered_backgrounds.get(&page)?;
        p.image(&format!("background:{page}"), || Some(bytes.clone()))
    }
}

fn finish(doc: Document) -> Result<Vec<u8>, PdfError> {
    doc.finish().map_err(|e| PdfError::Krilla(format!("{e:?}")))
}

fn new_document(title: &str) -> Document {
    let mut doc = Document::new();
    let mut meta = Metadata::new().creator("PrintFold".to_string()).producer("PrintFold (krilla)".to_string());
    if !title.is_empty() {
        meta = meta.title(title.to_string());
    }
    doc.set_metadata(meta);
    doc
}

/// Generate the booklet/sequential PDF for a project.
pub fn generate_pdf(fonts: &mut FontRegistry, ctx: &PdfContext) -> Result<Vec<u8>, PdfError> {
    let mut doc = new_document(&ctx.project.name);
    let mut painter = Painter::new(fonts);
    let layout = sheets::Layout::new(ctx);
    if ctx.project.output_options.is_booklet() {
        sheets::booklet(&mut doc, &mut painter, ctx, &layout);
    } else {
        sheets::sequential(&mut doc, &mut painter, ctx, &layout);
    }
    finish(doc)
}

/// Generate the two-page duplex offset calibration PDF.
pub fn generate_test_page(fonts: &mut FontRegistry, ctx: &PdfContext) -> Result<Vec<u8>, PdfError> {
    let mut doc = new_document("Duplex calibration");
    let mut painter = Painter::new(fonts);
    sheets::test_page(&mut doc, &mut painter, ctx);
    finish(doc)
}

/// Which pages the editor must rasterise before export.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrerenderPlan {
    /// Full-page overlays: every page in "render text as images" mode;
    /// otherwise non-text pages with content and text pages with items or
    /// items crossing in from their reading-order neighbour.
    pub overlay: Vec<u32>,
    /// Text pages whose background fill is a gradient or pattern (solid
    /// colours and background images are drawn natively).
    pub background: Vec<u32>,
}

pub fn prerender_plan(project: &ProjectSnapshot) -> PrerenderPlan {
    use crate::model::{all_pages, page_size, PageState};
    let page_width = page_size(&project.output_options).width;
    let pages: HashMap<u32, &crate::model::PageContent> =
        all_pages(&project.signatures).map(|p| (p.page_number, p)).collect();
    let all = project.output_options.render_text_as_images == Some(true);
    let mut plan = PrerenderPlan::default();
    for page in all_pages(&project.signatures) {
        let adjacent = if page.is_recto { page.page_number.checked_sub(1) } else { Some(page.page_number + 1) }
            .and_then(|n| pages.get(&n));
        let crossing = adjacent.is_some_and(|a| {
            a.items().iter().any(|i| if page.is_recto { i.x + i.width > page_width } else { i.x < 0.0 })
        });
        let own = page.has_items() || page.background_fill.is_some() || page.custom_background_image_id.is_some();
        let is_text = page.page_state == PageState::Text;
        let overlay = all || if is_text { page.has_items() || crossing } else { own || crossing };
        if overlay {
            plan.overlay.push(page.page_number);
        }
        let fancy_background = page.background_fill.as_ref().is_some_and(|f| f.fill_type != "color");
        if !all && is_text && fancy_background {
            plan.background.push(page.page_number);
        }
    }
    for list in [&mut plan.overlay, &mut plan.background] {
        list.sort_unstable();
        list.dedup();
    }
    plan
}
