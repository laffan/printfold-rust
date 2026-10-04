//! Document structure: sections, rich text, pages, spreads and signatures.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use serde_with::skip_serializing_none;

use super::fill::FillConfig;
use super::items::{PageItem, SpanningItem};

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SectionType {
    Heading,
    #[default]
    Paragraph,
    Image,
    List,
    Code,
    Blockquote,
    Hr,
    EndnoteHeader,
    Endnote,
}

/// A run of text with uniform inline styling. Flags are only serialised when
/// set so the frontend sees `undefined` (not `false`/`null`) otherwise —
/// several renderers test `span.footnoteNumber !== undefined`.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSpan {
    pub text: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub code: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strikethrough: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub highlight: bool,
    pub link: Option<String>,
    pub footnote_number: Option<u32>,
}

impl TextSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self { text: text.into(), ..Default::default() }
    }

    /// Same inline styling (ignores text). Footnote markers never match.
    pub fn same_style(&self, other: &TextSpan) -> bool {
        if self.footnote_number.is_some() || other.footnote_number.is_some() {
            return false;
        }
        self.bold == other.bold
            && self.italic == other.italic
            && self.code == other.code
            && self.strikethrough == other.strikethrough
            && self.highlight == other.highlight
            && self.link == other.link
    }

    /// Copy of this span's style carrying different text.
    pub fn with_text(&self, text: impl Into<String>) -> Self {
        Self { text: text.into(), ..self.clone() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RichTextLine {
    pub spans: Vec<TextSpan>,
}

impl RichTextLine {
    pub fn empty() -> Self {
        Self { spans: vec![TextSpan::plain("")] }
    }

    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

/// A block of the document. Combines the TS `DocumentSection` with the
/// measured fields of `MeasuredSection` (present once laid out).
#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default)]
    pub section_type: SectionType,
    pub level: Option<u32>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub raw_markdown: String,
    pub image_ref: Option<String>,
    pub footnote_refs: Option<Vec<u32>>,
    pub endnote_number: Option<u32>,
    // Measured fields
    pub measured_height: Option<f64>,
    pub lines: Option<Vec<String>>,
    pub rich_lines: Option<Vec<RichTextLine>>,
    pub line_heights: Option<Vec<f64>>,
    /// Laid-out size of an image section's picture (absent when the image
    /// file is missing and a placeholder is drawn instead).
    pub image_width: Option<f64>,
    pub image_height: Option<f64>,
}

impl Section {
    pub fn lines(&self) -> &[String] {
        self.lines.as_deref().unwrap_or(&[])
    }

    pub fn rich_lines(&self) -> &[RichTextLine] {
        self.rich_lines.as_deref().unwrap_or(&[])
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FootnoteDefinition {
    pub id: String,
    pub number: u32,
    pub content: String,
}

/// One positioned line inside a polygon text-flow region.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolygonFlowLine {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub section_type: SectionType,
    pub section_level: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PageState {
    #[default]
    Available,
    Text,
    Static,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageContent {
    #[serde(default)]
    pub id: String,
    pub page_number: u32,
    #[serde(default)]
    pub page_state: PageState,
    #[serde(default)]
    pub sections: Vec<Section>,
    pub overflow: Option<Vec<Section>>,
    #[serde(default)]
    pub is_blank: bool,
    #[serde(default)]
    pub is_recto: bool,
    #[serde(default)]
    pub is_static: bool,
    pub is_back_cover: Option<bool>,
    pub items: Option<Vec<PageItem>>,
    pub background_fill: Option<FillConfig>,
    pub custom_background_image_id: Option<String>,
    pub footnotes: Option<Vec<FootnoteDefinition>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl PageContent {
    pub fn items(&self) -> &[PageItem] {
        self.items.as_deref().unwrap_or(&[])
    }

    pub fn has_items(&self) -> bool {
        !self.items().is_empty()
    }

    /// Page is static or available, either by state or by the legacy flags.
    pub fn is_static_or_available(&self) -> bool {
        matches!(self.page_state, PageState::Static | PageState::Available)
            || self.is_blank
            || self.is_static
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spread {
    pub id: String,
    pub spread_number: u32,
    pub verso: Option<PageContent>,
    pub recto: Option<PageContent>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    pub id: String,
    pub signature_number: u32,
    pub spreads: Vec<Spread>,
    pub page_count: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Signature {
    /// All pages of the signature in spread order.
    pub fn pages(&self) -> impl Iterator<Item = &PageContent> {
        self.spreads
            .iter()
            .flat_map(|s| s.verso.iter().chain(s.recto.iter()))
    }
}

/// Iterate every page of every signature in reading order.
pub fn all_pages(signatures: &[Signature]) -> impl Iterator<Item = &PageContent> {
    signatures.iter().flat_map(|s| s.pages())
}

/// Legacy static spread (exists independently of the markdown flow).
#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticSpread {
    pub id: String,
    #[serde(default)]
    pub index: u32,
    pub verso: Option<PageContent>,
    pub recto: Option<PageContent>,
    pub spanning_items: Option<Vec<SpanningItem>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
