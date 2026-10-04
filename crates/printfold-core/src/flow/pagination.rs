//! Page breaking for plain text pages (port of `textFlow/pagination.ts`).

use super::measure::{font_style_for_section, Measurer};
use crate::model::{
    new_id, FontOptions, LayoutOptions, PageContent, PageDimensions, PageState, Section, SectionType,
};

/// Create an empty page. Non-blank pages are text pages awaiting content.
pub fn create_empty_page(page_number: u32, is_blank: bool, is_static: bool) -> PageContent {
    let page_state = if is_static {
        PageState::Static
    } else if is_blank {
        PageState::Available
    } else {
        PageState::Text
    };
    PageContent {
        id: new_id(),
        page_number,
        page_state,
        is_blank,
        is_recto: page_number % 2 == 1,
        is_static,
        ..Default::default()
    }
}

/// An available page with an (empty) items list, as the TS padding code made.
pub fn available_page(page_number: u32) -> PageContent {
    let mut p = create_empty_page(page_number, true, false);
    p.items = Some(Vec::new());
    p
}

/// Slice `lines[start..end]` of a measured section. Only the final slice
/// carries the paragraph spacing.
pub fn partial_section(source: &Section, start: usize, end: usize, line_height: f64, is_last: bool, layout: &LayoutOptions) -> Section {
    let lines: Vec<String> = source.lines()[start.min(source.lines().len())..end.min(source.lines().len())].to_vec();
    let rich = source.rich_lines.as_ref().map(|r| r[start.min(r.len())..end.min(r.len())].to_vec());
    let count = end - start;
    let mut part = source.clone();
    part.line_heights = Some(vec![line_height; lines.len()]);
    part.lines = Some(lines);
    part.rich_lines = rich;
    part.measured_height = Some(count as f64 * line_height + if is_last { layout.paragraph_spacing } else { 0.0 });
    part
}

/// Number of lines a measured section splits by (rich lines win).
pub fn line_count(section: &Section) -> usize {
    let rich = section.rich_lines();
    if !rich.is_empty() {
        rich.len()
    } else {
        section.lines().len()
    }
}

struct PageBuilder {
    pages: Vec<PageContent>,
    current: PageContent,
    height: f64,
}

impl PageBuilder {
    fn finish_page(&mut self) {
        let next = create_empty_page(self.pages.len() as u32 + 2, false, false);
        self.pages.push(std::mem::replace(&mut self.current, next));
        self.height = 0.0;
        self.current.page_number = self.pages.len() as u32 + 1;
        self.current.is_recto = self.current.page_number % 2 == 1;
    }
}

/// Flow sections across text pages. `reserved(page_index)` subtracts space
/// (on-page footnotes) from a page's content height.
pub fn flow_sections(
    m: &mut Measurer,
    sections: &[Section],
    dims: &PageDimensions,
    fonts: &FontOptions,
    layout: &LayoutOptions,
    reserved: &dyn Fn(usize) -> f64,
) -> Vec<PageContent> {
    let mut b = PageBuilder { pages: Vec::new(), current: create_empty_page(1, false, false), height: 0.0 };
    let available = |b: &PageBuilder| dims.content_height - reserved(b.pages.len());

    for section in sections {
        if section.section_type == SectionType::Hr {
            if !b.current.sections.is_empty() {
                b.finish_page();
            }
            continue;
        }

        let is_h1 = section.section_type == SectionType::Heading && section.level == Some(1);
        if is_h1 && layout.empty_page_before_h1 && !b.current.sections.is_empty() {
            b.finish_page();
            // H1 starts on a recto: insert a blank verso if needed.
            if b.pages.len() % 2 == 1 {
                let n = b.pages.len() as u32 + 1;
                b.pages.push(create_empty_page(n, true, false));
                b.current.page_number = b.pages.len() as u32 + 1;
                b.current.is_recto = b.current.page_number % 2 == 1;
            }
        }

        let measured = m.measure_section(section, dims.content_width, fonts, layout);
        let mh = measured.measured_height.unwrap_or(0.0);

        if b.height + mh <= available(&b) {
            b.current.sections.push(measured);
            b.height += mh;
            continue;
        }

        let style = font_style_for_section(section.section_type, section.level, fonts);
        let line_height = layout.line_height * style.font_size;
        let total = line_count(&measured);
        let splittable = matches!(section.section_type, SectionType::Paragraph | SectionType::Blockquote)
            && measured.lines().len() > 1;

        if splittable {
            let mut cursor = 0;
            let fit = ((available(&b) - b.height) / line_height).floor();
            if fit >= 2.0 && cursor < total {
                let end = (cursor + fit as usize).min(total);
                let mut first = partial_section(&measured, cursor, end, line_height, false, layout);
                // The original gave the first slice no paragraph spacing.
                first.measured_height = Some((end - cursor) as f64 * line_height);
                b.current.sections.push(first);
                cursor = end;
                b.finish_page();
            } else if !b.current.sections.is_empty() {
                b.finish_page();
            }
            let per_page = ((available(&b) / line_height).floor().max(1.0)) as usize;
            while cursor < total {
                let take = (total - cursor).min(per_page);
                let end = cursor + take;
                let last = end >= total;
                let part = partial_section(&measured, cursor, end, line_height, last, layout);
                b.height = part.measured_height.unwrap_or(0.0);
                b.current.sections.push(part);
                cursor = end;
                if cursor < total {
                    b.finish_page();
                }
            }
            continue;
        }

        // Move the whole section to a fresh page.
        if !b.current.sections.is_empty() {
            b.finish_page();
        }
        if mh <= available(&b) {
            b.current.sections.push(measured);
            b.height = mh;
            continue;
        }

        // Taller than a page: force-break by lines.
        let per_page = ((available(&b) / line_height).floor().max(1.0)) as usize;
        let mut cursor = 0;
        if total == 0 {
            b.current.sections.push(measured);
            b.height = mh;
            continue;
        }
        while cursor < total {
            let take = (total - cursor).min(per_page);
            let end = cursor + take;
            let last = end >= total;
            let part = partial_section(&measured, cursor, end, line_height, last, layout);
            b.height = part.measured_height.unwrap_or(0.0);
            b.current.sections.push(part);
            cursor = end;
            if cursor < total {
                b.finish_page();
            }
        }
    }

    if !b.current.sections.is_empty() {
        let current = std::mem::take(&mut b.current);
        b.pages.push(current);
    }
    b.pages
}

/// Insert user-requested blank pages before the given (pre-insertion) page
/// numbers and renumber.
pub fn insert_blank_pages(pages: Vec<PageContent>, blank_pages: &[u32]) -> Vec<PageContent> {
    let mut result: Vec<PageContent> = Vec::with_capacity(pages.len() + blank_pages.len());
    for (i, mut page) in pages.into_iter().enumerate() {
        let original = i as u32 + 1;
        for &blank in blank_pages {
            if blank == original {
                let n = result.len() as u32 + 1;
                result.push(create_empty_page(n, true, false));
            }
        }
        page.page_number = result.len() as u32 + 1;
        page.is_recto = page.page_number % 2 == 1;
        result.push(page);
    }
    result
}
