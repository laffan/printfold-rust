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
    pub pre_rendered: &'a HashMap<u32, Vec<u8>>,
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

/// Pages that need a pre-rendered image from the editor before export:
/// every page in "render text as images" mode; otherwise non-text pages with
/// content and text pages with items or items crossing in from a neighbour.
pub fn pages_needing_prerender(project: &ProjectSnapshot) -> Vec<u32> {
    use crate::model::{all_pages, page_size, PageState};
    let page_width = page_size(&project.output_options).width;
    let pages: HashMap<u32, &crate::model::PageContent> =
        all_pages(&project.signatures).map(|p| (p.page_number, p)).collect();
    let all = project.output_options.render_text_as_images == Some(true);
    let mut out: Vec<u32> = all_pages(&project.signatures)
        .filter(|page| {
            if all {
                return true;
            }
            let adjacent = if page.is_recto { page.page_number.checked_sub(1) } else { Some(page.page_number + 1) }
                .and_then(|n| pages.get(&n));
            let crossing = adjacent.is_some_and(|a| {
                a.items().iter().any(|i| if page.is_recto { i.x + i.width > page_width } else { i.x < 0.0 })
            });
            let own = page.has_items() || page.background_fill.is_some() || page.custom_background_image_id.is_some();
            if page.page_state == PageState::Text {
                page.has_items() || crossing
            } else {
                own || crossing
            }
        })
        .map(|p| p.page_number)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}
