//! Text shaping and width measurement.
//!
//! Widths mirror what a WebKit canvas `measureText` reports for the same
//! font: HarfBuzz-compatible shaping (kerning + ligatures via rustybuzz),
//! per-character font fallback for glyphs the primary face lacks, and
//! WebKit's synthetic-bold advance bump (+1px per glyph) when bold is faked.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use super::registry::{FaceRef, FontRegistry, LoadedFace, ResolvedFont};

/// WebKit widens every glyph by one CSS pixel when it synthesises bold.
const SYNTHETIC_BOLD_OFFSET: f64 = 1.0;
/// Fallback scanning stops after this many non-preferred faces per char.
const MAX_FALLBACK_SCAN: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontRequest<'a> {
    pub family: &'a str,
    pub bold: bool,
    pub italic: bool,
}

impl<'a> FontRequest<'a> {
    pub fn new(family: &'a str, bold: bool, italic: bool) -> Self {
        Self { family, bold, italic }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    /// Byte range in the run's text covered by this glyph's cluster.
    pub cluster: (usize, usize),
    /// Advance in points at the requested size (synthetic bold included).
    pub x_advance: f64,
    pub x_offset: f64,
    pub y_offset: f64,
}

/// A run of text drawn with a single face.
#[derive(Clone)]
pub struct ShapedRun {
    pub font: ResolvedFont,
    pub face: Arc<LoadedFace>,
    /// Byte range of the run within the shaped string.
    pub range: Range<usize>,
    pub glyphs: Vec<ShapedGlyph>,
    pub width: f64,
}

#[derive(Clone, Default)]
pub struct ShapedText {
    pub runs: Vec<ShapedRun>,
    pub width: f64,
}

#[derive(Default)]
pub struct WidthCache {
    generation: u64,
    map: HashMap<(String, String, u64, bool, bool), f64>,
}

impl WidthCache {
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl FontRegistry {
    fn face_for_char(&mut self, primary: &Arc<LoadedFace>, c: char) -> Option<FaceRef> {
        if c.is_whitespace() || c.is_control() || primary.has_glyph(c) {
            return None;
        }
        if let Some(hit) = self.char_fallback.get(&c) {
            return *hit;
        }
        let mut found = None;
        for (scanned, candidate) in self.fallback_candidates().into_iter().enumerate() {
            if scanned > MAX_FALLBACK_SCAN {
                break;
            }
            if let Some(face) = self.load(candidate) {
                if face.has_glyph(c) {
                    found = Some(candidate);
                    break;
                }
            }
        }
        self.char_fallback.insert(c, found);
        found
    }

    /// Shape `text` with the requested font at `size` points.
    pub fn shape(&mut self, text: &str, req: FontRequest, size: f64) -> ShapedText {
        let Some(resolved) = self.resolve(req.family, req.bold, req.italic) else {
            return ShapedText::default();
        };
        let Some(primary) = self.load(resolved.face) else {
            return ShapedText::default();
        };

        // Segment into runs of consecutive characters sharing a face.
        let mut segments: Vec<(Range<usize>, Option<FaceRef>)> = Vec::new();
        for (i, c) in text.char_indices() {
            let face = self.face_for_char(&primary, c);
            let end = i + c.len_utf8();
            match segments.last_mut() {
                Some((range, f)) if *f == face => range.end = end,
                _ => segments.push((i..end, face)),
            }
        }

        let mut shaped = ShapedText::default();
        for (range, fallback) in segments {
            let (font, face) = match fallback.and_then(|f| self.load(f).map(|l| (f, l))) {
                Some((f, l)) => {
                    let synthetic_bold = req.bold && l.weight < 600;
                    let synthetic_italic = req.italic && !l.italic;
                    (ResolvedFont { face: f, synthetic_bold, synthetic_italic }, l)
                }
                None => (resolved, primary.clone()),
            };
            let run = shape_run(&text[range.clone()], range.start, font, face, size);
            shaped.width += run.width;
            shaped.runs.push(run);
        }
        shaped
    }

    /// Width of `text` in points (cached).
    pub fn measure(&mut self, cache: &mut WidthCache, text: &str, req: FontRequest, size: f64) -> f64 {
        if cache.generation != self.generation {
            cache.map.clear();
            cache.generation = self.generation;
        }
        let key = (text.to_string(), req.family.to_string(), size.to_bits(), req.bold, req.italic);
        if let Some(w) = cache.map.get(&key) {
            return *w;
        }
        let w = self.shape(text, req, size).width;
        cache.map.insert(key, w);
        w
    }
}

fn shape_run(text: &str, offset: usize, font: ResolvedFont, face: Arc<LoadedFace>, size: f64) -> ShapedRun {
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let output = rustybuzz::shape(face.face(), &[], buffer);
    let scale = size / face.units_per_em;
    let infos = output.glyph_infos();
    let positions = output.glyph_positions();

    let mut glyphs = Vec::with_capacity(infos.len());
    let mut width = 0.0;
    for (i, (info, pos)) in infos.iter().zip(positions).enumerate() {
        let start = info.cluster as usize;
        let end = infos[i + 1..]
            .iter()
            .map(|n| n.cluster as usize)
            .find(|&c| c != start)
            .unwrap_or(text.len());
        let mut advance = pos.x_advance as f64 * scale;
        if font.synthetic_bold {
            advance += SYNTHETIC_BOLD_OFFSET;
        }
        width += advance;
        glyphs.push(ShapedGlyph {
            glyph_id: info.glyph_id as u16,
            cluster: (offset + start, offset + end.max(start)),
            x_advance: advance,
            x_offset: pos.x_offset as f64 * scale,
            y_offset: pos.y_offset as f64 * scale,
        });
    }
    ShapedRun { font, face, range: offset..offset + text.len(), glyphs, width }
}
