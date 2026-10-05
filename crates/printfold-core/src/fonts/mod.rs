//! Font discovery, family resolution, shaping and measurement.

pub mod registry;
pub mod shaping;
pub mod woff;

pub use registry::{FaceRef, FamilyVariants, FontBytes, FontRegistry, LoadedFace, ResolvedFont};
pub use shaping::{FontRequest, ShapedGlyph, ShapedRun, ShapedText, WidthCache};

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> FontRegistry {
        let mut r = FontRegistry::new();
        r.load_system_fonts();
        r
    }

    #[test]
    fn resolves_and_measures() {
        let mut r = registry();
        if r.face_count() == 0 {
            return; // no fonts in this environment
        }
        let mut cache = WidthCache::default();
        let narrow = r.measure(&mut cache, "iiii", FontRequest::new("DejaVu Sans", false, false), 12.0);
        let wide = r.measure(&mut cache, "WWWW", FontRequest::new("DejaVu Sans", false, false), 12.0);
        assert!(narrow > 0.0 && wide > narrow);
        let double = r.measure(&mut cache, "WWWW", FontRequest::new("DejaVu Sans", false, false), 24.0);
        assert!((double - wide * 2.0).abs() < 0.01);
        // Unknown families fall back rather than failing.
        assert!(r.resolve("No Such Family", false, false).is_some());
        // A family list picks the first installed member.
        let a = r.resolve("\"Nope\", DejaVu Serif, serif", false, false).unwrap();
        let b = r.resolve("DejaVu Serif", false, false).unwrap();
        assert_eq!(a.face, b.face);
    }

    #[test]
    fn bold_uses_real_face_when_available() {
        let mut r = registry();
        if !r.has_family("DejaVu Sans") {
            return;
        }
        let regular = r.resolve("DejaVu Sans", false, false).unwrap();
        let bold = r.resolve("DejaVu Sans", true, false).unwrap();
        assert_ne!(regular.face, bold.face);
        assert!(!bold.synthetic_bold);
        assert!(r.family_variants("DejaVu Sans").bold);
    }
}
