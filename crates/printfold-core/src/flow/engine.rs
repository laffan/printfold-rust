//! The reflow orchestrator (port of `TextFlowEngine.reflow`).

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::measure::Measurer;
use super::pagination::{flow_sections, insert_blank_pages};
use super::signatures::{
    capture_static_pages, default_signature, merge_static_pages, pad_to_complete_signature,
    signatures_from_pages,
};
use super::slots::{build_initial_slots, flow_sections_into_slots, materialize_slots};
use crate::markdown::footnotes::{build_endnote_sections, collect_footnote_numbers_on_page, inject_chapter_endnotes};
use crate::markdown::parse_markdown_with_footnotes;
use crate::model::{
    page_dimensions, FootnoteDefinition, PageContent, PageItemType, PageState, ProjectSnapshot, Signature,
};

/// Everything a reflow needs: the concatenated markdown plus the project
/// (options and the current signatures, whose static pages are preserved).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRequest {
    pub markdown: String,
    pub project: ProjectSnapshot,
    /// Pixel sizes of project images keyed by file name (filled in by the
    /// app from its file store; names are matched case-insensitively).
    #[serde(default)]
    pub image_sizes: HashMap<String, (f64, f64)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowResult {
    pub signatures: Vec<Signature>,
    pub total_pages: usize,
}

/// Footnotes may never take more than 70% of a page's content height.
const MAX_RESERVATION_RATIO: f64 = 0.7;
/// Reservation iterations (monotonic, so this is a generous bound).
const MAX_RESERVATION_PASSES: usize = 6;

pub fn reflow(m: &mut Measurer, req: &FlowRequest) -> FlowResult {
    let project = &req.project;
    let fonts = &project.font_options;
    let layout = &project.layout_options;
    let output = &project.output_options;

    let static_pages = capture_static_pages(&project.signatures);

    let parsed = parse_markdown_with_footnotes(&req.markdown);
    let footnotes_by_number: HashMap<u32, FootnoteDefinition> =
        parsed.footnotes.iter().map(|f| (f.number, f.clone())).collect();
    let as_endnotes = layout.show_footnotes_as_endnotes == Some(true);
    let mut sections = parsed.sections;
    if as_endnotes && !parsed.footnotes.is_empty() {
        if layout.endnote_placement.as_deref() == Some("chapter") {
            sections = inject_chapter_endnotes(sections, &parsed.footnotes);
        } else {
            sections.extend(build_endnote_sections(&parsed.footnotes));
        }
    }

    let dims = page_dimensions(output, layout);
    m.image_sizes = req.image_sizes.iter().map(|(k, v)| (k.to_lowercase(), *v)).collect();
    m.max_image_height = dims.content_height.max(1.0);
    let has_flow_items = static_pages
        .values()
        .any(|p| p.items().iter().any(|i| i.item_type == PageItemType::TextFlow));
    let place_on_page = !as_endnotes && !parsed.footnotes.is_empty();
    let max_reservation = dims.content_height * MAX_RESERVATION_RATIO;

    let defs_for = |page: &PageContent| -> Vec<FootnoteDefinition> {
        collect_footnote_numbers_on_page(page)
            .into_iter()
            .filter_map(|n| footnotes_by_number.get(&n).cloned())
            .collect()
    };

    let mut effective_static = static_pages.clone();
    let run_flow = |m: &mut Measurer, reservations: &[f64], effective: &mut BTreeMap<u32, PageContent>| {
        let reserved = |i: usize| reservations.get(i).copied().unwrap_or(0.0);
        if has_flow_items {
            let initial = build_initial_slots(&static_pages, &dims);
            let filled = flow_sections_into_slots(m, &sections, initial, &dims, &static_pages, fonts, layout, &reserved);
            let (text_pages, updated) = materialize_slots(filled, &static_pages);
            *effective = updated;
            text_pages
        } else {
            flow_sections(m, &sections, &dims, fonts, layout, &reserved)
        }
    };

    let mut reservations: Vec<f64> = Vec::new();
    let mut text_pages = run_flow(m, &reservations, &mut effective_static);

    if place_on_page {
        for _ in 0..MAX_RESERVATION_PASSES {
            let needed: Vec<f64> = text_pages
                .iter()
                .map(|p| {
                    let defs = defs_for(p);
                    if defs.is_empty() {
                        0.0
                    } else {
                        m.footnote_block_height(&defs, dims.content_width, fonts, layout).min(max_reservation)
                    }
                })
                .collect();
            let len = reservations.len().max(needed.len());
            let merged: Vec<f64> = (0..len)
                .map(|i| reservations.get(i).copied().unwrap_or(0.0).max(needed.get(i).copied().unwrap_or(0.0)))
                .collect();
            let stable = merged.len() == reservations.len()
                && merged.iter().zip(&reservations).all(|(a, b)| (a - b).abs() < 0.5);
            if stable {
                break;
            }
            reservations = merged;
            text_pages = run_flow(m, &reservations, &mut effective_static);
        }
        for page in &mut text_pages {
            let defs = defs_for(page);
            if !defs.is_empty() {
                page.footnotes = Some(defs);
            }
        }
    }

    let text_pages = insert_blank_pages(text_pages, &project.blank_pages);
    let per_sig = output.pages_per_signature();

    if text_pages.is_empty() && static_pages.is_empty() {
        if project.signatures.is_empty() {
            let signatures = default_signature(per_sig, &project.signatures);
            let total = signatures.iter().map(|s| s.page_count as usize).sum();
            return FlowResult { signatures, total_pages: total };
        }
        // Keep the existing structure, but clear text that no longer exists
        // (the original left stale text pages behind when all markdown was
        // removed).
        let mut signatures = project.signatures.clone();
        for sig in &mut signatures {
            for spread in &mut sig.spreads {
                for page in [spread.verso.as_mut(), spread.recto.as_mut()].into_iter().flatten() {
                    if page.page_state == PageState::Text {
                        page.page_state = PageState::Available;
                        page.is_blank = true;
                        page.sections.clear();
                        page.footnotes = None;
                    }
                }
            }
        }
        let total = signatures.iter().map(|s| s.page_count as usize).sum();
        return FlowResult { signatures, total_pages: total };
    }

    let all = merge_static_pages(text_pages, &effective_static);
    let padded = if output.is_booklet() { pad_to_complete_signature(all, per_sig) } else { all };
    let total = padded.len();
    let signatures = signatures_from_pages(padded, per_sig, &project.signatures);
    FlowResult { signatures, total_pages: total }
}
