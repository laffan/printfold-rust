//! Booklet imposition (port of `textFlow/imposition.ts`).

use serde::{Deserialize, Serialize};

use crate::model::Signature;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetSide {
    pub left: u32,
    pub right: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpositionSheet {
    pub sheet_number: u32,
    pub front: SheetSide,
    pub back: SheetSide,
}

/// Sheets for one signature. Nested, folded sheets read in order when the
/// front pairs (n, total-n+1) and the back the next two inward pages.
/// Page numbers are global (offset by the signature's position).
pub fn calculate_imposition(signature: &Signature) -> Vec<ImpositionSheet> {
    let count = signature.page_count;
    let base = signature.signature_number.saturating_sub(1) * count;
    (0..count / 4)
        .map(|sheet| ImpositionSheet {
            sheet_number: sheet + 1,
            front: SheetSide { left: count - sheet * 2 + base, right: sheet * 2 + 1 + base },
            back: SheetSide { left: sheet * 2 + 2 + base, right: count - sheet * 2 - 1 + base },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_page_signature() {
        let sig = Signature { signature_number: 2, page_count: 8, ..Default::default() };
        let sheets = calculate_imposition(&sig);
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[0].front, SheetSide { left: 16, right: 9 });
        assert_eq!(sheets[0].back, SheetSide { left: 10, right: 15 });
        assert_eq!(sheets[1].front, SheetSide { left: 14, right: 11 });
        assert_eq!(sheets[1].back, SheetSide { left: 12, right: 13 });
    }
}
