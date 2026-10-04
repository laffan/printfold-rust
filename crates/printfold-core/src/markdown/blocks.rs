//! Block-level markdown parsing into document [`Section`]s.

use std::collections::HashMap;
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};

use super::footnotes::{expand_footnote_refs, extract_footnotes, footnote_numbers_in_text};
use super::inline::parser_options;
use crate::model::{new_id, FootnoteDefinition, Section, SectionType};

#[derive(Debug, Clone, Default)]
pub struct ParsedMarkdown {
    pub sections: Vec<Section>,
    /// Footnote definitions in reference order.
    pub footnotes: Vec<FootnoteDefinition>,
    pub footnotes_by_id: HashMap<String, FootnoteDefinition>,
}

/// One top-level block: its opening tag, inner events and source range.
struct Block<'a> {
    tag: Tag<'a>,
    events: Vec<Event<'a>>,
    range: Range<usize>,
}

fn top_level_blocks(source: &str) -> Vec<Block<'_>> {
    let mut blocks = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<Block> = None;
    for (event, range) in Parser::new_ext(source, parser_options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    current = Some(Block { tag, events: Vec::new(), range });
                } else if let Some(b) = current.as_mut() {
                    b.events.push(Event::Start(tag));
                }
                depth += 1;
            }
            Event::End(end) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(b) = current.take() {
                        blocks.push(b);
                    }
                } else if let Some(b) = current.as_mut() {
                    b.events.push(Event::End(end));
                }
            }
            Event::Rule if depth == 0 => blocks.push(Block { tag: Tag::Paragraph, events: vec![Event::Rule], range }),
            other => {
                if let Some(b) = current.as_mut() {
                    b.events.push(other);
                }
            }
        }
    }
    blocks
}

/// Plain text of inline events (markup removed). Breaks become newlines.
fn inline_text(events: &[Event]) -> String {
    let mut out = String::new();
    for e in events {
        match e {
            Event::Text(t) | Event::Code(t) => out.push_str(t),
            Event::InlineHtml(t) | Event::Html(t) => out.push_str(t),
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            _ => {}
        }
    }
    out
}

fn heading_level(level: HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// A paragraph that consists of exactly one image becomes an image section.
fn solo_image(events: &[Event]) -> Option<(String, String)> {
    let mut iter = events.iter().filter(|e| !matches!(e, Event::Text(t) if t.trim().is_empty()));
    let Some(Event::Start(Tag::Image { dest_url, title, .. })) = iter.next() else { return None };
    let mut alt = String::new();
    for e in iter.by_ref() {
        match e {
            Event::End(TagEnd::Image) => break,
            Event::Text(t) | Event::Code(t) => alt.push_str(t),
            _ => {}
        }
    }
    if iter.next().is_some() {
        return None;
    }
    let caption = if alt.is_empty() { title.to_string() } else { alt };
    Some((caption, dest_url.to_string()))
}

/// Render list items as lines: `• item` or `n. item`, nested lists indented.
fn list_text(start: Option<u64>, events: &[Event]) -> String {
    let mut lines: Vec<String> = Vec::new();
    // Stack of (ordered, next number) per list depth.
    let mut stack: Vec<(bool, u64)> = vec![(start.is_some(), 1)];
    let mut current: Option<String> = None;
    let flush = |current: &mut Option<String>, lines: &mut Vec<String>| {
        if let Some(line) = current.take() {
            lines.push(line.trim_end().to_string());
        }
    };
    for e in events {
        match e {
            Event::Start(Tag::List(s)) => {
                flush(&mut current, &mut lines);
                stack.push((s.is_some(), 1));
            }
            Event::End(TagEnd::List(_)) => {
                flush(&mut current, &mut lines);
                stack.pop();
            }
            Event::Start(Tag::Item) => {
                flush(&mut current, &mut lines);
                // Nesting is shown by the bullet glyph (• ◦ ▪) plus a
                // two-space indent; wrapping collapses leading spaces, so
                // the glyph is what keeps levels distinguishable.
                let depth = stack.len().saturating_sub(1);
                let (ordered, n) = stack.last_mut().map(|t| (t.0, &mut t.1)).unwrap();
                let bullet = match depth {
                    0 => "\u{2022}",
                    1 => "\u{25E6}",
                    _ => "\u{25AA}",
                };
                let marker = if ordered { format!("{n}. ") } else { format!("{bullet} ") };
                *n += 1;
                current = Some(format!("{}{}", "  ".repeat(depth), marker));
            }
            Event::End(TagEnd::Item) => flush(&mut current, &mut lines),
            Event::End(TagEnd::Paragraph) => {
                if let Some(line) = current.as_mut() {
                    if !line.ends_with(' ') {
                        line.push('\n');
                    }
                }
            }
            Event::Text(t) | Event::Code(t) | Event::InlineHtml(t) => {
                current.get_or_insert_with(String::new).push_str(t);
            }
            Event::SoftBreak | Event::HardBreak => {
                current.get_or_insert_with(String::new).push('\n');
            }
            _ => {}
        }
    }
    flush(&mut current, &mut lines);
    lines.join("\n").replace("\n\n", "\n")
}

/// Paragraph texts of a blockquote joined with newlines.
fn blockquote_text(events: &[Event]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut buf = String::new();
    for e in events {
        match e {
            Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Heading(_)) | Event::End(TagEnd::CodeBlock) => {
                parts.push(std::mem::take(&mut buf).trim_end_matches('\n').to_string());
            }
            Event::Text(t) | Event::Code(t) | Event::InlineHtml(t) => buf.push_str(t),
            Event::SoftBreak | Event::HardBreak => buf.push('\n'),
            _ => {}
        }
    }
    if !buf.is_empty() {
        parts.push(buf);
    }
    parts.join("\n")
}

/// Strip blockquote markers (`> `) from raw quote source so the inline
/// parser sees the quoted text itself.
pub fn strip_blockquote_markers(raw: &str) -> String {
    raw.split('\n')
        .map(|line| {
            let mut l = line.trim_start();
            while let Some(rest) = l.strip_prefix('>') {
                l = rest.strip_prefix(' ').unwrap_or(rest).trim_start_matches('\t');
            }
            l
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn block_to_section(block: &Block, source: &str) -> Option<Section> {
    // Include leading indentation on the block's first line (pulldown's
    // range for indented code starts after it), so `rawMarkdown` is the
    // literal source like the original parser's.
    let mut start = block.range.start;
    while start > 0 && matches!(source.as_bytes()[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    let raw = source[start..block.range.end].trim_end_matches(['\n', '\r']).to_string();
    let base = |section_type: SectionType, content: String| Section {
        id: new_id(),
        section_type,
        content,
        raw_markdown: raw.clone(),
        ..Default::default()
    };
    match &block.tag {
        Tag::Heading { level, .. } => {
            let mut s = base(SectionType::Heading, inline_text(&block.events).trim().to_string());
            s.level = Some(heading_level(*level));
            Some(s)
        }
        Tag::Paragraph if matches!(block.events.first(), Some(Event::Rule)) => Some(base(SectionType::Hr, String::new())),
        Tag::Paragraph => {
            if let Some((caption, href)) = solo_image(&block.events) {
                let mut s = base(SectionType::Image, caption);
                s.image_ref = Some(href);
                return Some(s);
            }
            Some(base(SectionType::Paragraph, inline_text(&block.events)))
        }
        Tag::List(start) => Some(base(SectionType::List, list_text(*start, &block.events))),
        Tag::CodeBlock(kind) => {
            let mut code = inline_text(&block.events);
            if code.ends_with('\n') {
                code.pop();
            }
            if matches!(kind, CodeBlockKind::Indented) {
                code = code.trim_end_matches('\n').to_string();
            }
            Some(base(SectionType::Code, code))
        }
        Tag::BlockQuote(_) => Some(base(SectionType::Blockquote, blockquote_text(&block.events))),
        // Tables, HTML blocks, footnote definitions etc. are not laid out
        // (the original skipped them too).
        _ => None,
    }
}

/// Parse markdown into sections, extracting footnotes first. Inline `[^id]`
/// references become sentinels in `content`/`rawMarkdown`.
pub fn parse_markdown_with_footnotes(markdown: &str) -> ParsedMarkdown {
    let extraction = extract_footnotes(markdown);
    let expanded = expand_footnote_refs(&extraction.stripped_markdown, &extraction.definitions);

    let mut sections = Vec::new();
    for block in top_level_blocks(&expanded) {
        let Some(mut section) = block_to_section(&block, &expanded) else { continue };
        let mut refs = footnote_numbers_in_text(&section.content);
        refs.extend(footnote_numbers_in_text(&section.raw_markdown));
        if !refs.is_empty() {
            let mut unique = Vec::new();
            for r in refs {
                if !unique.contains(&r) {
                    unique.push(r);
                }
            }
            section.footnote_refs = Some(unique);
        }
        sections.push(section);
    }

    ParsedMarkdown { sections, footnotes: extraction.ordered, footnotes_by_id: extraction.definitions }
}

pub fn parse_markdown(markdown: &str) -> Vec<Section> {
    parse_markdown_with_footnotes(markdown).sections
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(sections: &[Section]) -> Vec<SectionType> {
        sections.iter().map(|s| s.section_type).collect()
    }

    #[test]
    fn block_types() {
        let md = "# Title **bold**\n\nPara one\nline two.\n\n> quote **b**\n> more\n\n- a **x**\n- b\n  - nested\n\n1. one\n2. two\n\n```js\ncode\n```\n\n---\n\n![cap](img.png)\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nlast";
        let s = parse_markdown(md);
        use SectionType::*;
        assert_eq!(kinds(&s), vec![Heading, Paragraph, Blockquote, List, List, Code, Hr, Image, Paragraph]);
        assert_eq!(s[0].content, "Title bold");
        assert_eq!(s[0].level, Some(1));
        assert_eq!(s[1].content, "Para one\nline two.");
        assert_eq!(s[1].raw_markdown, "Para one\nline two.");
        assert_eq!(s[2].content, "quote b\nmore");
        assert_eq!(s[3].content, "\u{2022} a x\n\u{2022} b\n  \u{25E6} nested");
        assert_eq!(s[4].content, "1. one\n2. two");
        assert_eq!(s[5].content, "code");
        assert_eq!(s[7].content, "cap");
        assert_eq!(s[7].image_ref.as_deref(), Some("img.png"));
        assert_eq!(s[8].content, "last");
    }

    #[test]
    fn footnote_refs_annotated() {
        let p = parse_markdown_with_footnotes("A[^1] b[^2].\n\n[^1]: one\n[^2]: two");
        assert_eq!(p.footnotes.len(), 2);
        assert_eq!(p.sections.len(), 1);
        assert_eq!(p.sections[0].footnote_refs, Some(vec![1, 2]));
    }

    #[test]
    fn strips_quote_markers() {
        assert_eq!(strip_blockquote_markers("> a\n>b\n> > c"), "a\nb\nc");
    }
}
