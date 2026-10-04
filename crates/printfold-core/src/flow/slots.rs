//! Slot-based flow for documents with text-flow regions
//! (port of `textFlow/slotFlow.ts`).
//!
//! Pages and text-flow items on static pages form one ordered sequence of
//! slots; markdown sections are poured through them in document order, so a
//! region behaves like a mini-page embedded in a static page.

use std::collections::{BTreeMap, HashMap};

use super::measure::{font_style_for_section, Measurer};
use super::pagination::{line_count, partial_section};
use super::polygon::{flatten_polygon, horizontal_extent, Pt};
use crate::markdown::footnotes::sentinels_to_numbers;
use crate::model::{
    new_id, FontOptions, FontStyle, LayoutOptions, PageContent, PageDimensions, PageItemType,
    PageState, PolygonFlowLine, Section, SectionType,
};
use crate::text::{apply_text_transform, words};

#[derive(Debug, Clone)]
pub enum SlotKind {
    Page { page_number: u32 },
    Item { host: u32, item_id: String },
    Polygon { host: u32, item_id: String, points: Vec<Pt> },
}

#[derive(Debug, Clone)]
pub struct FlowSlot {
    pub kind: SlotKind,
    pub content_width: f64,
    pub content_height: f64,
    pub sections: Vec<Section>,
    pub polygon_lines: Vec<PolygonFlowLine>,
}

impl FlowSlot {
    fn page(page_number: u32, dims: &PageDimensions) -> Self {
        Self {
            kind: SlotKind::Page { page_number },
            content_width: dims.content_width,
            content_height: dims.content_height,
            sections: Vec::new(),
            polygon_lines: Vec::new(),
        }
    }

    fn is_page(&self) -> bool {
        matches!(self.kind, SlotKind::Page { .. })
    }

    fn has_content(&self) -> bool {
        !self.sections.is_empty() || !self.polygon_lines.is_empty()
    }
}

/// Slots from page 1 to the highest static page: static pages contribute
/// one slot per text-flow item (top-to-bottom), other pages a text slot.
pub fn build_initial_slots(static_pages: &BTreeMap<u32, PageContent>, dims: &PageDimensions) -> Vec<FlowSlot> {
    let mut slots = Vec::new();
    let max = static_pages.keys().max().copied().unwrap_or(0);
    for pn in 1..=max {
        let Some(page) = static_pages.get(&pn) else {
            slots.push(FlowSlot::page(pn, dims));
            continue;
        };
        let mut items: Vec<_> = page.items().iter().filter(|i| i.item_type == PageItemType::TextFlow).collect();
        items.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal).then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)));
        for item in items {
            let padding = item.padding.unwrap_or(0.0);
            if item.is_polygon_flow() {
                let points = flatten_polygon(item.polygon_points.as_deref().unwrap_or(&[]), item.width, item.height);
                slots.push(FlowSlot {
                    kind: SlotKind::Polygon { host: pn, item_id: item.id.clone(), points },
                    content_width: item.width,
                    content_height: item.height,
                    sections: Vec::new(),
                    polygon_lines: Vec::new(),
                });
            } else {
                slots.push(FlowSlot {
                    kind: SlotKind::Item { host: pn, item_id: item.id.clone() },
                    content_width: (item.width - padding * 2.0).max(0.0),
                    content_height: (item.height - padding * 2.0).max(0.0),
                    sections: Vec::new(),
                    polygon_lines: Vec::new(),
                });
            }
        }
    }
    slots
}

struct SlotCursor<'a> {
    slots: Vec<FlowSlot>,
    index: usize,
    height: f64,
    polygon_y: f64,
    next_text_page: u32,
    static_pages: &'a BTreeMap<u32, PageContent>,
    dims: &'a PageDimensions,
    reserved: &'a dyn Fn(usize) -> f64,
}

impl SlotCursor<'_> {
    fn ensure(&mut self) -> usize {
        while self.index >= self.slots.len() {
            while self.static_pages.contains_key(&self.next_text_page) {
                self.next_text_page += 1;
            }
            let pn = self.next_text_page;
            self.next_text_page += 1;
            self.slots.push(FlowSlot::page(pn, self.dims));
        }
        self.index
    }

    fn advance(&mut self) {
        self.index += 1;
        self.height = 0.0;
        self.polygon_y = 0.0;
    }

    /// Content height of slot `idx` after footnote reservations.
    fn effective_height(&self, idx: usize) -> f64 {
        let slot = &self.slots[idx];
        if !slot.is_page() {
            return slot.content_height;
        }
        let page_index = self.slots[..=idx].iter().filter(|s| s.is_page()).count() - 1;
        slot.content_height - (self.reserved)(page_index)
    }

    /// Spread `measured`'s lines from `cursor` across as many slots as
    /// needed. Lines are re-wrapped when a slot's width differs from the
    /// width they were measured at. Returns the unflowed remainder when the
    /// next slot is a polygon region (polygons lay out their own lines).
    #[allow(clippy::too_many_arguments)]
    fn spill_lines(
        &mut self,
        m: &mut Measurer,
        mut measured: Section,
        mut measured_width: f64,
        mut cursor: usize,
        line_height: f64,
        fonts: &FontOptions,
        layout: &LayoutOptions,
    ) -> Option<Section> {
        let mut empty_skips = 0;
        while cursor < line_count(&measured) {
            let idx = self.ensure();
            if matches!(self.slots[idx].kind, SlotKind::Polygon { .. }) {
                return Some(remainder_from_lines(&measured, cursor));
            }
            let width = self.slots[idx].content_width;
            if (width - measured_width).abs() > 0.5 {
                measured = m.rewrap_section(&measured, cursor, width, fonts, layout);
                measured_width = width;
                cursor = 0;
            }
            let total = line_count(&measured);
            let per_slot = (self.effective_height(idx) / line_height).floor();
            if per_slot < 1.0 {
                // Slot too small. Guard against an endless run of tiny slots.
                empty_skips += 1;
                if empty_skips > 10_000 {
                    break;
                }
                self.advance();
                continue;
            }
            let take = (total - cursor).min(per_slot as usize);
            let end = cursor + take;
            let last = end >= total;
            self.slots[idx].sections.push(partial_section(&measured, cursor, end, line_height, last, layout));
            self.height = take as f64 * line_height + if last { layout.paragraph_spacing } else { 0.0 };
            cursor = end;
            if cursor < total {
                self.advance();
            }
        }
        None
    }
}

/// Plain-text remainder of a measured section from line `from` onward.
fn remainder_from_lines(section: &Section, from: usize) -> Section {
    let text = section.lines()[from.min(section.lines().len())..].join(" ");
    let mut rest = section.clone();
    rest.content = text.clone();
    rest.raw_markdown = text;
    rest.measured_height = None;
    rest.lines = None;
    rest.rich_lines = None;
    rest.line_heights = None;
    rest
}

/// Pour sections through the slot sequence, auto-extending with text pages.
#[allow(clippy::too_many_arguments)]
pub fn flow_sections_into_slots(
    m: &mut Measurer,
    sections: &[Section],
    initial: Vec<FlowSlot>,
    dims: &PageDimensions,
    static_pages: &BTreeMap<u32, PageContent>,
    fonts: &FontOptions,
    layout: &LayoutOptions,
    reserved: &dyn Fn(usize) -> f64,
) -> Vec<FlowSlot> {
    let mut next_text_page = 1;
    for s in &initial {
        if let SlotKind::Page { page_number } = s.kind {
            next_text_page = next_text_page.max(page_number + 1);
        }
    }
    let mut c = SlotCursor { slots: initial, index: 0, height: 0.0, polygon_y: 0.0, next_text_page, static_pages, dims, reserved };

    let mut queue: Vec<Section> = sections.to_vec();
    let mut i = 0;
    let mut polygon_retries = 0usize;
    while i < queue.len() {
        let section = queue[i].clone();

        if section.section_type == SectionType::Hr {
            let idx = c.ensure();
            if c.slots[idx].has_content() {
                c.advance();
            }
            i += 1;
            continue;
        }

        let mut idx = c.ensure();
        let is_h1 = section.section_type == SectionType::Heading && section.level == Some(1);
        if is_h1 && layout.empty_page_before_h1 && c.slots[idx].is_page() && !c.slots[idx].sections.is_empty() {
            c.advance();
            idx = c.ensure();
        }

        if let SlotKind::Polygon { points, .. } = &c.slots[idx].kind {
            let points = points.clone();
            let max_y = c.slots[idx].content_height;
            let start_y = c.polygon_y;
            let out = flow_into_polygon(m, &section, &points, max_y, start_y, fonts, layout);
            c.slots[idx].polygon_lines.extend(out.lines);
            c.polygon_y = out.next_y;
            match out.remainder {
                None => i += 1,
                Some(rest) => {
                    c.advance();
                    // Retry the remainder in the next slot (bounded so a
                    // region too small for any word can't spin forever).
                    polygon_retries += 1;
                    if polygon_retries > 10_000 {
                        i += 1;
                    } else {
                        queue[i] = rest;
                    }
                }
            }
            continue;
        }

        let measured = m.measure_section(&section, c.slots[idx].content_width, fonts, layout);
        let mh = measured.measured_height.unwrap_or(0.0);
        if c.height + mh <= c.effective_height(idx) {
            c.slots[idx].sections.push(measured);
            c.height += mh;
            i += 1;
            continue;
        }

        let style = font_style_for_section(section.section_type, section.level, fonts);
        let line_height = layout.line_height * style.font_size;
        let splittable = matches!(section.section_type, SectionType::Paragraph | SectionType::Blockquote)
            && measured.lines().len() > 1;

        if splittable {
            let mut cursor = 0;
            let total = line_count(&measured);
            let fit = ((c.effective_height(idx) - c.height) / line_height).floor();
            if fit >= 2.0 {
                let end = (fit as usize).min(total);
                c.slots[idx].sections.push(partial_section(&measured, 0, end, line_height, false, layout));
                cursor = end;
                c.advance();
            } else if !c.slots[idx].sections.is_empty() {
                c.advance();
            }
            let width = c.slots[idx].content_width;
            match c.spill_lines(m, measured, width, cursor, line_height, fonts, layout) {
                Some(rest) => queue[i] = rest,
                None => i += 1,
            }
            continue;
        }

        if !c.slots[idx].sections.is_empty() {
            c.advance();
        }
        let idx = c.ensure();
        if mh <= c.effective_height(idx) || line_count(&measured) == 0 {
            c.slots[idx].sections.push(measured);
            c.height = mh;
            i += 1;
            continue;
        }
        let width = c.slots[idx].content_width;
        match c.spill_lines(m, measured, width, 0, line_height, fonts, layout) {
            Some(rest) => queue[i] = rest,
            None => i += 1,
        }
    }
    c.slots
}

struct PolygonOutcome {
    lines: Vec<PolygonFlowLine>,
    next_y: f64,
    remainder: Option<Section>,
}

/// Lay a section into a polygon line by line, using the polygon's width at
/// each line's vertical centre.
fn flow_into_polygon(
    m: &mut Measurer,
    section: &Section,
    points: &[Pt],
    max_y: f64,
    start_y: f64,
    fonts: &FontOptions,
    layout: &LayoutOptions,
) -> PolygonOutcome {
    let mut lines = Vec::new();
    if matches!(section.section_type, SectionType::Image | SectionType::Hr) {
        return PolygonOutcome { lines, next_y: start_y, remainder: None };
    }
    let style = font_style_for_section(section.section_type, section.level, fonts).clone();
    let line_height = layout.line_height * style.font_size;
    let mut y = start_y;
    if section.section_type == SectionType::Heading {
        y += match section.level.unwrap_or(1) {
            1..=3 => layout.spacing_above_heading(section.level),
            _ => 0.0,
        };
    }

    let content = sentinels_to_numbers(&apply_text_transform(&section.content, style.text_transform.as_deref()));
    let hard_lines: Vec<&str> = content.split('\n').collect();
    let align = style.text_align.clone().unwrap_or_else(|| layout.text_align.clone());

    for (hi, hard) in hard_lines.iter().enumerate() {
        let mut remaining: Vec<&str> = words(hard);
        if remaining.is_empty() {
            // Blank line: consume one line of height if there's room.
            if y + line_height <= max_y {
                y += line_height;
                continue;
            }
            return remainder(section, &hard_lines[hi..], Vec::new(), lines, y, start_y);
        }
        while !remaining.is_empty() {
            let mut placed = false;
            while y + line_height <= max_y {
                let mid = y + line_height / 2.0;
                let Some((left, right)) = horizontal_extent(points, mid) else {
                    y += line_height;
                    continue;
                };
                let avail = right - left;
                if avail < style.font_size {
                    y += line_height;
                    continue;
                }
                let (line, rest) = wrap_one_line(m, &remaining, avail, &style);
                if line.is_empty() {
                    y += line_height;
                    continue;
                }
                let width = m.text_width(&line, &style);
                let x = match align.as_str() {
                    "center" => left + (avail - width) / 2.0,
                    "right" => right - width,
                    _ => left,
                };
                lines.push(PolygonFlowLine { text: line, x, y, section_type: section.section_type, section_level: section.level });
                y += line_height;
                remaining = rest;
                placed = true;
                break;
            }
            if !placed {
                return remainder(section, &hard_lines[hi + 1..], remaining, lines, y, start_y);
            }
        }
    }
    PolygonOutcome { lines, next_y: y + layout.paragraph_spacing, remainder: None }
}

fn remainder(section: &Section, later: &[&str], current: Vec<&str>, lines: Vec<PolygonFlowLine>, y: f64, start_y: f64) -> PolygonOutcome {
    if lines.is_empty() && y == start_y {
        return PolygonOutcome { lines, next_y: y, remainder: Some(section.clone()) };
    }
    let mut parts = Vec::new();
    if !current.is_empty() {
        parts.push(current.join(" "));
    }
    parts.extend(later.iter().map(|s| s.to_string()));
    let text = parts.join("\n");
    let mut rest = section.clone();
    rest.content = text.clone();
    rest.raw_markdown = text;
    rest.measured_height = None;
    rest.lines = None;
    rest.rich_lines = None;
    rest.line_heights = None;
    PolygonOutcome { lines, next_y: y, remainder: Some(rest) }
}

fn wrap_one_line<'w>(m: &mut Measurer, words: &[&'w str], avail: f64, style: &FontStyle) -> (String, Vec<&'w str>) {
    let safe = avail * super::measure::SAFE_WIDTH_RATIO;
    let mut line = String::new();
    let mut i = 0;
    while i < words.len() {
        let candidate = if line.is_empty() { words[i].to_string() } else { format!("{line} {}", words[i]) };
        if m.text_width(&candidate, style) <= safe {
            line = candidate;
            i += 1;
        } else {
            if line.is_empty() {
                return (String::new(), words.to_vec());
            }
            break;
        }
    }
    (line, words[i..].to_vec())
}

/// Turn filled slots back into text pages and static pages whose text-flow
/// items carry their flowed content.
pub fn materialize_slots(slots: Vec<FlowSlot>, static_pages: &BTreeMap<u32, PageContent>) -> (Vec<PageContent>, BTreeMap<u32, PageContent>) {
    let mut text_pages = Vec::new();
    let mut item_sections: HashMap<(u32, String), Vec<Section>> = HashMap::new();
    let mut polygon_lines: HashMap<(u32, String), Vec<PolygonFlowLine>> = HashMap::new();

    for slot in slots {
        match slot.kind {
            SlotKind::Page { page_number } => {
                if slot.sections.is_empty() {
                    continue;
                }
                text_pages.push(PageContent {
                    id: new_id(),
                    page_number,
                    page_state: PageState::Text,
                    sections: slot.sections,
                    is_recto: page_number % 2 == 1,
                    ..Default::default()
                });
            }
            SlotKind::Item { host, item_id } => {
                item_sections.insert((host, item_id), slot.sections);
            }
            SlotKind::Polygon { host, item_id, .. } => {
                polygon_lines.insert((host, item_id), slot.polygon_lines);
            }
        }
    }

    let mut updated = BTreeMap::new();
    for (&pn, page) in static_pages {
        let mut page = page.clone();
        if let Some(items) = page.items.as_mut() {
            for item in items.iter_mut().filter(|i| i.item_type == PageItemType::TextFlow) {
                let key = (pn, item.id.clone());
                if let Some(lines) = polygon_lines.remove(&key) {
                    item.flowed_polygon_lines = Some(lines);
                } else if let Some(sections) = item_sections.remove(&key) {
                    item.flowed_sections = Some(sections);
                }
            }
        }
        updated.insert(pn, page);
    }
    (text_pages, updated)
}
