//! Drawing one booklet page onto a sheet (port of `PDFGenerator.drawPage`).
//!
//! Positions follow the editor (Konva) rather than the original pdf-lib
//! code, so exported pages match what the user sees: text boxes are placed
//! by their top edge with WebKit's "middle" baseline, headers/footers use
//! their configured colour, and markdown
//! images are drawn at their laid-out size.

use krilla::surface::Surface;

use super::canvas::{color_or_black, parse_color, Painter, Rgba, TextStyle};
use super::items::{draw_items_clipped, spanning_to_page_item};
use super::PdfContext;
use crate::flow::measure::{caption_style, find_image_key, CAPTION_FONT_SIZE, CAPTION_GAP, MISSING_IMAGE_HEIGHT, MISSING_IMAGE_MAX_WIDTH};
use crate::flow::{font_style_for_section, span_font_style};
use crate::markdown::footnotes::{FOOTNOTE_RULE_GAP, FOOTNOTE_RULE_THICKNESS, FOOTNOTE_RULE_WIDTH_RATIO};
use crate::model::{FontOptions, FontStyle, HeaderFooterSection, PageContent, PageState, RichTextLine, SectionType, SpanningItem};
use crate::text::{apply_text_transform, words};

/// Page box on the sheet (top-left origin).
#[derive(Debug, Clone, Copy)]
pub struct PageBox {
    pub x: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

fn text_style<'a>(style: &'a FontStyle, color: Rgba) -> TextStyle<'a> {
    TextStyle {
        family: &style.font_family,
        size: style.font_size,
        bold: style.is_bold(),
        italic: style.is_italic(),
        color,
        opacity: 1.0,
    }
}

/// Draw `text` as a Konva text box with its top at `top`.
fn draw_box_text(p: &mut Painter, s: &mut Surface, text: &str, x: f64, top: f64, style: &TextStyle) -> f64 {
    let baseline = top + p.baseline_offset(style);
    p.draw_text(s, text, x, baseline, style)
}

pub fn draw_page(
    p: &mut Painter,
    s: &mut Surface,
    ctx: &PdfContext,
    page: &PageContent,
    bx: PageBox,
    adjacent: Option<&PageContent>,
    spanning: Option<&[SpanningItem]>,
) {
    let output = &ctx.project.output_options;
    let render_all_as_images = output.render_text_as_images == Some(true);
    let pre_rendered = ctx.pre_rendered_image(p, page.page_number);

    if render_all_as_images {
        if let Some(img) = pre_rendered.clone() {
            p.draw_image(s, img, bx.x, bx.top, bx.width, bx.height, 1.0);
            draw_spanning(p, s, ctx, spanning, bx, page.is_recto);
            return;
        }
    }

    let is_text_page = page.page_state == PageState::Text;
    let has_background = page.background_fill.is_some() || page.custom_background_image_id.is_some();
    if !is_text_page && (page.has_items() || has_background || page.is_static_or_available()) {
        if let Some(img) = pre_rendered {
            p.draw_image(s, img, bx.x, bx.top, bx.width, bx.height, 1.0);
        } else {
            draw_background(p, s, ctx, page, bx);
            draw_items_with_crossing(p, s, ctx, page, bx, adjacent);
        }
        draw_spanning(p, s, ctx, spanning, bx, page.is_recto);
        return;
    }

    draw_background(p, s, ctx, page, bx);
    draw_text_content(p, s, ctx, page, bx);
    draw_header_footer(p, s, ctx, page, bx);

    if let Some(img) = pre_rendered {
        p.draw_image(s, img, bx.x, bx.top, bx.width, bx.height, 1.0);
    } else {
        draw_items_with_crossing(p, s, ctx, page, bx, adjacent);
    }
    draw_spanning(p, s, ctx, spanning, bx, page.is_recto);
}

/// Solid background fill and custom background image (fallback path; the
/// pre-rendered page image normally carries these).
fn draw_background(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, page: &PageContent, bx: PageBox) {
    if let Some(fill) = &page.background_fill {
        if let Some(color) = super::items::fill_fallback_color(fill) {
            p.fill_rect(s, bx.x, bx.top, bx.width, bx.height, color, 1.0);
        }
    }
    if let Some(id) = &page.custom_background_image_id {
        if let Some(img) = ctx.image_by_id(p, id) {
            p.draw_image(s, img, bx.x, bx.top, bx.width, bx.height, 1.0);
        }
    }
}

fn draw_items_with_crossing(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, page: &PageContent, bx: PageBox, adjacent: Option<&PageContent>) {
    let adjacent_items = adjacent.map(|a| a.items()).unwrap_or(&[]);
    if !page.has_items() && adjacent_items.is_empty() {
        return;
    }
    let clipped = p.push_clip_rect(s, bx.x, bx.top, bx.width, bx.height);
    draw_items_clipped(p, s, ctx, page.items(), bx, 0.0);
    // Items from the reading-order neighbour that extend onto this page.
    let crossing: Vec<_> = adjacent_items
        .iter()
        .filter(|i| if page.is_recto { i.x + i.width > bx.width } else { i.x < 0.0 })
        .cloned()
        .collect();
    if !crossing.is_empty() {
        let offset = if page.is_recto { -bx.width } else { bx.width };
        draw_items_clipped(p, s, ctx, &crossing, bx, offset);
    }
    if clipped {
        s.pop();
    }
}

fn draw_spanning(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, spanning: Option<&[SpanningItem]>, bx: PageBox, is_recto: bool) {
    let Some(items) = spanning else { return };
    let offset = if is_recto { -bx.width } else { 0.0 };
    let visible: Vec<_> = items
        .iter()
        .filter_map(spanning_to_page_item)
        .filter(|i| i.x + offset + i.width > 0.0 && i.x + offset < bx.width)
        .collect();
    if visible.is_empty() {
        return;
    }
    let clipped = p.push_clip_rect(s, bx.x, bx.top, bx.width, bx.height);
    draw_items_clipped(p, s, ctx, &visible, bx, offset);
    if clipped {
        s.pop();
    }
}

/// Inner/outer margins on the page's left/right sides.
fn side_margins(ctx: &PdfContext, page: &PageContent) -> (f64, f64) {
    let m = ctx.project.layout_options.margins;
    if page.is_recto {
        (m.inner, m.outer)
    } else {
        (m.outer, m.inner)
    }
}

fn draw_text_content(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, page: &PageContent, bx: PageBox) {
    let layout = &ctx.project.layout_options;
    let fonts = &ctx.project.font_options;
    let m = layout.margins;
    let (left, right) = side_margins(ctx, page);
    let content_x = bx.x + left;
    let content_w = bx.width - left - right;
    let content_top = bx.top + m.top;
    let content_bottom = bx.top + bx.height - m.bottom;
    let mut y = content_top;

    for section in &page.sections {
        let style = font_style_for_section(section.section_type, section.level, fonts);
        let line_height = layout.line_height * style.font_size;
        if section.section_type == SectionType::Heading && section.level.unwrap_or(1) <= 3 {
            y += layout.spacing_above_heading(section.level);
        }

        if section.section_type == SectionType::Image {
            draw_image_section(p, s, ctx, section, content_x, y, content_w);
            y += section.measured_height.unwrap_or(MISSING_IMAGE_HEIGHT + 10.0 + layout.paragraph_spacing);
            continue;
        }

        let align = style.text_align.clone().unwrap_or_else(|| layout.text_align.clone());
        let rich = section.rich_lines();
        if !rich.is_empty() {
            for line in rich {
                if y > content_bottom {
                    break;
                }
                draw_rich_line(p, s, line, content_x, y, style, fonts, content_w, line_height, &align);
                y += line_height;
            }
        } else {
            let color = color_or_black(&style.color);
            for line in section.lines() {
                if y > content_bottom {
                    break;
                }
                draw_plain_line(p, s, line, content_x, y, style, color, content_w, line_height, &align);
                y += line_height;
            }
        }
        y += layout.paragraph_spacing;
    }

    if let Some(notes) = page.footnotes.as_ref().filter(|n| !n.is_empty()) {
        draw_footnotes(p, s, ctx, notes, content_x, content_bottom, content_w);
    }
}

fn draw_image_section(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, section: &crate::model::Section, x: f64, y: f64, width: f64) {
    let reference = section.image_ref.clone().unwrap_or_default();
    let found = find_image_key(&reference, ctx.files.iter().filter(|f| f.file_type == "image").map(|f| f.name.to_lowercase()))
        .and_then(|key| ctx.files.iter().find(|f| f.name.to_lowercase() == key))
        .map(|f| f.id.clone());
    match (found, section.image_width, section.image_height) {
        (Some(id), Some(w), Some(h)) => {
            let ix = x + (width - w).max(0.0) / 2.0;
            if let Some(img) = ctx.image_by_id(p, &id) {
                p.draw_image(s, img, ix, y, w, h, 1.0);
            }
            let caption = caption_style();
            let style = TextStyle { family: &caption.font_family, size: CAPTION_FONT_SIZE, bold: false, italic: true, color: color_or_black("#666666"), opacity: 1.0 };
            let mut ty = y + h + CAPTION_GAP;
            for line in section.lines() {
                let lw = p.text_width(line, &style);
                draw_box_text(p, s, line, ix + (w - lw) / 2.0, ty, &style);
                ty += CAPTION_FONT_SIZE;
            }
        }
        _ => {
            let w = width.min(MISSING_IMAGE_MAX_WIDTH);
            p.fill_rect(s, x, y, w, MISSING_IMAGE_HEIGHT, color_or_black("#f0f0f0"), 1.0);
            p.stroke_rect(s, x, y, w, MISSING_IMAGE_HEIGHT, color_or_black("#cccccc"), 1.0);
            let style = TextStyle { family: "Arial", size: 10.0, bold: false, italic: false, color: color_or_black("#999999"), opacity: 1.0 };
            let label = format!("Image not uploaded: {}", if reference.is_empty() { "unknown" } else { &reference });
            let lw = p.text_width(&label, &style).min(w - 20.0);
            draw_box_text(p, s, &label, x + 10.0 + ((w - 20.0) - lw).max(0.0) / 2.0, y + 40.0, &style);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_plain_line(p: &mut Painter, s: &mut Surface, line: &str, x: f64, top: f64, style: &FontStyle, color: Rgba, width: f64, line_height: f64, align: &str) {
    let ts = text_style(style, color);
    let w = p.text_width(line, &ts);
    let lx = match align {
        "center" => x + (width - w) / 2.0,
        "right" => x + width - w,
        _ => x,
    };
    if let Some(bg) = style.background_color.as_deref().filter(|c| !c.eq_ignore_ascii_case("#ffffff")) {
        if let Some(bg) = parse_color(bg) {
            p.fill_rect(s, lx, top, w, line_height, bg, 1.0);
        }
    }
    draw_box_text(p, s, line, lx, top, &ts);
    let deco = style.text_decoration.as_deref().unwrap_or("none");
    let thickness = style.font_size / 15.0;
    if deco.contains("underline") {
        let uy = top + style.font_size / 2.0 + (style.font_size / 2.0).round();
        p.line(s, lx, uy, lx + w, uy, color, thickness);
    }
    if deco.contains("line-through") {
        let sy = top + style.font_size / 2.0;
        p.line(s, lx, sy, lx + w, sy, color, thickness);
    }
}

/// Draw a styled line, mirroring the editor's `drawRichLineKonva`.
#[allow(clippy::too_many_arguments)]
pub fn draw_rich_line(
    p: &mut Painter,
    s: &mut Surface,
    line: &RichTextLine,
    x: f64,
    top: f64,
    base: &FontStyle,
    fonts: &FontOptions,
    width: f64,
    line_height: f64,
    align: &str,
) {
    let styles: Vec<FontStyle> = line.spans.iter().map(|sp| span_render_style(base, sp, fonts)).collect();
    let widths: Vec<f64> = line
        .spans
        .iter()
        .zip(&styles)
        .map(|(sp, st)| p.text_width(&sp.text, &text_style(st, Rgba::BLACK)))
        .collect();
    let total: f64 = widths.iter().sum();
    let mut cx = match align {
        "center" => x + (width - total) / 2.0,
        "right" => x + width - total,
        _ => x,
    };
    for ((span, st), w) in line.spans.iter().zip(&styles).zip(&widths) {
        let mut color = color_or_black(&base.color);
        if span.highlight {
            if let Some(h) = &fonts.highlight {
                if let Some(bg) = parse_color(&h.background_color) {
                    p.fill_rect(s, cx, top, *w, line_height, bg, 1.0);
                }
                color = color_or_black(&h.text_color);
            }
        } else if span.footnote_number.is_some() {
            if let Some(c) = &fonts.footnote_number_color {
                color = color_or_black(c);
            }
        } else if span.strikethrough {
            if let Some(st) = &fonts.strikethrough {
                color = color_or_black(&st.text_color);
            }
        }
        let y_offset = if span.footnote_number.is_some() { -base.font_size * 0.35 } else { 0.0 };
        let ts = text_style(st, color);
        draw_box_text(p, s, &span.text, cx, top + y_offset, &ts);
        if span.strikethrough {
            let line_color = fonts.strikethrough.as_ref().map(|st| color_or_black(&st.line_color)).unwrap_or(color);
            let sy = top + st.font_size / 2.0;
            p.line(s, cx, sy, cx + w, sy, line_color, st.font_size / 15.0);
        }
        cx += w;
    }
}

/// Span style as the editor renders it (code font, superscript size,
/// bold/italic inherited from the base style or the span).
fn span_render_style(base: &FontStyle, span: &crate::model::TextSpan, fonts: &FontOptions) -> FontStyle {
    let mut st = span_font_style(base, span, fonts);
    if span.footnote_number.is_some() {
        st.font_weight = if base.is_bold() || span.bold { "bold".into() } else { "normal".into() };
        st.font_style = if base.is_italic() || span.italic { "italic".into() } else { "normal".into() };
    }
    st
}

/// Footnote block anchored to the bottom of the content area.
fn draw_footnotes(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, notes: &[crate::model::FootnoteDefinition], x: f64, content_bottom: f64, width: f64) {
    let fonts = &ctx.project.font_options;
    let layout = &ctx.project.layout_options;
    let style = &fonts.footnote;
    let line_height = style.line_height.unwrap_or(layout.line_height) * style.font_size;
    let gap = fonts.footnote_gap.unwrap_or(0.0);
    let text_color = color_or_black(&style.color);
    let number_hex = fonts.footnote_number_color.clone().unwrap_or_else(|| style.color.clone());
    let distinct_number = number_hex != style.color;
    let number_color = color_or_black(&number_hex);
    let ts = text_style(style, text_color);

    let wrapped: Vec<Vec<String>> = notes.iter().map(|f| wrap_plain(p, &format!("{}. {}", f.number, f.content), width, &ts)).collect();
    let total_lines: usize = wrapped.iter().map(|w| w.len()).sum();
    let rule_space = FOOTNOTE_RULE_GAP + FOOTNOTE_RULE_THICKNESS + FOOTNOTE_RULE_GAP;
    let block = rule_space + total_lines as f64 * line_height + notes.len().saturating_sub(1) as f64 * gap;

    let mut y = content_bottom - block;
    p.line(s, x, y + FOOTNOTE_RULE_GAP, x + width * FOOTNOTE_RULE_WIDTH_RATIO, y + FOOTNOTE_RULE_GAP, text_color, FOOTNOTE_RULE_THICKNESS);
    y += rule_space;
    for (fi, lines) in wrapped.iter().enumerate() {
        let prefix = format!("{}. ", notes[fi].number);
        for (li, line) in lines.iter().enumerate() {
            if li == 0 && distinct_number && line.starts_with(&prefix) {
                let numbered = TextStyle { color: number_color, ..ts.clone() };
                let pw = draw_box_text(p, s, &prefix, x, y, &numbered);
                draw_box_text(p, s, &line[prefix.len()..], x + pw, y, &ts);
            } else {
                draw_box_text(p, s, line, x, y, &ts);
            }
            y += line_height;
        }
        if fi + 1 < wrapped.len() {
            y += gap;
        }
    }
}

fn wrap_plain(p: &mut Painter, text: &str, width: f64, style: &TextStyle) -> Vec<String> {
    let safe = width * crate::flow::measure::SAFE_WIDTH_RATIO;
    let mut out = Vec::new();
    for hard in text.split('\n') {
        let ws = words(hard);
        if ws.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        for w in ws {
            let candidate = if line.is_empty() { w.to_string() } else { format!("{line} {w}") };
            if p.text_width(&candidate, style) <= safe {
                line = candidate;
            } else {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                line = w.to_string();
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn draw_header_footer(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, page: &PageContent, bx: PageBox) {
    let hf = &ctx.project.header_footer;
    let m = ctx.project.layout_options.margins;
    let (left, right) = side_margins(ctx, page);
    let content_x = bx.x + left;
    let content_w = bx.width - left - right;
    // `showOnFirstPage` exists in the data model but has never had a UI
    // control or an effect in either renderer; it is left unused here too.
    if hf.header.enabled {
        let line_y = bx.top + m.top - hf.header.height;
        let top = line_y - hf.header.font.font_size - 2.0;
        draw_hf_content(p, s, &hf.header, page, content_x, top, content_w);
    }
    if hf.footer.enabled {
        let line_y = bx.top + bx.height - m.bottom + hf.footer.height;
        draw_hf_content(p, s, &hf.footer, page, content_x, line_y + 2.0, content_w);
    }
}

fn draw_hf_content(p: &mut Painter, s: &mut Surface, cfg: &HeaderFooterSection, page: &PageContent, x: f64, top: f64, width: f64) {
    let positions = if page.is_recto { &cfg.recto } else { &cfg.verso };
    let font = &cfg.font;
    // The editor draws header/footer text in the font's colour without
    // weight/style; match it.
    let ts = TextStyle { family: &font.font_family, size: font.font_size, bold: false, italic: false, color: color_or_black(&font.color), opacity: 1.0 };
    let expand = |t: &str| apply_text_transform(&t.replace("{{pageNumber}}", &page.page_number.to_string()), font.text_transform.as_deref());
    if !positions.left.is_empty() {
        draw_box_text(p, s, &expand(&positions.left), x, top, &ts);
    }
    if !positions.center.is_empty() {
        let t = expand(&positions.center);
        let w = p.text_width(&t, &ts);
        draw_box_text(p, s, &t, x + width / 2.0 - w / 2.0, top, &ts);
    }
    if !positions.right.is_empty() {
        let t = expand(&positions.right);
        let w = p.text_width(&t, &ts);
        draw_box_text(p, s, &t, x + width - w, top, &ts);
    }
}
