//! WOFF 1.0 → SFNT (TrueType/OpenType) conversion.
//!
//! Users can upload `.woff` fonts; WebKit renders them directly, but the
//! shaper and the PDF embedder need the plain SFNT tables. WOFF 1.0 is a
//! table directory with optionally zlib-compressed tables, so unpacking is
//! straightforward. (WOFF2 is not accepted by the app.)

const WOFF_SIGNATURE: &[u8; 4] = b"wOFF";
const HEADER_LEN: usize = 44;
const DIR_ENTRY_LEN: usize = 20;

fn be_u16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be_u32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

pub fn is_woff(bytes: &[u8]) -> bool {
    bytes.starts_with(WOFF_SIGNATURE)
}

/// Unpack a WOFF 1.0 font; returns `None` if the data is malformed.
pub fn woff_to_sfnt(bytes: &[u8]) -> Option<Vec<u8>> {
    if !is_woff(bytes) {
        return None;
    }
    let flavor = be_u32(bytes, 4)?;
    let num_tables = be_u16(bytes, 12)? as usize;

    struct Table {
        tag: u32,
        checksum: u32,
        data: Vec<u8>,
    }
    let mut tables = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let e = HEADER_LEN + i * DIR_ENTRY_LEN;
        let tag = be_u32(bytes, e)?;
        let offset = be_u32(bytes, e + 4)? as usize;
        let comp_len = be_u32(bytes, e + 8)? as usize;
        let orig_len = be_u32(bytes, e + 12)? as usize;
        let checksum = be_u32(bytes, e + 16)?;
        let raw = bytes.get(offset..offset.checked_add(comp_len)?)?;
        let data = if comp_len < orig_len {
            let out = miniz_oxide::inflate::decompress_to_vec_zlib(raw).ok()?;
            if out.len() != orig_len {
                return None;
            }
            out
        } else {
            raw.to_vec()
        };
        tables.push(Table { tag, checksum, data });
    }
    tables.sort_by_key(|t| t.tag);

    let n = tables.len() as u16;
    let mut entry_selector = 0u16;
    while (1u32 << (entry_selector + 1)) <= n as u32 {
        entry_selector += 1;
    }
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = n * 16 - search_range;

    let mut out = Vec::new();
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    let mut offset = 12 + 16 * tables.len();
    for t in &tables {
        out.extend_from_slice(&t.tag.to_be_bytes());
        out.extend_from_slice(&t.checksum.to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(t.data.len() as u32).to_be_bytes());
        offset += t.data.len().div_ceil(4) * 4;
    }
    for t in &tables {
        out.extend_from_slice(&t.data);
        out.resize(out.len().div_ceil(4) * 4, 0);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a WOFF from SFNT tables (compressing each) for round-tripping.
    fn sfnt_to_woff(sfnt: &[u8]) -> Vec<u8> {
        let num = be_u16(sfnt, 4).unwrap() as usize;
        let mut dir = Vec::new();
        let mut data = Vec::new();
        let data_start = HEADER_LEN + num * DIR_ENTRY_LEN;
        for i in 0..num {
            let r = 12 + i * 16;
            let tag = be_u32(sfnt, r).unwrap();
            let checksum = be_u32(sfnt, r + 4).unwrap();
            let off = be_u32(sfnt, r + 8).unwrap() as usize;
            let len = be_u32(sfnt, r + 12).unwrap() as usize;
            let table = &sfnt[off..off + len];
            let comp = miniz_oxide::deflate::compress_to_vec_zlib(table, 6);
            let (stored, comp_len) = if comp.len() < len { (comp, None) } else { (table.to_vec(), Some(len)) };
            let comp_len = comp_len.unwrap_or(stored.len());
            dir.extend_from_slice(&tag.to_be_bytes());
            dir.extend_from_slice(&((data_start + data.len()) as u32).to_be_bytes());
            dir.extend_from_slice(&(comp_len as u32).to_be_bytes());
            dir.extend_from_slice(&(len as u32).to_be_bytes());
            dir.extend_from_slice(&checksum.to_be_bytes());
            data.extend_from_slice(&stored);
            data.resize(data.len().div_ceil(4) * 4, 0);
        }
        let mut w = Vec::new();
        w.extend_from_slice(WOFF_SIGNATURE);
        w.extend_from_slice(&sfnt[0..4]);
        w.extend_from_slice(&((data_start + data.len()) as u32).to_be_bytes());
        w.extend_from_slice(&(num as u16).to_be_bytes());
        w.extend_from_slice(&[0, 0]);
        w.extend_from_slice(&(sfnt.len() as u32).to_be_bytes());
        w.extend_from_slice(&[0, 1, 0, 0]);
        w.extend_from_slice(&[0; 20]);
        w.extend_from_slice(&dir);
        w.extend_from_slice(&data);
        w
    }

    #[test]
    fn round_trips_a_system_font() {
        let candidates = ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/System/Library/Fonts/Supplemental/Arial.ttf"];
        let Some(sfnt) = candidates.iter().find_map(|p| std::fs::read(p).ok()) else { return };
        let woff = sfnt_to_woff(&sfnt);
        assert!(is_woff(&woff));
        let back = woff_to_sfnt(&woff).expect("decode");
        let face = rustybuzz::Face::from_slice(&back, 0).expect("parse");
        assert!(face.glyph_index('A').is_some());
    }

    #[test]
    fn rejects_garbage() {
        assert!(woff_to_sfnt(b"wOFFnot really a font").is_none());
        assert!(woff_to_sfnt(b"OTTO").is_none());
    }
}
