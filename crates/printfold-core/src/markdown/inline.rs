//! Inline markdown → styled [`TextSpan`]s.
//!
//! Handles `**bold**`, `*italic*`, `` `code` ``, `~~strike~~`, links,
//! Obsidian `==highlight==` and footnote sentinels. Each hard line of a
//! paragraph is parsed on its own (single newlines are hard breaks in
//! PrintFold, so poetry keeps its shape).

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::footnotes::split_span_on_sentinels;
use crate::model::TextSpan;

/// Private-use markers standing in for `==` while the line is parsed.
const HL_START: char = '\u{E000}';
const HL_END: char = '\u{E001}';

pub(crate) fn parser_options() -> Options {
    Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS
}

/// Replace `==text==` pairs (text without `=`) with highlight markers.
fn mark_highlights(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("==") {
        let after = &rest[start + 2..];
        let inner_len = after.find('=').unwrap_or(after.len());
        if inner_len > 0 && after[inner_len..].starts_with("==") {
            out.push_str(&rest[..start]);
            out.push(HL_START);
            out.push_str(&after[..inner_len]);
            out.push(HL_END);
            rest = &after[inner_len + 2..];
        } else {
            out.push_str(&rest[..start + 1]);
            rest = &rest[start + 1..];
        }
    }
    out.push_str(rest);
    out
}

/// Escape a leading character that would otherwise start a block construct
/// when a single line is fed to the block parser (`# x`, `> x`, `- x`,
/// `1. x`, fences, reference definitions). The original renderer kept such
/// characters as literal text; this preserves that.
fn escape_block_start(line: &str) -> String {
    let bytes = line.as_bytes();
    let Some(&first) = bytes.first() else { return String::new() };
    let second = bytes.get(1).copied();
    let needs_escape = match first {
        b'#' | b'>' | b'<' | b'|' => true,
        b'-' | b'+' | b'*' => matches!(second, None | Some(b' ') | Some(b'\t'))
            || line.chars().all(|c| c == first as char || c == ' '),
        b'`' => line.starts_with("```"),
        b'~' => line.starts_with("~~~"),
        b'=' => line.chars().all(|c| c == '=' || c == ' '),
        b'[' => line.contains("]:"),
        _ => false,
    };
    if needs_escape {
        return format!("\\{line}");
    }
    // Ordered list marker: digits followed by '.' or ')'.
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 && digits <= 9 {
        if let Some(&m) = bytes.get(digits) {
            if (m == b'.' || m == b')') && matches!(bytes.get(digits + 1), None | Some(b' ') | Some(b'\t')) {
                return format!("{}\\{}", &line[..digits], &line[digits..]);
            }
        }
    }
    line.to_string()
}

#[derive(Default, Clone)]
struct StyleState {
    bold: u32,
    italic: u32,
    strike: u32,
    links: Vec<String>,
}

impl StyleState {
    fn span(&self, text: String) -> TextSpan {
        TextSpan {
            text,
            bold: self.bold > 0,
            italic: self.italic > 0,
            strikethrough: self.strike > 0,
            link: self.links.last().cloned(),
            ..Default::default()
        }
    }
}

/// Inline events of `source` flattened to spans (block containers ignored).
fn events_to_spans(source: &str) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    let mut st = StyleState::default();
    let mut image_depth = 0u32;
    for event in Parser::new_ext(source, parser_options()) {
        match event {
            Event::Start(Tag::Strong) => st.bold += 1,
            Event::End(TagEnd::Strong) => st.bold = st.bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => st.italic += 1,
            Event::End(TagEnd::Emphasis) => st.italic = st.italic.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => st.strike += 1,
            Event::End(TagEnd::Strikethrough) => st.strike = st.strike.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => st.links.push(dest_url.to_string()),
            Event::End(TagEnd::Link) => {
                st.links.pop();
            }
            Event::Start(Tag::Image { .. }) => image_depth += 1,
            Event::End(TagEnd::Image) => image_depth = image_depth.saturating_sub(1),
            Event::Text(t) => spans.push(st.span(t.to_string())),
            Event::Code(t) => {
                let mut s = st.span(t.to_string());
                s.code = true;
                spans.push(s);
            }
            Event::Html(t) | Event::InlineHtml(t) => spans.push(st.span(t.trim_end_matches('\n').to_string())),
            Event::SoftBreak | Event::HardBreak => spans.push(st.span("\n".into())),
            Event::TaskListMarker(_) => {}
            _ => {}
        }
    }
    let _ = image_depth;
    spans
}

/// Split spans on highlight markers, flagging the enclosed text.
fn apply_highlight_markers(spans: Vec<TextSpan>) -> Vec<TextSpan> {
    let mut out = Vec::with_capacity(spans.len());
    let mut active = false;
    for span in spans {
        if !span.text.contains([HL_START, HL_END]) {
            let mut s = span;
            s.highlight = active;
            out.push(s);
            continue;
        }
        let mut buf = String::new();
        for c in span.text.chars() {
            if c == HL_START || c == HL_END {
                if !buf.is_empty() {
                    let mut s = span.with_text(std::mem::take(&mut buf));
                    s.highlight = active;
                    out.push(s);
                }
                active = c == HL_START;
            } else {
                buf.push(c);
            }
        }
        if !buf.is_empty() {
            let mut s = span.with_text(buf);
            s.highlight = active;
            out.push(s);
        }
    }
    out
}

/// Merge neighbouring spans with identical styling (footnote markers never merge).
pub fn merge_adjacent_spans(spans: Vec<TextSpan>) -> Vec<TextSpan> {
    let mut result: Vec<TextSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        match result.last_mut() {
            Some(last) if last.same_style(&span) => last.text.push_str(&span.text),
            _ => {
                if result.last().is_some_and(|l| l.text.is_empty()) {
                    result.pop();
                }
                result.push(span);
            }
        }
    }
    if result.last().is_some_and(|l| l.text.is_empty()) {
        result.pop();
    }
    result
}

/// Parse one line of inline markdown into styled spans.
pub fn parse_inline_markdown(text: &str) -> Vec<TextSpan> {
    let marked = mark_highlights(text);
    let source = escape_block_start(&marked);
    let spans = events_to_spans(&source);
    let spans = apply_highlight_markers(spans);
    let spans: Vec<TextSpan> = spans.iter().flat_map(split_span_on_sentinels).collect();
    merge_adjacent_spans(spans)
}

/// Plain text of inline markdown (markers removed, sentinels kept).
pub fn inline_plain_text(text: &str) -> String {
    parse_inline_markdown(text).into_iter().map(|s| s.text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(spans: &[TextSpan]) -> Vec<&str> {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn basic_styles() {
        let spans = parse_inline_markdown("plain **bold** and *it* `code` ~~del~~");
        assert_eq!(texts(&spans), vec!["plain ", "bold", " and ", "it", " ", "code", " ", "del"]);
        assert!(spans[1].bold);
        assert!(spans[3].italic);
        assert!(spans[5].code);
        assert!(spans[7].strikethrough);
    }

    #[test]
    fn nested_and_links() {
        let spans = parse_inline_markdown("***both*** [link](http://x)");
        assert!(spans[0].bold && spans[0].italic);
        assert_eq!(spans.last().unwrap().link.as_deref(), Some("http://x"));
    }

    #[test]
    fn highlight_after_markup() {
        let spans = parse_inline_markdown("**a** ==b== c");
        assert_eq!(texts(&spans), vec!["a", " ", "b", " c"]);
        assert!(spans[2].highlight);
        assert!(!spans[3].highlight);
    }

    #[test]
    fn block_markers_stay_literal() {
        assert_eq!(inline_plain_text("# not a heading"), "# not a heading");
        assert_eq!(inline_plain_text("1. not a list"), "1. not a list");
        assert_eq!(inline_plain_text("- dash"), "- dash");
        assert_eq!(inline_plain_text("> quote"), "> quote");
        assert_eq!(inline_plain_text("**bold** start"), "bold start");
    }

    #[test]
    fn entities_decode() {
        assert_eq!(inline_plain_text("a &amp; b &mdash; c"), "a & b \u{2014} c");
    }

    #[test]
    fn footnote_markers() {
        let spans = parse_inline_markdown("word\u{1}FN2\u{1} more");
        assert_eq!(spans[1].footnote_number, Some(2));
        assert_eq!(spans[1].text, "2");
    }
}
