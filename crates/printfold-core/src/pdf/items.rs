//! Vector drawing of page items. The app normally pre-renders pages that
//! carry items (Konva, 300 DPI) and the PDF just places that image; this
//! module is the fallback used when no pre-render is available, and also
//! draws legacy spread-spanning items. Unlike the original fallback it keeps
//! gradients, rotation and text-flow region fills/strokes.

use krilla::geom::{PathBuilder, Transform};
use krilla::num::NormalizedF32;
use krilla::paint::{Fill, FillRule, LinearGradient, Paint, RadialGradient, SpreadMethod, Stop};
use krilla::surface::Surface;

use super::canvas::{color_or_black, fill_of, parse_color, stroke_of, Painter, Rgba, TextStyle};
use super::page::{draw_rich_line, PageBox};
use super::PdfContext;
use crate::flow::font_style_for_section;
use crate::flow::polygon::{flatten_polygon, offset_flat_polygon, Pt};
use crate::model::{FillConfig, GradientStop, PageItem, PageItemType, SpanningItem};
use crate::text::apply_text_transform;

/// Single colour standing in for a fill (pattern → light gray, gradients →
/// first stop), used for backgrounds on the fallback path.
pub fn fill_fallback_color(fill: &FillConfig) -> Option<Rgba> {
    match fill.fill_type.as_str() {
        "color" => fill.color.as_deref().and_then(parse_color),
        "linearGradient" => fill.linear_gradient.as_ref().and_then(|g| g.stops.first()).and_then(|s| parse_color(&s.color)),
        "radialGradient" => fill.radial_gradient.as_ref().and_then(|g| g.stops.first()).and_then(|s| parse_color(&s.color)),
        "pattern" => Some(Rgba::gray(0.9)),
        _ => None,
    }
}

fn stops(stops: &[GradientStop]) -> Vec<Stop> {
    stops
        .iter()
        .map(|s| {
            let c = color_or_black(&s.color);
            Stop {
                offset: NormalizedF32::new(s.offset.clamp(0.0, 1.0) as f32).unwrap_or(NormalizedF32::ZERO),
                color: krilla::color::rgb::Color::new(c.r, c.g, c.b).into(),
                opacity: NormalizedF32::new(c.a).unwrap_or(NormalizedF32::ONE),
            }
        })
        .collect()
}

/// Paint for a fill in shape-local coordinates (origin at the shape's
/// top-left, size w×h), mirroring the editor's Konva gradient setup.
fn paint_for(fill: Option<&FillConfig>, fallback: Option<&str>, w: f64, h: f64) -> Option<(Paint, f32)> {
    let Some(fill) = fill else {
        let c = parse_color(fallback?)?;
        return Some((c.paint(), c.a));
    };
    match fill.fill_type.as_str() {
        "linearGradient" => {
            let g = fill.linear_gradient.as_ref()?;
            let a = g.angle.to_radians();
            let (cx, cy) = (w / 2.0, h / 2.0);
            let len = (w * w + h * h).sqrt() / 2.0;
            let lg = LinearGradient {
                x1: (cx - a.cos() * len) as f32,
                y1: (cy - a.sin() * len) as f32,
                x2: (cx + a.cos() * len) as f32,
                y2: (cy + a.sin() * len) as f32,
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: stops(&g.stops),
                anti_alias: true,
            };
            Some((lg.into(), 1.0))
        }
        "radialGradient" => {
            let g = fill.radial_gradient.as_ref()?;
            let (cx, cy) = ((g.center_x * w) as f32, (g.center_y * h) as f32);
            let r = (g.radius * w.max(h)) as f32;
            let rg = RadialGradient {
                fx: cx,
                fy: cy,
                fr: 0.0,
                cx,
                cy,
                cr: r,
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: stops(&g.stops),
                anti_alias: true,
            };
            Some((rg.into(), 1.0))
        }
        _ => {
            let c = fill_fallback_color(fill)?;
            Some((c.paint(), c.a))
        }
    }
}

/// Legacy spanning item → page item (fills in the TS defaults).
pub fn spanning_to_page_item(item: &SpanningItem) -> Option<PageItem> {
    let mut it = item.clone();
    match it.item_type {
        PageItemType::Text => {
            it.content.get_or_insert_with(String::new);
            it.font_family.get_or_insert_with(|| "Arial".into());
            it.font_size.get_or_insert(16.0);
            it.color.get_or_insert_with(|| "#000000".into());
        }
        PageItemType::Shape => {
            it.shape_type.get_or_insert_with(|| "rectangle".into());
        }
        PageItemType::Image => {
            it.image_file_id.get_or_insert_with(String::new);
        }
        PageItemType::TextFlow => return None,
    }
    Some(it)
}

/// Draw items with their x shifted by `offset_x` (crossing/spanning items).
pub fn draw_items_clipped(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, items: &[PageItem], bx: PageBox, offset_x: f64) {
    let mut sorted: Vec<&PageItem> = items.iter().collect();
    sorted.sort_by(|a, b| a.z_index.unwrap_or(0.0).partial_cmp(&b.z_index.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
    for item in sorted {
        let x = item.x + offset_x;
        if x + item.width <= 0.0 || x >= bx.width {
            continue;
        }
        let ox = bx.x + x;
        let oy = bx.top + item.y;
        let opacity = item.opacity_or_default();
        let rotation = item.rotation.unwrap_or(0.0);
        // Konva rotates nodes about their position (top-left; centre for
        // ellipses/circles, handled in draw_shape).
        let rotate_about_origin = rotation != 0.0 && !matches!(item.shape_type.as_deref(), Some("ellipse") | Some("circle"));
        s.push_transform(&Transform::from_translate(ox as f32, oy as f32));
        if rotate_about_origin {
            s.push_transform(&Transform::from_rotate(rotation as f32));
        }
        if opacity < 1.0 {
            s.push_opacity(super::canvas::norm(opacity));
        }
        match item.item_type {
            PageItemType::Shape => draw_shape(p, s, item),
            PageItemType::Text => draw_text_item(p, s, item),
            PageItemType::Image => {
                if let Some(img) = item.image_file_id.as_deref().and_then(|id| ctx.image_by_id(p, id)) {
                    p.draw_image(s, img, 0.0, 0.0, item.width, item.height, 1.0);
                }
            }
            PageItemType::TextFlow => draw_text_flow(p, s, ctx, item),
        }
        if opacity < 1.0 {
            s.pop();
        }
        if rotate_about_origin {
            s.pop();
        }
        s.pop();
    }
}

fn fill_and_stroke(p: &mut Painter, s: &mut Surface, path: &krilla::geom::Path, item: &PageItem, default_fill: bool, default_stroke: bool) {
    let _ = p;
    if item.has_fill.unwrap_or(default_fill) {
        if let Some((paint, alpha)) = paint_for(item.fill.as_ref(), item.fill_color.as_deref(), item.width, item.height) {
            s.set_stroke(None);
            s.set_fill(Some(Fill { paint, opacity: super::canvas::norm(alpha as f64), rule: FillRule::NonZero }));
            s.draw_path(path);
        }
    }
    if item.has_stroke.unwrap_or(default_stroke) {
        let color = color_or_black(item.stroke_color.as_deref().unwrap_or("#000000"));
        s.set_fill(None);
        s.set_stroke(Some(stroke_of(color, item.stroke_width.unwrap_or(1.0), 1.0)));
        s.draw_path(path);
        s.set_stroke(None);
    }
}

fn ellipse_path(cx: f64, cy: f64, rx: f64, ry: f64) -> Option<krilla::geom::Path> {
    const K: f64 = 0.552_284_749_8;
    let mut pb = PathBuilder::new();
    let (cx, cy, rx, ry) = (cx as f32, cy as f32, rx as f32, ry as f32);
    let (kx, ky) = (rx * K as f32, ry * K as f32);
    pb.move_to(cx + rx, cy);
    pb.cubic_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
    pb.cubic_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
    pb.cubic_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
    pb.cubic_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
    pb.close();
    pb.finish()
}

fn draw_shape(p: &mut Painter, s: &mut Surface, item: &PageItem) {
    let (w, h) = (item.width, item.height);
    let rotation = item.rotation.unwrap_or(0.0) as f32;
    match item.shape_type.as_deref().unwrap_or("rectangle") {
        "ellipse" | "circle" => {
            let (rx, ry) = if item.shape_type.as_deref() == Some("circle") {
                let r = w.min(h) / 2.0;
                (r, r)
            } else {
                (w / 2.0, h / 2.0)
            };
            let (cx, cy) = (rx, ry);
            if rotation != 0.0 {
                s.push_transform(&Transform::from_rotate_at(rotation, cx as f32, cy as f32));
            }
            if let Some(path) = ellipse_path(cx, cy, rx, ry) {
                fill_and_stroke(p, s, &path, item, true, true);
            }
            if rotation != 0.0 {
                s.pop();
            }
        }
        kind @ ("line" | "arrow") => {
            let color = color_or_black(item.stroke_color.as_deref().unwrap_or("#000000"));
            let sw = item.stroke_width.unwrap_or(1.0);
            let mid = h / 2.0;
            p.line(s, 0.0, mid, w, mid, color, sw);
            if kind == "arrow" {
                let a = 8.0_f64.max(sw * 3.0);
                p.line(s, w, mid, w - a, mid - a / 2.0, color, sw);
                p.line(s, w, mid, w - a, mid + a / 2.0, color, sw);
            }
        }
        _ => {
            if let Some(path) = super::canvas::rect_path(0.0, 0.0, w, h) {
                fill_and_stroke(p, s, &path, item, true, true);
            }
        }
    }
}

fn draw_text_item(p: &mut Painter, s: &mut Surface, item: &PageItem) {
    let content = apply_text_transform(item.content.as_deref().unwrap_or(""), item.text_transform.as_deref());
    let size = item.font_size.unwrap_or(16.0);
    let color = match (&item.fill, item.has_fill) {
        (_, Some(false)) => None,
        (Some(f), _) => fill_fallback_color(f),
        (None, _) => Some(color_or_black(item.color.as_deref().unwrap_or("#000000"))),
    };
    let Some(color) = color else { return };
    let family = item.font_family.clone().unwrap_or_else(|| "Arial".into());
    let style = TextStyle {
        family: &family,
        size,
        bold: item.font_weight.as_deref() == Some("bold"),
        italic: item.font_style.as_deref() == Some("italic"),
        color,
        opacity: 1.0,
    };
    let baseline = p.baseline_offset(&style);
    for (i, line) in content.split('\n').enumerate() {
        let w = p.text_width(line, &style);
        let x = match item.text_align.as_deref() {
            Some("center") => (item.width - w) / 2.0,
            Some("right") => item.width - w,
            _ => 0.0,
        };
        p.draw_text(s, line, x, i as f64 * size + baseline, &style);
    }
}

fn polygon_path(points: &[Pt]) -> Option<krilla::geom::Path> {
    let mut pb = PathBuilder::new();
    let first = points.first()?;
    pb.move_to(first.x as f32, first.y as f32);
    for pt in &points[1..] {
        pb.line_to(pt.x as f32, pt.y as f32);
    }
    pb.close();
    pb.finish()
}

fn draw_text_flow(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, item: &PageItem) {
    let fonts = &ctx.project.font_options;
    let layout = &ctx.project.layout_options;

    // Region fill and stroke (with perpendicular offsets).
    let outline: Vec<Pt> = if item.is_polygon_flow() {
        flatten_polygon(item.polygon_points.as_deref().unwrap_or(&[]), item.width, item.height)
    } else {
        vec![Pt { x: 0.0, y: 0.0 }, Pt { x: item.width, y: 0.0 }, Pt { x: item.width, y: item.height }, Pt { x: 0.0, y: item.height }]
    };
    if item.has_fill == Some(true) {
        if let Some(c) = item.fill.as_ref().and_then(fill_fallback_color) {
            if let Some(path) = polygon_path(&offset_flat_polygon(&outline, item.fill_offset.unwrap_or(0.0), 8.0)) {
                s.set_stroke(None);
                s.set_fill(Some(fill_of(c, 1.0)));
                s.draw_path(&path);
            }
        }
    }
    if item.has_stroke == Some(true) {
        let c = color_or_black(item.stroke_color.as_deref().unwrap_or("#000000"));
        if let Some(path) = polygon_path(&offset_flat_polygon(&outline, item.stroke_offset.unwrap_or(0.0), 8.0)) {
            s.set_fill(None);
            s.set_stroke(Some(stroke_of(c, item.stroke_width.unwrap_or(1.0), 1.0)));
            s.draw_path(&path);
            s.set_stroke(None);
        }
    }

    if item.is_polygon_flow() {
        for line in item.flowed_polygon_lines.as_deref().unwrap_or(&[]) {
            let st = font_style_for_section(line.section_type, line.section_level, fonts);
            let color = color_or_black(item.text_color.as_deref().unwrap_or(&st.color));
            let ts = TextStyle { family: &st.font_family, size: st.font_size, bold: st.is_bold(), italic: st.is_italic(), color, opacity: 1.0 };
            let baseline = line.y + p.baseline_offset(&ts);
            p.draw_text(s, &line.text, line.x, baseline, &ts);
        }
        return;
    }

    let pad = item.padding.unwrap_or(0.0);
    let content_w = (item.width - pad * 2.0).max(0.0);
    let bottom = item.height - pad;
    let mut y = pad;
    for section in item.flowed_sections.as_deref().unwrap_or(&[]) {
        let base = font_style_for_section(section.section_type, section.level, fonts);
        let mut st = base.clone();
        if let Some(c) = &item.text_color {
            st.color = c.clone();
        }
        let line_height = layout.line_height * st.font_size;
        if section.section_type == crate::model::SectionType::Heading && section.level.unwrap_or(1) <= 3 {
            y += layout.spacing_above_heading(section.level);
        }
        let align = st.text_align.clone().unwrap_or_else(|| layout.text_align.clone());
        let rich = section.rich_lines();
        if !rich.is_empty() {
            for line in rich {
                if y > bottom {
                    break;
                }
                draw_rich_line(p, s, line, pad, y, &st, fonts, content_w, line_height, &align);
                y += line_height;
            }
        } else {
            let color = color_or_black(&st.color);
            let ts = TextStyle { family: &st.font_family, size: st.font_size, bold: st.is_bold(), italic: st.is_italic(), color, opacity: 1.0 };
            for line in section.lines() {
                if y > bottom {
                    break;
                }
                let w = p.text_width(line, &ts);
                let x = match align.as_str() {
                    "center" => pad + (content_w - w) / 2.0,
                    "right" => pad + content_w - w,
                    _ => pad,
                };
                let baseline = y + p.baseline_offset(&ts);
                p.draw_text(s, line, x, baseline, &ts);
                y += line_height;
            }
        }
        y += layout.paragraph_spacing;
    }
}
