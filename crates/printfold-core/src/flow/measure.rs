//! Section measurement and line wrapping (port of `textFlow/measurement.ts`).

use std::collections::HashMap;

use crate::fonts::{FontRegistry, FontRequest, WidthCache};
use crate::markdown::footnotes::{
    footnote_marker_font_size, FOOTNOTE_RULE_GAP, FOOTNOTE_RULE_THICKNESS,
};
use crate::markdown::{merge_adjacent_spans, parse_inline_markdown, strip_blockquote_markers};
use crate::model::{
    FontOptions, FontStyle, FootnoteDefinition, LayoutOptions, RichTextLine, Section, SectionType,
    TextSpan,
};
use crate::text::{apply_text_transform, is_js_space, split_ws_js, words};

/// Wrapping keeps a 2% safety margin, as the original did, so small
/// differences between engines never push text past the margin.
pub const SAFE_WIDTH_RATIO: f64 = 0.98;

/// Text measurement bound to a font registry and its width cache.
pub struct Measurer<'a> {
    pub fonts: &'a mut FontRegistry,
    pub cache: &'a mut WidthCache,
    /// Pixel sizes of project images keyed by lower-cased file name.
    pub image_sizes: HashMap<String, (f64, f64)>,
    /// Content height of a full page (images are scaled to fit it).
    pub max_image_height: f64,
}

/// Image captions render like the editor's: 9px italic Arial, centred.
pub const CAPTION_FONT_SIZE: f64 = 9.0;
pub const CAPTION_GAP: f64 = 4.0;
/// Placeholder box for images that are not in the project.
pub const MISSING_IMAGE_HEIGHT: f64 = 100.0;
pub const MISSING_IMAGE_MAX_WIDTH: f64 = 200.0;

pub fn caption_style() -> FontStyle {
    FontStyle::new("Arial", CAPTION_FONT_SIZE, "normal", "italic")
}

/// Look up an image by markdown reference: exact (case-insensitive) file
/// name first, then the reference's last path component, URL-decoded.
pub fn find_image_key(reference: &str, names: impl Iterator<Item = String>) -> Option<String> {
    let lower = reference.trim().to_lowercase();
    let base = lower.rsplit(['/', '\\']).next().unwrap_or(&lower).replace("%20", " ");
    let names: Vec<String> = names.collect();
    names.iter().find(|n| **n == lower).or_else(|| names.iter().find(|n| **n == base)).cloned()
}

impl<'a> Measurer<'a> {
    pub fn new(fonts: &'a mut FontRegistry, cache: &'a mut WidthCache) -> Self {
        Self { fonts, cache, image_sizes: HashMap::new(), max_image_height: f64::INFINITY }
    }

    /// Lay out an image section: full content width at the image's aspect
    /// ratio, scaled down to fit a page, with the caption wrapped below.
    fn measure_image(&mut self, section: &Section, content_width: f64, layout: &LayoutOptions) -> Section {
        let mut m = section.clone();
        let reference = section.image_ref.clone().unwrap_or_default();
        let key = find_image_key(&reference, self.image_sizes.keys().cloned());
        let caption = section.content.trim().to_string();
        let block = match key.and_then(|k| self.image_sizes.get(&k).copied()) {
            Some((w, h)) if w > 0.0 && h > 0.0 && content_width > 0.0 => {
                let mut dw = content_width;
                let mut dh = dw * h / w;
                let style = caption_style();
                let mut caption_lines = if caption.is_empty() { Vec::new() } else { self.wrap_text(&caption, dw, &style) };
                let caption_h = |n: usize| if n == 0 { 0.0 } else { CAPTION_GAP + n as f64 * CAPTION_FONT_SIZE };
                let limit = self.max_image_height;
                if dh + caption_h(caption_lines.len()) > limit {
                    let avail = (limit - caption_h(caption_lines.len())).max(limit * 0.25);
                    dh = avail;
                    dw = dh * w / h;
                    if !caption.is_empty() {
                        caption_lines = self.wrap_text(&caption, dw, &style);
                    }
                }
                m.image_width = Some(dw);
                m.image_height = Some(dh);
                let block = dh + caption_h(caption_lines.len());
                m.line_heights = Some(vec![CAPTION_FONT_SIZE; caption_lines.len()]);
                m.lines = Some(caption_lines);
                block
            }
            _ => {
                m.image_width = None;
                m.image_height = None;
                m.lines = Some(Vec::new());
                m.line_heights = Some(Vec::new());
                MISSING_IMAGE_HEIGHT + 10.0
            }
        };
        m.measured_height = Some(block + layout.paragraph_spacing);
        m
    }

    pub fn text_width(&mut self, text: &str, style: &FontStyle) -> f64 {
        let req = FontRequest::new(&style.font_family, style.is_bold(), style.is_italic());
        self.fonts.measure(self.cache, text, req, style.font_size)
    }

    /// Greedy word wrap of plain text (JS-compatible word splitting).
    pub fn wrap_text(&mut self, text: &str, max_width: f64, style: &FontStyle) -> Vec<String> {
        let safe = max_width * SAFE_WIDTH_RATIO;
        let mut lines = Vec::new();
        let mut current = String::new();
        for word in split_ws_js(text) {
            let test = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
            if self.text_width(&test, style) <= safe {
                current = test;
                continue;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            if self.text_width(word, style) > safe {
                let mut char_line = String::new();
                for c in word.chars() {
                    let candidate = format!("{char_line}{c}");
                    if self.text_width(&candidate, style) <= safe {
                        char_line = candidate;
                    } else {
                        if !char_line.is_empty() {
                            lines.push(std::mem::take(&mut char_line));
                        }
                        char_line = c.to_string();
                    }
                }
                current = char_line;
            } else {
                current = word.to_string();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
        lines
    }

    pub fn span_width(&mut self, span: &TextSpan, base: &FontStyle, fonts: &FontOptions) -> f64 {
        let style = span_font_style(base, span, fonts);
        self.text_width(&span.text, &style)
    }

    pub fn rich_line_width(&mut self, line: &RichTextLine, base: &FontStyle, fonts: &FontOptions) -> f64 {
        line.spans.iter().map(|s| self.span_width(s, base, fonts)).sum()
    }

    /// Wrap inline markdown into styled lines. Blank-line separated blocks
    /// are wrapped independently; single newlines are hard breaks.
    pub fn wrap_rich_text(
        &mut self,
        markdown: &str,
        max_width: f64,
        base: &FontStyle,
        fonts: &FontOptions,
    ) -> Vec<RichTextLine> {
        let safe = max_width * SAFE_WIDTH_RATIO;
        let mut all = Vec::new();
        for paragraph in split_paragraphs(markdown) {
            if paragraph.trim().is_empty() {
                continue;
            }
            for hard_line in paragraph.split('\n') {
                let trimmed = hard_line.trim();
                if trimmed.is_empty() {
                    all.push(RichTextLine::empty());
                    continue;
                }
                let spans = parse_inline_markdown(trimmed);
                if spans.is_empty() {
                    all.push(RichTextLine::empty());
                    continue;
                }
                let transform = base.text_transform.as_deref();
                let spans: Vec<TextSpan> = match transform {
                    Some(t) if t != "none" => spans
                        .iter()
                        .map(|s| s.with_text(apply_text_transform(&s.text, Some(t))))
                        .collect(),
                    _ => spans,
                };
                all.extend(self.wrap_spans(&spans, safe, base, fonts));
            }
        }
        all
    }

    pub(crate) fn wrap_spans(&mut self, spans: &[TextSpan], safe: f64, base: &FontStyle, fonts: &FontOptions) -> Vec<RichTextLine> {
        let units = spans_to_units(spans);
        if units.is_empty() {
            return vec![RichTextLine::empty()];
        }
        let mut lines = Vec::new();
        let mut current: Vec<TextSpan> = Vec::new();
        let mut current_width = 0.0;

        for unit in &units {
            let unit_width: f64 = unit.iter().map(|p| self.span_width(p, base, fonts)).sum();
            let first_style = span_font_style(base, &unit[0], fonts);
            let added = if current.is_empty() { unit_width } else { self.text_width(" ", &first_style) + unit_width };

            if current_width + added <= safe {
                let mut pieces = unit.iter();
                let first = pieces.next().unwrap();
                match current.last_mut() {
                    Some(last) if last.same_style(first) => {
                        last.text.push(' ');
                        last.text.push_str(&first.text);
                    }
                    Some(last) => {
                        last.text.push(' ');
                        current.push(first.clone());
                    }
                    None => current.push(first.clone()),
                }
                current.extend(pieces.cloned());
                current_width += added;
                continue;
            }

            if !current.is_empty() {
                lines.push(RichTextLine { spans: merge_adjacent_spans(std::mem::take(&mut current)) });
            }
            if unit_width > safe {
                let mut broken = self.break_long_unit(unit, safe, base, fonts);
                let last = broken.pop();
                lines.extend(broken);
                match last {
                    Some(l) => {
                        current_width = self.rich_line_width(&l, base, fonts);
                        current = l.spans;
                    }
                    None => current_width = 0.0,
                }
            } else {
                current = unit.clone();
                current_width = unit_width;
            }
        }
        if !current.is_empty() {
            lines.push(RichTextLine { spans: merge_adjacent_spans(current) });
        }
        lines
    }

    /// Break a word that is wider than the line, character by character.
    fn break_long_unit(&mut self, unit: &[TextSpan], max: f64, base: &FontStyle, fonts: &FontOptions) -> Vec<RichTextLine> {
        let mut lines = Vec::new();
        let mut line: Vec<TextSpan> = Vec::new();
        let mut line_width = 0.0;
        for piece in unit {
            for c in piece.text.chars() {
                let ch = piece.with_text(c.to_string());
                let w = self.span_width(&ch, base, fonts);
                if line_width + w > max && !line.is_empty() {
                    lines.push(RichTextLine { spans: merge_adjacent_spans(std::mem::take(&mut line)) });
                    line_width = 0.0;
                }
                match line.last_mut() {
                    Some(last) if last.same_style(&ch) => last.text.push(c),
                    _ => line.push(ch),
                }
                line_width += w;
            }
        }
        if !line.is_empty() {
            lines.push(RichTextLine { spans: merge_adjacent_spans(line) });
        }
        lines
    }

    /// Re-wrap the lines of `section` from line `from` onward for a new
    /// content width (used when a paragraph continues into a slot of a
    /// different width, e.g. from a page into a text-flow region).
    pub fn rewrap_section(&mut self, section: &Section, from: usize, width: f64, fonts: &FontOptions, layout: &LayoutOptions) -> Section {
        let style = font_style_for_section(section.section_type, section.level, fonts).clone();
        let line_height = layout.line_height * style.font_size;
        let mut out = section.clone();
        let rich = section.rich_lines();
        if !rich.is_empty() {
            let mut spans: Vec<TextSpan> = Vec::new();
            for (i, line) in rich[from.min(rich.len())..].iter().enumerate() {
                if i > 0 {
                    if let Some(last) = spans.last_mut() {
                        last.text.push(' ');
                    }
                }
                spans.extend(line.spans.iter().cloned());
            }
            let wrapped = self.wrap_spans(&merge_adjacent_spans(spans), width * SAFE_WIDTH_RATIO, &style, fonts);
            out.lines = Some(wrapped.iter().map(|l| l.plain_text()).collect());
            out.rich_lines = Some(wrapped);
        } else {
            let text = section.lines()[from.min(section.lines().len())..].join(" ");
            out.lines = Some(self.wrap_text(&text, width, &style));
            out.rich_lines = None;
        }
        let n = out.lines().len();
        out.line_heights = Some(vec![line_height; n]);
        out.measured_height = Some(n as f64 * line_height + layout.paragraph_spacing);
        out
    }

    /// Rough wrapped-line count used for footnote reservations.
    pub fn estimate_wrapped_lines(&mut self, text: &str, content_width: f64, style: &FontStyle) -> usize {
        if text.is_empty() {
            return 1;
        }
        let safe = content_width * SAFE_WIDTH_RATIO;
        let mut total = 0;
        for hard_line in text.split('\n') {
            let ws = words(hard_line);
            if ws.is_empty() {
                total += 1;
                continue;
            }
            let mut current = String::new();
            let mut count = 0;
            for w in ws {
                let candidate = if current.is_empty() { w.to_string() } else { format!("{current} {w}") };
                if self.text_width(&candidate, style) <= safe {
                    current = candidate;
                } else {
                    if !current.is_empty() {
                        count += 1;
                    }
                    current = w.to_string();
                }
            }
            if !current.is_empty() {
                count += 1;
            }
            total += count.max(1);
        }
        total
    }

    /// Height reserved at the bottom of a page for its footnote block.
    pub fn footnote_block_height(
        &mut self,
        footnotes: &[FootnoteDefinition],
        content_width: f64,
        fonts: &FontOptions,
        layout: &LayoutOptions,
    ) -> f64 {
        if footnotes.is_empty() {
            return 0.0;
        }
        let style = &fonts.footnote;
        let line_height = style.line_height.unwrap_or(layout.line_height) * style.font_size;
        let gap = fonts.footnote_gap.unwrap_or(0.0);
        let rule = FOOTNOTE_RULE_GAP + FOOTNOTE_RULE_THICKNESS + FOOTNOTE_RULE_GAP;
        let lines: usize = footnotes
            .iter()
            .map(|f| self.estimate_wrapped_lines(&format!("{}. {}", f.number, f.content), content_width, style))
            .sum();
        rule + lines as f64 * line_height + footnotes.len().saturating_sub(1) as f64 * gap
    }

    /// Measure a section for a given content width.
    pub fn measure_section(
        &mut self,
        section: &Section,
        content_width: f64,
        fonts: &FontOptions,
        layout: &LayoutOptions,
    ) -> Section {
        let style = font_style_for_section(section.section_type, section.level, fonts).clone();
        let line_height = layout.line_height * style.font_size;
        let mut measured = section.clone();

        match section.section_type {
            SectionType::Image => return self.measure_image(section, content_width, layout),
            SectionType::Hr => {
                measured.measured_height = Some(24.0);
                measured.lines = Some(vec!["\u{2014}\u{2014}\u{2014}\u{2014}\u{2014}".into()]);
                measured.line_heights = Some(vec![24.0]);
                return measured;
            }
            _ => {}
        }

        let spacing_before = if section.section_type == SectionType::Heading {
            match section.level.unwrap_or(1) {
                1..=3 => layout.spacing_above_heading(section.level),
                _ => 0.0,
            }
        } else {
            0.0
        };

        let rich_source = match section.section_type {
            SectionType::Paragraph | SectionType::Endnote => Some(section.raw_markdown.clone()),
            SectionType::Blockquote => Some(strip_blockquote_markers(&section.raw_markdown)),
            _ => None,
        };
        let rich = rich_source
            .map(|src| self.wrap_rich_text(&src, content_width, &style, fonts))
            .filter(|r| !r.is_empty());

        let lines: Vec<String> = match &rich {
            Some(r) => r.iter().map(|l| l.plain_text()).collect(),
            None => {
                let transformed = apply_text_transform(&section.content, style.text_transform.as_deref());
                let mut out = Vec::new();
                for line in transformed.split('\n') {
                    out.extend(self.wrap_text(line, content_width, &style));
                }
                out
            }
        };

        measured.measured_height = Some(spacing_before + lines.len() as f64 * line_height + layout.paragraph_spacing);
        measured.line_heights = Some(vec![line_height; lines.len()]);
        measured.lines = Some(lines);
        measured.rich_lines = rich;
        measured
    }
}

/// JS `markdown.split(/\n\n+/)`.
fn split_paragraphs(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' && bytes.get(i + 1) == Some(&b'\n') {
            out.push(&text[start..i]);
            while i < bytes.len() && bytes[i] == b'\n' {
                i += 1;
            }
            start = i;
            continue;
        }
        i += 1;
    }
    out.push(&text[start..]);
    out
}

/// Split spans into word units. A unit is a run of non-space text that may
/// cross style boundaries (`**un**done` is one unit of two pieces), so no
/// space is inserted where the source had none and lines never break
/// inside a word.
fn spans_to_units(spans: &[TextSpan]) -> Vec<Vec<TextSpan>> {
    let mut units: Vec<Vec<TextSpan>> = Vec::new();
    let mut glue = false;
    for span in spans {
        let mut buf = String::new();
        for c in span.text.chars() {
            if is_js_space(c) {
                if !buf.is_empty() {
                    push_piece(&mut units, span.with_text(std::mem::take(&mut buf)), glue);
                }
                glue = false;
            } else {
                buf.push(c);
            }
        }
        if !buf.is_empty() {
            push_piece(&mut units, span.with_text(buf), glue);
            glue = true;
        }
    }
    units
}

fn push_piece(units: &mut Vec<Vec<TextSpan>>, piece: TextSpan, glue: bool) {
    match units.last_mut() {
        Some(unit) if glue => unit.push(piece),
        _ => units.push(vec![piece]),
    }
}

/// Font style for a section type.
pub fn font_style_for_section(t: SectionType, level: Option<u32>, fonts: &FontOptions) -> &FontStyle {
    match t {
        SectionType::Heading => fonts.heading(level.unwrap_or(1)),
        SectionType::Code => &fonts.code,
        SectionType::Blockquote => &fonts.blockquote,
        SectionType::EndnoteHeader => fonts.heading(level.unwrap_or(2)),
        SectionType::Endnote => &fonts.footnote,
        _ => &fonts.body,
    }
}

/// Style for a span: code font for code, superscript size for footnote
/// markers, bold/italic overrides.
pub fn span_font_style(base: &FontStyle, span: &TextSpan, fonts: &FontOptions) -> FontStyle {
    let mut style = base.clone();
    if span.code {
        style.font_family = fonts.code.font_family.clone();
        style.font_size = base.font_size * 0.9;
    }
    if span.footnote_number.is_some() {
        style.font_size = footnote_marker_font_size(base.font_size);
        style.font_weight = "normal".into();
        style.font_style = "normal".into();
    }
    if span.bold {
        style.font_weight = "bold".into();
    }
    if span.italic {
        style.font_style = "italic".into();
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraph_split() {
        assert_eq!(split_paragraphs("a\n\n\nb\nc"), vec!["a", "b\nc"]);
        assert_eq!(split_paragraphs("a"), vec!["a"]);
    }

    #[test]
    fn units_keep_style_and_glue() {
        let spans = vec![
            TextSpan { text: "a b".into(), bold: true, ..Default::default() },
            TextSpan::plain(". c"),
        ];
        let u = spans_to_units(&spans);
        assert_eq!(u.len(), 3); // [a] [b .] [c]
        assert_eq!(u[1].len(), 2); // "b" (bold) glued to "." (plain)
        assert!(u[1][0].bold && !u[1][1].bold);
        assert_eq!(spans_to_units(&[TextSpan::plain("x")])[0][0].text, "x");
    }
}
