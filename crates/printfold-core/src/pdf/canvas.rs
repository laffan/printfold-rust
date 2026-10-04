//! Drawing helpers over a krilla surface, in top-left page coordinates
//! (y grows downward, like the editor and like krilla itself).

use std::collections::HashMap;

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::paint::{Fill, FillRule, LineCap, Paint, Stroke};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::Data;

use crate::fonts::{FaceRef, FontRegistry, FontRequest, ShapedText};

/// Synthetic italic slant used by WebKit (14°).
const SYNTHETIC_OBLIQUE: f64 = 0.249;

/// An sRGB colour with alpha parsed from CSS-style strings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba { r: 0, g: 0, b: 0, a: 1.0 };

    pub fn gray(level: f64) -> Rgba {
        let v = (level.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgba { r: v, g: v, b: v, a: 1.0 }
    }

    pub fn paint(&self) -> Paint {
        rgb::Color::new(self.r, self.g, self.b).into()
    }
}

/// Parse `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()/rgba()` and a few names.
pub fn parse_color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let h = |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16).ok();
        return match hex.len() {
            3 => Some(Rgba { r: h(0, 1)? * 17, g: h(1, 1)? * 17, b: h(2, 1)? * 17, a: 1.0 }),
            6 => Some(Rgba { r: h(0, 2)?, g: h(2, 2)?, b: h(4, 2)?, a: 1.0 }),
            8 => Some(Rgba { r: h(0, 2)?, g: h(2, 2)?, b: h(4, 2)?, a: h(6, 2)? as f32 / 255.0 }),
            _ => None,
        };
    }
    let lower = s.to_lowercase();
    if let Some(inner) = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb(")) {
        let nums: Vec<f32> = inner.trim_end_matches(')').split(',').filter_map(|p| p.trim().parse().ok()).collect();
        if nums.len() >= 3 {
            return Some(Rgba {
                r: nums[0].clamp(0.0, 255.0) as u8,
                g: nums[1].clamp(0.0, 255.0) as u8,
                b: nums[2].clamp(0.0, 255.0) as u8,
                a: nums.get(3).copied().unwrap_or(1.0).clamp(0.0, 1.0),
            });
        }
    }
    match lower.as_str() {
        "black" => Some(Rgba::BLACK),
        "white" => Some(Rgba { r: 255, g: 255, b: 255, a: 1.0 }),
        "red" => Some(Rgba { r: 255, g: 0, b: 0, a: 1.0 }),
        "transparent" => Some(Rgba { r: 0, g: 0, b: 0, a: 0.0 }),
        _ => None,
    }
}

pub fn color_or_black(s: &str) -> Rgba {
    parse_color(s).unwrap_or(Rgba::BLACK)
}

pub fn norm(v: f64) -> NormalizedF32 {
    NormalizedF32::new(v.clamp(0.0, 1.0) as f32).unwrap_or(NormalizedF32::ONE)
}

pub fn fill_of(color: Rgba, opacity: f64) -> Fill {
    Fill { paint: color.paint(), opacity: norm(color.a as f64 * opacity), rule: FillRule::NonZero }
}

pub fn stroke_of(color: Rgba, width: f64, opacity: f64) -> Stroke {
    Stroke { paint: color.paint(), width: width as f32, opacity: norm(color.a as f64 * opacity), line_cap: LineCap::Butt, ..Default::default() }
}

pub fn rect_path(x: f64, y: f64, w: f64, h: f64) -> Option<krilla::geom::Path> {
    let mut pb = PathBuilder::new();
    pb.push_rect(Rect::from_xywh(x as f32, y as f32, w.max(0.0) as f32, h.max(0.0) as f32)?);
    pb.finish()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ImageKind {
    Png,
    Jpeg,
    Gif,
    Webp,
}

fn sniff(bytes: &[u8]) -> Option<ImageKind> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some(ImageKind::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some(ImageKind::Jpeg)
    } else if bytes.starts_with(b"GIF8") {
        Some(ImageKind::Gif)
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageKind::Webp)
    } else {
        None
    }
}

/// Fully decode once to make sure krilla's deferred decode can't fail at
/// `finish()` (which would abort the whole document).
fn validate(kind: ImageKind, bytes: &[u8]) -> bool {
    use std::io::Cursor;
    match kind {
        ImageKind::Png => {
            let mut decoder = png::Decoder::new(Cursor::new(bytes));
            decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
            let Ok(mut reader) = decoder.read_info() else { return false };
            let Some(size) = reader.output_buffer_size() else { return false };
            let mut buf = vec![0; size];
            reader.next_frame(&mut buf).is_ok()
        }
        ImageKind::Jpeg => zune_jpeg::JpegDecoder::new(Cursor::new(bytes)).decode().is_ok(),
        ImageKind::Gif => {
            let mut opts = gif::DecodeOptions::new();
            opts.set_color_output(gif::ColorOutput::RGBA);
            opts.read_info(bytes).ok().and_then(|mut d| d.read_next_frame().ok().flatten().map(|_| ())).is_some()
        }
        ImageKind::Webp => match image_webp::WebPDecoder::new(Cursor::new(bytes)) {
            Ok(mut d) => match d.output_buffer_size() {
                Some(size) => d.read_image(&mut vec![0; size]).is_ok(),
                None => false,
            },
            Err(_) => false,
        },
    }
}

/// Decode an image by sniffing its magic bytes; invalid images yield None.
pub fn decode_image(bytes: &[u8]) -> Option<Image> {
    let kind = sniff(bytes)?;
    if !validate(kind, bytes) {
        return None;
    }
    let data: Data = bytes.to_vec().into();
    match kind {
        ImageKind::Png => Image::from_png(data, true).ok(),
        ImageKind::Jpeg => Image::from_jpeg(data, true).ok(),
        ImageKind::Gif => Image::from_gif(data, true).ok(),
        ImageKind::Webp => Image::from_webp(data, true).ok(),
    }
}

/// Pixel size of an encoded image (used for markdown image layout).
pub fn image_pixel_size(bytes: &[u8]) -> Option<(u32, u32)> {
    decode_image(bytes).map(|i| i.size())
}

/// Text style for one draw call.
#[derive(Debug, Clone)]
pub struct TextStyle<'a> {
    pub family: &'a str,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub color: Rgba,
    pub opacity: f64,
}

/// Font and image caches shared by every page of one document.
pub struct Painter<'r> {
    pub fonts: &'r mut FontRegistry,
    krilla_fonts: HashMap<FaceRef, Option<Font>>,
    images: HashMap<String, Option<Image>>,
}

impl<'r> Painter<'r> {
    pub fn new(fonts: &'r mut FontRegistry) -> Self {
        Self { fonts, krilla_fonts: HashMap::new(), images: HashMap::new() }
    }

    fn krilla_font(&mut self, face: FaceRef) -> Option<Font> {
        if let Some(f) = self.krilla_fonts.get(&face) {
            return f.clone();
        }
        let font = self.fonts.load(face).and_then(|lf| {
            let data: Data = lf.bytes().clone().into();
            Font::new(data, lf.index)
        });
        self.krilla_fonts.insert(face, font.clone());
        font
    }

    /// Cached image decode keyed by an id (file id or page key).
    pub fn image(&mut self, key: &str, bytes: impl FnOnce() -> Option<Vec<u8>>) -> Option<Image> {
        if let Some(i) = self.images.get(key) {
            return i.clone();
        }
        let img = bytes().and_then(|b| decode_image(&b));
        self.images.insert(key.to_string(), img.clone());
        img
    }

    pub fn shape(&mut self, text: &str, style: &TextStyle) -> ShapedText {
        self.fonts.shape(text, FontRequest::new(style.family, style.bold, style.italic), style.size)
    }

    pub fn text_width(&mut self, text: &str, style: &TextStyle) -> f64 {
        self.shape(text, style).width
    }

    /// Baseline offset below a text box top, matching Konva's rendering.
    pub fn baseline_offset(&mut self, style: &TextStyle) -> f64 {
        let resolved = self.fonts.resolve(style.family, style.bold, style.italic);
        match resolved.and_then(|r| self.fonts.load(r.face)) {
            Some(face) => face.middle_baseline_offset(style.size),
            None => style.size * 0.8,
        }
    }

    /// Draw text with its baseline at `baseline`; returns the advance width.
    pub fn draw_text(&mut self, s: &mut Surface, text: &str, x: f64, baseline: f64, style: &TextStyle) -> f64 {
        if text.is_empty() || style.size <= 0.0 {
            return 0.0;
        }
        let shaped = self.shape(text, style);
        let mut pen = x;
        for run in &shaped.runs {
            let Some(font) = self.krilla_font(run.font.face) else {
                pen += run.width;
                continue;
            };
            let size = style.size;
            let glyphs: Vec<KrillaGlyph> = run
                .glyphs
                .iter()
                .map(|g| {
                    KrillaGlyph::new(
                        GlyphId::new(g.glyph_id as u32),
                        (g.x_advance / size) as f32,
                        (g.x_offset / size) as f32,
                        (g.y_offset / size) as f32,
                        0.0,
                        (g.cluster.0 - run.range.start)..(g.cluster.1 - run.range.start),
                        None,
                    )
                })
                .collect();
            let run_text = &text[run.range.clone()];
            let skew = run.font.synthetic_italic;
            if skew {
                let k = SYNTHETIC_OBLIQUE as f32;
                s.push_transform(&Transform::from_row(1.0, 0.0, -k, 1.0, k * baseline as f32, 0.0));
            }
            s.set_fill(Some(fill_of(style.color, style.opacity)));
            if run.font.synthetic_bold {
                s.set_stroke(Some(stroke_of(style.color, (size * 0.04).clamp(0.3, 1.0), style.opacity)));
            } else {
                s.set_stroke(None);
            }
            s.draw_glyphs(Point::from_xy(pen as f32, baseline as f32), &glyphs, font, run_text, size as f32, false);
            s.set_stroke(None);
            if skew {
                s.pop();
            }
            pen += run.width;
        }
        shaped.width
    }

    pub fn fill_rect(&mut self, s: &mut Surface, x: f64, y: f64, w: f64, h: f64, color: Rgba, opacity: f64) {
        if let Some(path) = rect_path(x, y, w, h) {
            s.set_stroke(None);
            s.set_fill(Some(fill_of(color, opacity)));
            s.draw_path(&path);
        }
    }

    pub fn stroke_rect(&mut self, s: &mut Surface, x: f64, y: f64, w: f64, h: f64, color: Rgba, width: f64) {
        if let Some(path) = rect_path(x, y, w, h) {
            s.set_fill(None);
            s.set_stroke(Some(stroke_of(color, width, 1.0)));
            s.draw_path(&path);
            s.set_stroke(None);
        }
    }

    pub fn line(&mut self, s: &mut Surface, x1: f64, y1: f64, x2: f64, y2: f64, color: Rgba, width: f64) {
        let mut pb = PathBuilder::new();
        pb.move_to(x1 as f32, y1 as f32);
        pb.line_to(x2 as f32, y2 as f32);
        if let Some(path) = pb.finish() {
            s.set_fill(None);
            s.set_stroke(Some(stroke_of(color, width, 1.0)));
            s.draw_path(&path);
            s.set_stroke(None);
        }
    }

    pub fn draw_image(&mut self, s: &mut Surface, image: Image, x: f64, y: f64, w: f64, h: f64, opacity: f64) {
        let Some(size) = Size::from_wh(w as f32, h as f32) else { return };
        let fade = opacity < 1.0;
        if fade {
            s.push_opacity(norm(opacity));
        }
        s.push_transform(&Transform::from_translate(x as f32, y as f32));
        s.draw_image(image, size);
        s.pop();
        if fade {
            s.pop();
        }
    }

    pub fn push_clip_rect(&mut self, s: &mut Surface, x: f64, y: f64, w: f64, h: f64) -> bool {
        match rect_path(x, y, w, h) {
            Some(path) => {
                s.push_clip_path(&path, &FillRule::NonZero);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(parse_color("#fff"), Some(Rgba { r: 255, g: 255, b: 255, a: 1.0 }));
        assert_eq!(parse_color("#102030"), Some(Rgba { r: 16, g: 32, b: 48, a: 1.0 }));
        assert_eq!(parse_color("rgba(1, 2, 3, 0.5)").unwrap().a, 0.5);
        assert!(parse_color("nonsense").is_none());
    }
}
