//! Sheet assembly: booklet imposition (with multi-row fill), sequential
//! single/double-sided layouts, print marks and the duplex calibration page.
//! Coordinates are top-left; the original pdf-lib code's bottom-left maths
//! is converted with `y_top = sheet_height - y_pdf`.

use std::collections::HashMap;

use krilla::page::PageSettings;
use krilla::surface::Surface;
use krilla::Document;

use super::canvas::{color_or_black, Painter, Rgba, TextStyle};
use super::page::{draw_page, PageBox};
use super::PdfContext;
use crate::flow::{calculate_imposition, ImpositionSheet};
use crate::model::{oriented_sheet_size, page_size, spread_rows_per_sheet, PageContent, Size, SpanningItem};

pub(crate) struct Layout<'a> {
    pub sheet: Size,
    pub page: Size,
    pub pages: HashMap<u32, &'a PageContent>,
    /// Spanning items of the legacy static spread containing each page.
    pub spanning: HashMap<u32, &'a [SpanningItem]>,
}

impl<'a> Layout<'a> {
    pub fn new(ctx: &'a PdfContext) -> Self {
        let output = &ctx.project.output_options;
        let mut pages = HashMap::new();
        let mut spanning = HashMap::new();
        let statics = ctx.project.static_spreads.as_deref().unwrap_or(&[]);
        for sig in &ctx.project.signatures {
            for spread in &sig.spreads {
                let items = statics.iter().find(|st| st.id == spread.id).and_then(|st| st.spanning_items.as_deref());
                for page in spread.verso.iter().chain(spread.recto.iter()) {
                    pages.insert(page.page_number, page);
                    if let Some(items) = items {
                        spanning.insert(page.page_number, items);
                    }
                }
            }
        }
        Self { sheet: oriented_sheet_size(&output.sheet_size, &output.orientation), page: page_size(output), pages, spanning }
    }

    fn adjacent(&self, page: &PageContent) -> Option<&'a PageContent> {
        let n = if page.is_recto { page.page_number.checked_sub(1)? } else { page.page_number + 1 };
        self.pages.get(&n).copied()
    }

    fn draw(&self, p: &mut Painter, s: &mut Surface, ctx: &PdfContext, number: u32, x: f64, top: f64) {
        if let Some(page) = self.pages.get(&number) {
            let bx = PageBox { x, top, width: self.page.width, height: self.page.height };
            draw_page(p, s, ctx, page, bx, self.adjacent(page), self.spanning.get(&number).copied());
        }
    }

    fn settings(&self) -> PageSettings {
        PageSettings::from_wh(self.sheet.width as f32, self.sheet.height as f32).expect("valid sheet size")
    }
}

/// Booklet mode: imposed sheets, grouped `rows` per physical sheet across
/// signatures; fronts carry the duplex offset.
pub(crate) fn booklet(doc: &mut Document, p: &mut Painter, ctx: &PdfContext, layout: &Layout) {
    let output = &ctx.project.output_options;
    let rows = spread_rows_per_sheet(layout.sheet, layout.page.height, output.fill_available_space);
    let sheets: Vec<ImpositionSheet> = ctx.project.signatures.iter().flat_map(calculate_imposition).collect();
    let dx = output.duplex_offset_x.unwrap_or(0.0);
    let dy = output.duplex_offset_y.unwrap_or(0.0);

    for group in sheets.chunks(rows) {
        for front in [true, false] {
            let mut page = doc.start_page_with(layout.settings());
            let mut s = page.surface();
            for (row, sheet) in group.iter().enumerate() {
                let top = row as f64 * layout.page.height;
                let side = if front { sheet.front } else { sheet.back };
                // PDF y-up offset → move up = smaller top.
                let (ox, oy) = if front { (dx, -dy) } else { (0.0, 0.0) };
                layout.draw(p, &mut s, ctx, side.left, ox, top + oy);
                layout.draw(p, &mut s, ctx, side.right, layout.page.width + ox, top + oy);
            }
            if !ctx.project.signatures.is_empty() {
                print_marks(p, &mut s, ctx, layout, rows);
            }
            s.finish();
            page.finish();
        }
    }
}

/// Single/double-sided sequential pages placed by autofill, centre or
/// upper-left.
pub(crate) fn sequential(doc: &mut Document, p: &mut Painter, ctx: &PdfContext, layout: &Layout) {
    let output = &ctx.project.output_options;
    let placement = output.placement();
    let mut numbers: Vec<u32> = ctx.project.signatures.iter().flat_map(|s| s.pages().map(|p| p.page_number)).collect();
    numbers.dedup();
    let cols = ((layout.sheet.width / layout.page.width).floor() as usize).max(1);
    let rows = ((layout.sheet.height / layout.page.height).floor() as usize).max(1);
    let slots = if placement == "autofill" { cols * rows } else { 1 };
    let slot_pos = |slot: usize| -> (f64, f64) {
        match placement {
            "center" => ((layout.sheet.width - layout.page.width) / 2.0, (layout.sheet.height - layout.page.height) / 2.0),
            "upperLeft" => (0.0, 0.0),
            _ => ((slot % cols) as f64 * layout.page.width, (slot / cols) as f64 * layout.page.height),
        }
    };

    if output.booklet_type() == "doubleSided" {
        for chunk in numbers.chunks(slots * 2) {
            let mut page = doc.start_page_with(layout.settings());
            let mut s = page.surface();
            for slot in 0..slots {
                if let Some(&n) = chunk.get(slot * 2) {
                    let (x, y) = slot_pos(slot);
                    layout.draw(p, &mut s, ctx, n, x, y);
                }
            }
            s.finish();
            page.finish();
            if chunk.len() > 1 {
                let mut page = doc.start_page_with(layout.settings());
                let mut s = page.surface();
                for slot in 0..slots {
                    if let Some(&n) = chunk.get(slot * 2 + 1) {
                        let (x, y) = slot_pos(slot);
                        layout.draw(p, &mut s, ctx, n, x, y);
                    }
                }
                s.finish();
                page.finish();
            }
        }
    } else {
        for chunk in numbers.chunks(slots) {
            let mut page = doc.start_page_with(layout.settings());
            let mut s = page.surface();
            for (slot, &n) in chunk.iter().enumerate() {
                let (x, y) = slot_pos(slot);
                layout.draw(p, &mut s, ctx, n, x, y);
            }
            s.finish();
            page.finish();
        }
    }
}

const MARK_LENGTH: f64 = 18.0;
const MARK_OFFSET: f64 = 9.0;

/// Corner crop marks, optional centre fold marks, and row cut marks.
fn print_marks(p: &mut Painter, s: &mut Surface, ctx: &PdfContext, layout: &Layout, rows: usize) {
    let output = &ctx.project.output_options;
    let (w, h) = (layout.sheet.width, layout.sheet.height);
    let fold = Rgba::gray(0.7);
    let cx = w / 2.0;
    let (l, o) = (MARK_LENGTH, MARK_OFFSET);

    if !output.show_crop_marks.unwrap_or(true) {
        if output.show_fold_marks {
            p.line(s, cx, o, cx, o + l, fold, 0.5);
            p.line(s, cx, h - o, cx, h - o - l, fold, 0.5);
            for row in 1..rows {
                let y = row as f64 * layout.page.height;
                p.line(s, cx - l / 2.0, y, cx + l / 2.0, y, fold, 0.5);
            }
        }
        return;
    }

    let color = color_or_black(output.crop_mark_color.as_deref().unwrap_or("#000000"));
    let t = output.crop_mark_thickness.unwrap_or(0.5);
    for (x, y, sx, sy) in [(o, o, 1.0, 1.0), (w - o, o, -1.0, 1.0), (o, h - o, 1.0, -1.0), (w - o, h - o, -1.0, -1.0)] {
        p.line(s, x, y, x, y + sy * l, color, t);
        p.line(s, x, y, x + sx * l, y, color, t);
    }
    if output.show_fold_marks {
        p.line(s, cx, o, cx, o + l, fold, t);
        p.line(s, cx, h - o, cx, h - o - l, fold, t);
    }
    for row in 1..rows {
        let y = row as f64 * layout.page.height;
        p.line(s, o, y, o + l, y, color, t);
        p.line(s, w - o, y, w - o - l, y, color, t);
        if output.show_fold_marks {
            p.line(s, cx - l / 2.0, y, cx + l / 2.0, y, fold, t);
        }
    }
}

/// Two-page duplex calibration sheet: corner crosses and a millimetre
/// grid on the front (offset applied), crosses only on the back.
pub(crate) fn test_page(doc: &mut Document, p: &mut Painter, ctx: &PdfContext) {
    let output = &ctx.project.output_options;
    let sheet = oriented_sheet_size(&output.sheet_size, &output.orientation);
    let ox = output.duplex_offset_x.unwrap_or(0.0);
    let oy = output.duplex_offset_y.unwrap_or(0.0);
    let settings = || PageSettings::from_wh(sheet.width as f32, sheet.height as f32).expect("valid sheet size");

    let mut page = doc.start_page_with(settings());
    let mut s = page.surface();
    corner_crosses(p, &mut s, sheet, ox, oy);
    calibration_grid(p, &mut s, sheet, ox, oy, true);
    let text = format!("Duplex Offset: X={:.1}mm, Y={:.1}mm", ox * 25.4 / 72.0, oy * 25.4 / 72.0);
    let bold = TextStyle { family: "Helvetica, Arial, sans-serif", size: 20.0, bold: true, italic: false, color: Rgba::BLACK, opacity: 1.0 };
    let tw = p.text_width(&text, &bold);
    p.draw_text(&mut s, &text, sheet.width / 2.0 - tw / 2.0 + ox, sheet.height / 2.0 - 100.0 - oy, &bold);
    s.finish();
    page.finish();

    let mut page = doc.start_page_with(settings());
    let mut s = page.surface();
    corner_crosses(p, &mut s, sheet, 0.0, 0.0);
    calibration_grid(p, &mut s, sheet, 0.0, 0.0, false);
    s.finish();
    page.finish();
}

fn corner_crosses(p: &mut Painter, s: &mut Surface, sheet: Size, ox: f64, oy: f64) {
    let arm = 20.0;
    for (x, y) in [(36.0, 36.0), (sheet.width - 36.0, 36.0), (36.0, sheet.height - 36.0), (sheet.width - 36.0, sheet.height - 36.0)] {
        let (x, y) = (x + ox, y - oy);
        p.line(s, x - arm, y, x + arm, y, Rgba::BLACK, 0.5);
        p.line(s, x, y - arm, x, y + arm, Rgba::BLACK, 0.5);
    }
}

fn calibration_grid(p: &mut Painter, s: &mut Surface, sheet: Size, ox: f64, oy: f64, hash_marks: bool) {
    let cx = sheet.width / 2.0 + ox;
    let cy = sheet.height / 2.0 - oy;
    let mm = 72.0 / 25.4;
    let max_mm = 15;
    let (dark_len, light_len) = (10.0, 5.0);
    p.line(s, cx - 7.5, cy, cx + 7.5, cy, Rgba::BLACK, 0.5);
    p.line(s, cx, cy - 7.5, cx, cy + 7.5, Rgba::BLACK, 0.5);
    if !hash_marks {
        return;
    }
    for i in 1..=max_mm {
        let d = i as f64 * mm;
        let dark = i % 5 == 0;
        let (len, color) = if dark { (dark_len, Rgba::BLACK) } else { (light_len, Rgba::gray(0.7)) };
        for x in [cx + d, cx - d] {
            p.line(s, x, cy - len, x, cy + len, color, 0.5);
        }
        for y in [cy - d, cy + d] {
            p.line(s, cx - len, y, cx + len, y, color, 0.5);
        }
    }
    let label = TextStyle { family: "Helvetica, Arial, sans-serif", size: 10.0, bold: false, italic: false, color: Rgba::BLACK, opacity: 1.0 };
    let reach = max_mm as f64 * mm + dark_len + 5.0;
    p.draw_text(s, "+X", cx + reach, cy + 5.0, &label);
    let w = p.text_width("-X", &label);
    p.draw_text(s, "-X", cx - reach - w, cy + 5.0, &label);
    let w = p.text_width("+Y", &label);
    p.draw_text(s, "+Y", cx - w / 2.0, cy - reach, &label);
    let w = p.text_width("-Y", &label);
    p.draw_text(s, "-Y", cx - w / 2.0, cy + reach + 10.0, &label);
    let desc = TextStyle { size: 9.0, ..label.clone() };
    let text = "Each dark line represents 5mm";
    let w = p.text_width(text, &desc);
    p.draw_text(s, text, cx - w / 2.0, cy + max_mm as f64 * mm + dark_len + 30.0, &desc);
}
