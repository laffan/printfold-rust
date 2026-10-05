//! Static-page preservation, signature padding and spread construction
//! (port of `textFlow/signatures.ts`).

use std::collections::BTreeMap;

use super::pagination::available_page;
use crate::model::{new_id, PageContent, PageState, Signature, Spread};

/// Pages that survive a reflow: static pages, and available pages that
/// carry items. Keyed by page number.
pub fn capture_static_pages(signatures: &[Signature]) -> BTreeMap<u32, PageContent> {
    let mut preserved = BTreeMap::new();
    for page in signatures.iter().flat_map(|s| s.pages()) {
        let keep = page.page_state == PageState::Static
            || (page.page_state == PageState::Available && page.has_items());
        if keep {
            // Static and available pages own items and backgrounds only;
            // drop flowed text left over from when they were text pages.
            let mut page = page.clone();
            page.sections.clear();
            page.footnotes = None;
            preserved.insert(page.page_number, page);
        }
    }
    preserved
}

/// Interleave flowed text pages with preserved pages at their numbers.
pub fn merge_static_pages(text_pages: Vec<PageContent>, preserved: &BTreeMap<u32, PageContent>) -> Vec<PageContent> {
    if preserved.is_empty() {
        return text_pages
            .into_iter()
            .enumerate()
            .map(|(i, mut p)| {
                p.page_number = i as u32 + 1;
                p.is_recto = p.page_number % 2 == 1;
                p.page_state = PageState::Text;
                p
            })
            .collect();
    }
    let max_preserved = preserved.keys().max().copied().unwrap_or(0) as usize;
    let needed = (text_pages.len() + preserved.len()).max(max_preserved);
    let mut text = text_pages.into_iter();
    let mut result = Vec::with_capacity(needed);
    for n in 1..=needed as u32 {
        if let Some(p) = preserved.get(&n) {
            let mut p = p.clone();
            p.page_number = n;
            p.is_recto = n % 2 == 1;
            result.push(p);
        } else if let Some(mut p) = text.next() {
            p.page_number = n;
            p.is_recto = n % 2 == 1;
            p.page_state = PageState::Text;
            result.push(p);
        } else {
            let mut blank = super::pagination::create_empty_page(n, true, false);
            blank.is_recto = n % 2 == 1;
            result.push(blank);
        }
    }
    for mut p in text {
        let n = result.len() as u32 + 1;
        p.page_number = n;
        p.is_recto = n % 2 == 1;
        p.page_state = PageState::Text;
        result.push(p);
    }
    result
}

/// Pad with available pages up to a whole number of signatures.
pub fn pad_to_complete_signature(mut pages: Vec<PageContent>, per_sig: usize) -> Vec<PageContent> {
    if pages.is_empty() || per_sig == 0 {
        return pages;
    }
    let rem = pages.len() % per_sig;
    if rem == 0 {
        return pages;
    }
    let mut max = pages.iter().map(|p| p.page_number).max().unwrap_or(0);
    for _ in 0..(per_sig - rem) {
        max += 1;
        pages.push(available_page(max));
    }
    pages
}

/// Visual spreads for one signature: page 1 alone on the first spread's
/// recto, middle pages paired, last page alone as the back cover (verso).
/// The first signature keeps the previous first-spread id (stable editor
/// selection) and page 1's items/background if the new page 1 has none.
pub fn spreads_for_signature(pages: &mut [PageContent], signature_number: u32, previous: &[Signature]) -> Vec<Spread> {
    let mut spreads = Vec::new();
    if pages.is_empty() {
        return spreads;
    }
    let prev_first = if signature_number == 1 { previous.first().and_then(|s| s.spreads.first()) } else { None };

    if let Some(prev_recto) = prev_first.and_then(|s| s.recto.as_ref()) {
        let page1 = &mut pages[0];
        if prev_recto.page_number == page1.page_number {
            if prev_recto.items.is_some() && !page1.has_items() {
                page1.items = prev_recto.items.clone();
            }
            if prev_recto.background_fill.is_some() && page1.background_fill.is_none() {
                page1.background_fill = prev_recto.background_fill.clone();
            }
        }
    }

    let first_id = prev_first.map(|s| s.id.clone()).unwrap_or_else(new_id);
    spreads.push(Spread { id: first_id, spread_number: 1, verso: None, recto: Some(pages[0].clone()), ..Default::default() });

    let rest = &pages[1..];
    if let Some((last, middle)) = rest.split_last() {
        for pair in middle.chunks(2) {
            spreads.push(Spread {
                id: new_id(),
                spread_number: spreads.len() as u32 + 1,
                verso: pair.first().cloned(),
                recto: pair.get(1).cloned(),
                ..Default::default()
            });
        }
        if last.page_number != pages[0].page_number {
            spreads.push(Spread {
                id: new_id(),
                spread_number: spreads.len() as u32 + 1,
                verso: Some(last.clone()),
                recto: None,
                ..Default::default()
            });
        }
    }
    spreads
}

pub fn signatures_from_pages(mut pages: Vec<PageContent>, per_sig: usize, previous: &[Signature]) -> Vec<Signature> {
    let per_sig = per_sig.max(1);
    let mut signatures = Vec::new();
    let mut start = 0;
    while start < pages.len() {
        let end = (start + per_sig).min(pages.len());
        let number = (start / per_sig) as u32 + 1;
        let chunk = &mut pages[start..end];
        let spreads = spreads_for_signature(chunk, number, previous);
        signatures.push(Signature {
            id: new_id(),
            signature_number: number,
            spreads,
            page_count: chunk.len() as u32,
            ..Default::default()
        });
        start = end;
    }
    signatures
}

/// A single signature of available pages.
pub fn default_signature(per_sig: usize, previous: &[Signature]) -> Vec<Signature> {
    let pages: Vec<PageContent> = (1..=per_sig as u32).map(available_page).collect();
    signatures_from_pages(pages, per_sig, previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreads_shape() {
        let pages: Vec<PageContent> = (1..=8).map(available_page).collect();
        let sigs = signatures_from_pages(pages, 8, &[]);
        assert_eq!(sigs.len(), 1);
        let s = &sigs[0].spreads;
        assert_eq!(s.len(), 5);
        assert!(s[0].verso.is_none() && s[0].recto.as_ref().unwrap().page_number == 1);
        assert_eq!(s[1].verso.as_ref().unwrap().page_number, 2);
        assert_eq!(s[1].recto.as_ref().unwrap().page_number, 3);
        assert_eq!(s[4].verso.as_ref().unwrap().page_number, 8);
        assert!(s[4].recto.is_none());
    }

    #[test]
    fn merge_places_static_pages() {
        let mut stat = BTreeMap::new();
        let mut p = available_page(2);
        p.page_state = PageState::Static;
        stat.insert(2, p);
        let text: Vec<PageContent> = (1..=2).map(|n| super::super::pagination::create_empty_page(n, false, false)).collect();
        let merged = merge_static_pages(text, &stat);
        let states: Vec<PageState> = merged.iter().map(|p| p.page_state).collect();
        assert_eq!(states, vec![PageState::Text, PageState::Static, PageState::Text]);
        assert_eq!(pad_to_complete_signature(merged, 4).len(), 4);
    }
}
