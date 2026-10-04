//! Font discovery and CSS-like family resolution.
//!
//! The webview renders text with the platform's fonts by CSS family name;
//! the layout engine and the PDF writer must use the *same* font files so
//! measurements match what the user sees. [`FontRegistry`] indexes system
//! fonts (via `fontdb`) plus user-uploaded custom fonts and resolves a
//! family/weight/style request the way WebKit would: exact family match,
//! nearest weight/style within the family, and synthetic bold/italic when
//! the family lacks a real face.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use fontdb::{Database, Source, Style, Weight, ID};
use self_cell::self_cell;

/// Identifies a face: a system face from `fontdb` or a custom font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaceRef {
    System(ID),
    Custom(u32),
}

pub type FontBytes = Arc<Vec<u8>>;

self_cell!(
    struct FaceCell {
        owner: FontBytes,
        #[covariant]
        dependent: RbFace,
    }
);

type RbFace<'a> = rustybuzz::Face<'a>;

/// A parsed face ready for shaping, plus the raw bytes for PDF embedding.
pub struct LoadedFace {
    cell: FaceCell,
    pub index: u32,
    pub units_per_em: f64,
    pub weight: u16,
    pub italic: bool,
    /// Ascent and descent as fractions of the em (both positive).
    pub ascent: f64,
    pub descent: f64,
}

impl LoadedFace {
    fn load(bytes: FontBytes, index: u32) -> Option<Self> {
        let cell = FaceCell::try_new(bytes, |b| rustybuzz::Face::from_slice(b, index).ok_or(())).ok()?;
        let face = cell.borrow_dependent();
        let units_per_em = face.units_per_em() as f64;
        let weight = face.weight().to_number();
        let italic = face.is_italic() || face.is_oblique();
        let ascent = face.ascender() as f64 / units_per_em;
        let descent = -(face.descender() as f64) / units_per_em;
        Some(Self { cell, index, units_per_em, weight, italic, ascent, descent })
    }

    pub fn face(&self) -> &rustybuzz::Face<'_> {
        self.cell.borrow_dependent()
    }

    pub fn bytes(&self) -> &FontBytes {
        self.cell.borrow_owner()
    }

    /// Offset from a text box's top to its baseline as WebKit computes it
    /// for canvas `textBaseline = "middle"` (what Konva uses): the em box is
    /// centred on `top + size / 2`.
    pub fn middle_baseline_offset(&self, size: f64) -> f64 {
        size / 2.0 + (self.ascent - self.descent) / 2.0 * size
    }

    pub fn has_glyph(&self, c: char) -> bool {
        self.face().glyph_index(c).is_some()
    }
}

/// A resolved request: the face to use and which styles must be synthesised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedFont {
    pub face: FaceRef,
    pub synthetic_bold: bool,
    pub synthetic_italic: bool,
}

/// Which variants a family really provides (drives the B/I buttons in the UI).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyVariants {
    pub regular: bool,
    pub bold: bool,
    pub italic: bool,
    pub bold_italic: bool,
}

struct CustomFont {
    family: String,
    bytes: FontBytes,
}

/// Generic CSS families and the fallbacks used when a family is missing.
/// WebKit's default ("standard") font is Times, so unknown families fall
/// back to a serif like the canvas measurement in the original did.
const SERIF_FALLBACKS: &[&str] = &["Times", "Times New Roman", "Liberation Serif", "DejaVu Serif", "FreeSerif"];
const SANS_FALLBACKS: &[&str] = &["Helvetica", "Arial", "Liberation Sans", "DejaVu Sans", "FreeSans"];
const MONO_FALLBACKS: &[&str] = &["Courier", "Courier New", "Menlo", "Liberation Mono", "DejaVu Sans Mono", "FreeMono"];

/// Metric-compatible stand-ins for common families that may be missing,
/// mirroring the aliases WebKit gets from the platform (fontconfig on Linux,
/// CoreText on Apple) so the layout engine picks the same face the webview
/// renders with.
const FAMILY_ALIASES: &[(&str, &[&str])] = &[
    ("arial", &["Helvetica", "Liberation Sans", "Arimo"]),
    ("helvetica", &["Helvetica Neue", "Arial", "Liberation Sans", "Arimo"]),
    ("helvetica neue", &["Helvetica", "Arial", "Liberation Sans"]),
    ("times new roman", &["Times", "Liberation Serif", "Tinos"]),
    ("times", &["Times New Roman", "Liberation Serif", "Tinos"]),
    ("courier new", &["Courier", "Liberation Mono", "Cousine"]),
    ("courier", &["Courier New", "Liberation Mono", "Cousine"]),
    ("georgia", &["Gelasio", "DejaVu Serif"]),
    ("verdana", &["DejaVu Sans"]),
    ("menlo", &["DejaVu Sans Mono"]),
    ("monaco", &["Menlo", "DejaVu Sans Mono"]),
];

/// Order of last-resort fallbacks. WebKit on Apple platforms falls back to
/// Times for unknown families; on Linux fontconfig's default is a sans.
#[cfg(any(target_os = "macos", target_os = "ios"))]
const DEFAULT_FALLBACKS: [&[&str]; 2] = [SERIF_FALLBACKS, SANS_FALLBACKS];
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
const DEFAULT_FALLBACKS: [&[&str]; 2] = [SANS_FALLBACKS, SERIF_FALLBACKS];

pub struct FontRegistry {
    db: Database,
    families: HashMap<String, Vec<ID>>,
    display_names: HashMap<String, String>,
    custom: HashMap<u32, CustomFont>,
    custom_by_family: HashMap<String, u32>,
    next_custom: u32,
    loaded: HashMap<FaceRef, Option<Arc<LoadedFace>>>,
    file_cache: HashMap<String, FontBytes>,
    resolve_cache: HashMap<(String, bool, bool), Option<ResolvedFont>>,
    pub(crate) char_fallback: HashMap<char, Option<FaceRef>>,
    /// Bumped whenever the set of fonts changes (invalidates width caches).
    pub generation: u64,
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FontRegistry {
    pub fn new() -> Self {
        Self {
            db: Database::new(),
            families: HashMap::new(),
            display_names: HashMap::new(),
            custom: HashMap::new(),
            custom_by_family: HashMap::new(),
            next_custom: 1,
            loaded: HashMap::new(),
            file_cache: HashMap::new(),
            resolve_cache: HashMap::new(),
            char_fallback: HashMap::new(),
            generation: 0,
        }
    }

    /// Scan the platform font directories (macOS, iOS, Linux, Windows).
    pub fn load_system_fonts(&mut self) {
        self.db.load_system_fonts();
        self.reindex();
    }

    pub fn load_fonts_dir(&mut self, dir: impl AsRef<Path>) {
        self.db.load_fonts_dir(dir);
        self.reindex();
    }

    pub fn load_font_data(&mut self, data: Vec<u8>) {
        self.db.load_font_data(data);
        self.reindex();
    }

    fn reindex(&mut self) {
        self.families.clear();
        self.display_names.clear();
        for face in self.db.faces() {
            for (name, _) in &face.families {
                let key = name.to_lowercase();
                self.families.entry(key.clone()).or_default().push(face.id);
                self.display_names.entry(key).or_insert_with(|| name.clone());
            }
        }
        self.invalidate();
    }

    fn invalidate(&mut self) {
        self.resolve_cache.clear();
        self.char_fallback.clear();
        self.generation += 1;
    }

    /// Register an uploaded font under `family` (replacing any previous one).
    pub fn register_custom_font(&mut self, family: &str, bytes: Vec<u8>) -> bool {
        if rustybuzz::Face::from_slice(&bytes, 0).is_none() {
            return false;
        }
        self.unregister_custom_font(family);
        let id = self.next_custom;
        self.next_custom += 1;
        self.custom.insert(id, CustomFont { family: family.to_string(), bytes: Arc::new(bytes) });
        self.custom_by_family.insert(family.to_lowercase(), id);
        self.invalidate();
        true
    }

    pub fn unregister_custom_font(&mut self, family: &str) {
        if let Some(id) = self.custom_by_family.remove(&family.to_lowercase()) {
            self.custom.remove(&id);
            self.loaded.remove(&FaceRef::Custom(id));
            self.invalidate();
        }
    }

    /// Take over the custom fonts registered on another registry (used when
    /// a freshly scanned system registry replaces a temporary one).
    pub fn adopt_custom_fonts(&mut self, other: FontRegistry) {
        for (_, font) in other.custom {
            let bytes = Arc::try_unwrap(font.bytes).unwrap_or_else(|arc| (*arc).clone());
            self.register_custom_font(&font.family, bytes);
        }
    }

    pub fn custom_families(&self) -> Vec<String> {
        let mut v: Vec<String> = self.custom.values().map(|c| c.family.clone()).collect();
        v.sort();
        v
    }

    /// Installed family names suitable for a font picker (hidden `.`-prefixed
    /// system families excluded), sorted case-insensitively.
    pub fn system_families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .display_names
            .values()
            .filter(|n| !n.starts_with('.') && !n.is_empty())
            .cloned()
            .collect();
        names.sort_by_key(|a| a.to_lowercase());
        names.dedup();
        names
    }

    pub fn has_family(&self, family: &str) -> bool {
        let key = family.trim().to_lowercase();
        self.custom_by_family.contains_key(&key) || self.families.contains_key(&key)
    }

    /// Which real faces a family offers.
    pub fn family_variants(&self, family: &str) -> FamilyVariants {
        let key = family.trim().to_lowercase();
        if self.custom_by_family.contains_key(&key) {
            return FamilyVariants { regular: true, ..Default::default() };
        }
        let mut v = FamilyVariants::default();
        for id in self.families.get(&key).into_iter().flatten() {
            let Some(info) = self.db.face(*id) else { continue };
            let bold = info.weight.0 >= 600;
            let italic = info.style != Style::Normal;
            match (bold, italic) {
                (false, false) => v.regular = true,
                (true, false) => v.bold = true,
                (false, true) => v.italic = true,
                (true, true) => v.bold_italic = true,
            }
        }
        v
    }

    /// Split a CSS family list (`"Palatino Linotype", Palatino, serif`).
    fn family_candidates(family: &str) -> Vec<String> {
        family
            .split(',')
            .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_string())
            .filter(|f| !f.is_empty())
            .collect()
    }

    fn generic_fallbacks(name: &str) -> Option<&'static [&'static str]> {
        match name.to_lowercase().as_str() {
            "serif" | "ui-serif" => Some(SERIF_FALLBACKS),
            "sans-serif" | "system-ui" | "ui-sans-serif" | "-apple-system" => Some(SANS_FALLBACKS),
            "monospace" | "ui-monospace" => Some(MONO_FALLBACKS),
            _ => None,
        }
    }

    fn best_in_family(&self, key: &str, bold: bool, italic: bool) -> Option<ResolvedFont> {
        if let Some(&id) = self.custom_by_family.get(key) {
            return Some(ResolvedFont { face: FaceRef::Custom(id), synthetic_bold: bold, synthetic_italic: italic });
        }
        let ids = self.families.get(key)?;
        let want_weight = if bold { 700 } else { 400 };
        let want_style = if italic { Style::Italic } else { Style::Normal };
        let mut best: Option<(i32, ID, Weight, Style)> = None;
        for id in ids {
            let Some(info) = self.db.face(*id) else { continue };
            // Prefer matching style strongly, then the closest weight.
            let style_penalty = match (want_style, info.style) {
                (a, b) if a == b => 0,
                (Style::Italic, Style::Oblique) | (Style::Oblique, Style::Italic) => 50,
                _ => 10_000,
            };
            let mut weight_penalty = (info.weight.0 as i32 - want_weight).abs();
            // CSS: for bold requests heavier faces win ties; for normal, lighter.
            if bold && (info.weight.0 as i32) < want_weight {
                weight_penalty += 300;
            }
            let stretch_penalty = (info.stretch.to_number() as i32 - 5).abs() * 200;
            let score = style_penalty + weight_penalty + stretch_penalty;
            if best.map(|b| score < b.0).unwrap_or(true) {
                best = Some((score, *id, info.weight, info.style));
            }
        }
        let (_, id, weight, style) = best?;
        Some(ResolvedFont {
            face: FaceRef::System(id),
            synthetic_bold: bold && weight.0 < 600,
            synthetic_italic: italic && style == Style::Normal,
        })
    }

    /// Resolve a CSS family (or family list) + bold/italic to a face.
    pub fn resolve(&mut self, family: &str, bold: bool, italic: bool) -> Option<ResolvedFont> {
        let cache_key = (family.to_string(), bold, italic);
        if let Some(hit) = self.resolve_cache.get(&cache_key) {
            return *hit;
        }
        let mut result = None;
        for name in Self::family_candidates(family) {
            if let Some(generic) = Self::generic_fallbacks(&name) {
                result = generic.iter().find_map(|g| self.best_in_family(&g.to_lowercase(), bold, italic));
            } else {
                let key = name.to_lowercase();
                result = self.best_in_family(&key, bold, italic).or_else(|| {
                    FAMILY_ALIASES
                        .iter()
                        .find(|(k, _)| *k == key)
                        .and_then(|(_, alts)| alts.iter().find_map(|a| self.best_in_family(&a.to_lowercase(), bold, italic)))
                });
            }
            if result.is_some() {
                break;
            }
        }
        if result.is_none() {
            result = DEFAULT_FALLBACKS
                .iter()
                .flat_map(|list| list.iter())
                .find_map(|g| self.best_in_family(&g.to_lowercase(), bold, italic));
        }
        if result.is_none() {
            result = self.db.faces().next().map(|f| ResolvedFont {
                face: FaceRef::System(f.id),
                synthetic_bold: bold,
                synthetic_italic: italic,
            });
        }
        self.resolve_cache.insert(cache_key, result);
        result
    }

    fn read_source(&mut self, source: &Source) -> Option<FontBytes> {
        match source {
            Source::Binary(data) => Some(Arc::new(data.as_ref().as_ref().to_vec())),
            Source::File(path) => {
                let key = path.to_string_lossy().to_string();
                if let Some(bytes) = self.file_cache.get(&key) {
                    return Some(bytes.clone());
                }
                let bytes = Arc::new(std::fs::read(path).ok()?);
                self.file_cache.insert(key, bytes.clone());
                Some(bytes)
            }
        }
    }

    /// Load (and cache) a face for shaping/embedding.
    pub fn load(&mut self, face: FaceRef) -> Option<Arc<LoadedFace>> {
        if let Some(hit) = self.loaded.get(&face) {
            return hit.clone();
        }
        let loaded = match face {
            FaceRef::Custom(id) => {
                let bytes = self.custom.get(&id).map(|c| c.bytes.clone());
                bytes.and_then(|b| LoadedFace::load(b, 0))
            }
            FaceRef::System(id) => {
                let (source, index) = self.db.face_source(id)?;
                self.read_source(&source).and_then(|b| LoadedFace::load(b, index))
            }
        }
        .map(Arc::new);
        self.loaded.insert(face, loaded.clone());
        loaded
    }

    /// Ordered candidates for per-character fallback (emoji, CJK, symbols).
    pub(crate) fn fallback_candidates(&self) -> Vec<FaceRef> {
        const PREFERRED: &[&str] = &[
            "Helvetica", "Arial Unicode MS", "Apple Color Emoji", "PingFang SC", "Hiragino Sans",
            "Apple SD Gothic Neo", "Apple Symbols", "Noto Sans", "Noto Color Emoji", "DejaVu Sans",
            "FreeSerif", "WenQuanYi Zen Hei", "Unifont",
        ];
        let mut out: Vec<FaceRef> = PREFERRED
            .iter()
            .filter_map(|n| self.families.get(&n.to_lowercase()))
            .flat_map(|ids| ids.iter().map(|id| FaceRef::System(*id)))
            .collect();
        for f in self.db.faces() {
            let r = FaceRef::System(f.id);
            if !out.contains(&r) {
                out.push(r);
            }
        }
        out
    }

    /// Family name of a face (for diagnostics / PDF font naming).
    pub fn face_family(&self, face: FaceRef) -> String {
        match face {
            FaceRef::Custom(id) => self.custom.get(&id).map(|c| c.family.clone()).unwrap_or_default(),
            FaceRef::System(id) => self
                .db
                .face(id)
                .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
                .unwrap_or_default(),
        }
    }

    pub fn face_count(&self) -> usize {
        self.db.len()
    }
}
