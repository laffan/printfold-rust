//! GFM-style footnotes (`[^id]` references, `[^id]: body` definitions).
//!
//! Definitions are peeled out of the source before block parsing, and every
//! reference with a matching definition is rewritten to a sentinel
//! (`\x01FN<n>\x01`) that passes through the markdown parser as plain text.
//! The inline parser later turns sentinels into superscript marker spans.

use std::collections::HashMap;

use crate::model::{
    new_id, FootnoteDefinition, PageContent, Section, SectionType, TextSpan,
};

pub const SENTINEL: char = '\u{1}';

/// Layout constants shared by pagination and both renderers.
pub const FOOTNOTE_RULE_GAP: f64 = 4.0;
pub const FOOTNOTE_RULE_THICKNESS: f64 = 0.5;
pub const FOOTNOTE_RULE_WIDTH_RATIO: f64 = 0.3;

/// Superscript markers render at 65% of the surrounding size.
pub fn footnote_marker_font_size(base: f64) -> f64 {
    base * 0.65
}

#[derive(Debug, Clone, Default)]
pub struct FootnoteExtraction {
    pub stripped_markdown: String,
    /// Definitions keyed by user id (only those actually referenced).
    pub definitions: HashMap<String, FootnoteDefinition>,
    /// Same definitions in first-reference order (numbered 1..).
    pub ordered: Vec<FootnoteDefinition>,
}

/// Parse `[^id]: text` at the start of a line.
fn parse_definition_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("[^")?;
    let close = rest.find(']')?;
    let id = &rest[..close];
    if id.is_empty() {
        return None;
    }
    let after = rest[close + 1..].strip_prefix(':')?;
    let body = after.trim_start_matches([' ', '\t']);
    Some((id, body))
}

fn is_continuation(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t']);
    trimmed.len() < line.len() && trimmed.chars().next().is_some_and(|c| !c.is_whitespace())
}

/// Iterate `[^id]` references: yields (start, end, id).
fn find_refs(text: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = text[from..].find("[^") {
        let start = from + pos;
        let id_start = start + 2;
        match text[id_start..].find(']') {
            Some(rel) if rel > 0 => {
                let id_end = id_start + rel;
                out.push((start, id_end + 1, &text[id_start..id_end]));
                from = id_end + 1;
            }
            _ => from = id_start,
        }
    }
    out
}

pub fn extract_footnotes(markdown: &str) -> FootnoteExtraction {
    let lines: Vec<&str> = markdown.split('\n').collect();
    let mut raw_defs: HashMap<String, String> = HashMap::new();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());

    let mut i = 0;
    while i < lines.len() {
        if let Some((id, body)) = parse_definition_line(lines[i]) {
            let id = id.trim().to_string();
            let mut parts = vec![body.to_string()];
            let mut j = i + 1;
            while j < lines.len() {
                let next = lines[j];
                if is_continuation(next) {
                    parts.push(next.trim_start_matches([' ', '\t']).to_string());
                    j += 1;
                } else if next.trim().is_empty() && j + 1 < lines.len() && is_continuation(lines[j + 1]) {
                    parts.push(String::new());
                    j += 1;
                } else {
                    break;
                }
            }
            if !id.is_empty() {
                raw_defs.entry(id).or_insert_with(|| parts.join("\n").trim().to_string());
            }
            i = j;
            continue;
        }
        kept.push(lines[i]);
        i += 1;
    }

    let stripped_markdown = kept.join("\n");
    let mut definitions = HashMap::new();
    let mut ordered = Vec::new();
    for (_, _, id) in find_refs(&stripped_markdown) {
        let id = id.trim();
        if definitions.contains_key(id) {
            continue;
        }
        let Some(content) = raw_defs.get(id) else { continue };
        let def = FootnoteDefinition {
            id: id.to_string(),
            number: ordered.len() as u32 + 1,
            content: content.clone(),
        };
        ordered.push(def.clone());
        definitions.insert(id.to_string(), def);
    }

    FootnoteExtraction { stripped_markdown, definitions, ordered }
}

/// Replace defined `[^id]` references with `\x01FN<n>\x01` sentinels.
pub fn expand_footnote_refs(text: &str, defs: &HashMap<String, FootnoteDefinition>) -> String {
    if defs.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (start, end, id) in find_refs(text) {
        if let Some(def) = defs.get(id.trim()) {
            out.push_str(&text[last..start]);
            out.push_str(&format!("{SENTINEL}FN{}{SENTINEL}", def.number));
            last = end;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Positions of sentinels in `text`: (start, end, number).
fn find_sentinels(text: &str) -> Vec<(usize, usize, u32)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 1 && text[i + 1..].starts_with("FN") {
            let digits_start = i + 3;
            let mut j = digits_start;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > digits_start && j < bytes.len() && bytes[j] == 1 {
                if let Ok(n) = text[digits_start..j].parse() {
                    out.push((i, j + 1, n));
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// Footnote numbers referenced (via sentinels) in a text run.
pub fn footnote_numbers_in_text(text: &str) -> Vec<u32> {
    find_sentinels(text).into_iter().map(|(_, _, n)| n).collect()
}

/// Remove sentinels, leaving the visible number (for plain-text contexts).
pub fn sentinels_to_numbers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (start, end, n) in find_sentinels(text) {
        out.push_str(&text[last..start]);
        out.push_str(&n.to_string());
        last = end;
    }
    out.push_str(&text[last..]);
    out
}

/// Split a styled span on sentinels; numeric pieces become marker spans.
pub fn split_span_on_sentinels(span: &TextSpan) -> Vec<TextSpan> {
    let found = find_sentinels(&span.text);
    if found.is_empty() {
        return vec![span.clone()];
    }
    let mut out = Vec::new();
    let mut last = 0;
    for (start, end, n) in found {
        if start > last {
            out.push(span.with_text(&span.text[last..start]));
        }
        let mut marker = span.with_text(n.to_string());
        marker.footnote_number = Some(n);
        out.push(marker);
        last = end;
    }
    if last < span.text.len() {
        out.push(span.with_text(&span.text[last..]));
    }
    out
}

/// Endnote block: a page break (`hr`) followed by one section per note.
pub fn build_endnote_sections(footnotes: &[FootnoteDefinition]) -> Vec<Section> {
    if footnotes.is_empty() {
        return Vec::new();
    }
    let mut sections = vec![Section {
        id: new_id(),
        section_type: SectionType::Hr,
        raw_markdown: "---".into(),
        ..Default::default()
    }];
    for f in footnotes {
        let prefixed = format!("{}. {}", f.number, f.content);
        sections.push(Section {
            id: new_id(),
            section_type: SectionType::Endnote,
            endnote_number: Some(f.number),
            content: prefixed.clone(),
            raw_markdown: prefixed,
            ..Default::default()
        });
    }
    sections
}

/// Insert endnote groups before each H1 (and at the end) for the footnotes
/// first referenced in the preceding chapter.
pub fn inject_chapter_endnotes(sections: Vec<Section>, ordered: &[FootnoteDefinition]) -> Vec<Section> {
    if ordered.is_empty() {
        return sections;
    }
    let by_number: HashMap<u32, &FootnoteDefinition> = ordered.iter().map(|f| (f.number, f)).collect();
    let mut result = Vec::with_capacity(sections.len());
    let mut pending: Vec<FootnoteDefinition> = Vec::new();

    for section in sections {
        let is_h1 = section.section_type == SectionType::Heading && section.level == Some(1);
        if is_h1 && !pending.is_empty() {
            result.extend(build_endnote_sections(&pending));
            pending.clear();
        }
        if let Some(refs) = &section.footnote_refs {
            for n in refs {
                if let Some(def) = by_number.get(n) {
                    if !pending.iter().any(|p| p.number == def.number) {
                        pending.push((*def).clone());
                    }
                }
            }
        }
        result.push(section);
    }
    result.extend(build_endnote_sections(&pending));
    result
}

/// Footnote numbers whose markers landed on a page (sorted, unique).
/// Rich sections use only the lines that landed here; a split paragraph's
/// `footnoteRefs` covers the whole paragraph and would over-claim.
pub fn collect_footnote_numbers_on_page(page: &PageContent) -> Vec<u32> {
    let mut seen = Vec::new();
    for section in &page.sections {
        if matches!(section.section_type, SectionType::Endnote | SectionType::EndnoteHeader) {
            continue;
        }
        let rich = section.rich_lines();
        if !rich.is_empty() {
            for line in rich {
                for span in &line.spans {
                    if let Some(n) = span.footnote_number {
                        if !seen.contains(&n) {
                            seen.push(n);
                        }
                    }
                }
            }
            continue;
        }
        if let Some(refs) = &section.footnote_refs {
            for &n in refs {
                if !seen.contains(&n) {
                    seen.push(n);
                }
            }
        }
    }
    seen.sort_unstable();
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_numbers_in_reference_order() {
        let md = "Text[^b] and[^a].\n\n[^a]: Alpha\n[^b]: Beta\n    continued\n\nAfter";
        let ex = extract_footnotes(md);
        assert_eq!(ex.stripped_markdown, "Text[^b] and[^a].\n\n\nAfter");
        assert_eq!(ex.ordered.len(), 2);
        assert_eq!(ex.ordered[0].id, "b");
        assert_eq!(ex.ordered[0].number, 1);
        assert_eq!(ex.ordered[0].content, "Beta\ncontinued");
        assert_eq!(ex.ordered[1].content, "Alpha");
        let expanded = expand_footnote_refs(&ex.stripped_markdown, &ex.definitions);
        assert_eq!(footnote_numbers_in_text(&expanded), vec![1, 2]);
        assert_eq!(sentinels_to_numbers(&expanded), "Text1 and2.\n\n\nAfter");
    }

    #[test]
    fn undefined_refs_stay_literal() {
        let ex = extract_footnotes("Hi[^x]");
        assert!(ex.ordered.is_empty());
        assert_eq!(expand_footnote_refs("Hi[^x]", &ex.definitions), "Hi[^x]");
    }

    #[test]
    fn splits_sentinel_spans() {
        let span = TextSpan { text: format!("a{SENTINEL}FN3{SENTINEL}b"), bold: true, ..Default::default() };
        let parts = split_span_on_sentinels(&span);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1].text, "3");
        assert_eq!(parts[1].footnote_number, Some(3));
        assert!(parts[2].bold);
    }
}
