//! Output, layout, typography and header/footer options, with the same
//! defaults as the TypeScript `state/defaults.ts`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use serde_with::skip_serializing_none;

#[skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputOptions {
    /// letter | a4 | legal | tabloid | a3
    pub sheet_size: String,
    /// booklet | doubleSided | singleSided
    pub booklet_type: Option<String>,
    /// half | quarter | eighth | sixteenth | custom
    pub booklet_size: String,
    pub custom_width: Option<f64>,
    pub custom_height: Option<f64>,
    pub pages_per_signature: u32,
    /// portrait | landscape
    pub orientation: String,
    /// autofill | center | upperLeft
    pub placement: Option<String>,
    pub fill_available_space: bool,
    pub show_fold_marks: bool,
    pub creep_enabled: Option<bool>,
    pub creep_per_sheet: Option<f64>,
    pub render_text_as_images: Option<bool>,
    pub duplex_offset_x: Option<f64>,
    pub duplex_offset_y: Option<f64>,
    pub show_crop_marks: Option<bool>,
    pub crop_mark_color: Option<String>,
    pub crop_mark_thickness: Option<f64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            sheet_size: "letter".into(),
            booklet_type: Some("booklet".into()),
            booklet_size: "quarter".into(),
            custom_width: None,
            custom_height: None,
            pages_per_signature: 4,
            orientation: "portrait".into(),
            placement: Some("autofill".into()),
            fill_available_space: true,
            show_fold_marks: false,
            creep_enabled: Some(false),
            creep_per_sheet: Some(0.0625 * 72.0),
            render_text_as_images: None,
            duplex_offset_x: Some(0.0),
            duplex_offset_y: Some(0.0),
            show_crop_marks: Some(true),
            crop_mark_color: Some("#000000".into()),
            crop_mark_thickness: Some(0.5),
            extra: Map::new(),
        }
    }
}

impl OutputOptions {
    pub fn booklet_type(&self) -> &str {
        self.booklet_type.as_deref().unwrap_or("booklet")
    }

    pub fn is_booklet(&self) -> bool {
        self.booklet_type() == "booklet"
    }

    pub fn placement(&self) -> &str {
        self.placement.as_deref().unwrap_or("autofill")
    }

    pub fn pages_per_signature(&self) -> usize {
        self.pages_per_signature.max(4) as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Margins {
    pub top: f64,
    pub bottom: f64,
    pub inner: f64,
    pub outer: f64,
}

impl Default for Margins {
    fn default() -> Self {
        Self { top: 54.0, bottom: 54.0, inner: 54.0, outer: 36.0 }
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialMargins {
    pub top: Option<f64>,
    pub bottom: Option<f64>,
    pub inner: Option<f64>,
    pub outer: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMarginOverride {
    pub page_number: u32,
    #[serde(default)]
    pub margins: PartialMargins,
}

#[skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LayoutOptions {
    pub margins: Margins,
    pub margin_overrides: Vec<PageMarginOverride>,
    #[serde(rename = "emptyPageBeforeH1")]
    pub empty_page_before_h1: bool,
    #[serde(rename = "spacingAboveH1")]
    pub spacing_above_h1: f64,
    #[serde(rename = "spacingAboveH2")]
    pub spacing_above_h2: f64,
    #[serde(rename = "spacingAboveH3")]
    pub spacing_above_h3: f64,
    pub paragraph_spacing: f64,
    pub line_height: f64,
    /// left | justify
    pub text_align: String,
    pub show_footnotes_as_endnotes: Option<bool>,
    /// document | chapter
    pub endnote_placement: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            margins: Margins::default(),
            margin_overrides: Vec::new(),
            empty_page_before_h1: true,
            spacing_above_h1: 72.0,
            spacing_above_h2: 36.0,
            spacing_above_h3: 24.0,
            paragraph_spacing: 12.0,
            line_height: 1.5,
            text_align: "left".into(),
            show_footnotes_as_endnotes: Some(false),
            endnote_placement: Some("document".into()),
            extra: Map::new(),
        }
    }
}

impl LayoutOptions {
    /// Extra space above a heading of the given level.
    pub fn spacing_above_heading(&self, level: Option<u32>) -> f64 {
        match level.unwrap_or(1) {
            1 => self.spacing_above_h1,
            2 => self.spacing_above_h2,
            3 => self.spacing_above_h3,
            _ => 0.0,
        }
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FontStyle {
    pub font_family: String,
    pub font_size: f64,
    pub font_weight: String,
    pub font_style: String,
    pub color: String,
    pub text_transform: Option<String>,
    pub letter_spacing: Option<f64>,
    pub line_height: Option<f64>,
    pub text_align: Option<String>,
    pub background_color: Option<String>,
    pub text_decoration: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl FontStyle {
    pub fn new(family: &str, size: f64, weight: &str, style: &str) -> Self {
        Self {
            font_family: family.into(),
            font_size: size,
            font_weight: weight.into(),
            font_style: style.into(),
            color: "#000000".into(),
            text_transform: None,
            letter_spacing: None,
            line_height: None,
            text_align: None,
            background_color: None,
            text_decoration: None,
            extra: Map::new(),
        }
    }

    pub fn is_bold(&self) -> bool {
        self.font_weight == "bold"
    }

    pub fn is_italic(&self) -> bool {
        self.font_style == "italic"
    }
}

impl Default for FontStyle {
    fn default() -> Self {
        Self::new("Georgia", 12.0, "normal", "normal")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightStyle {
    pub text_color: String,
    pub background_color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrikethroughStyle {
    pub text_color: String,
    pub line_color: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FontOptions {
    pub body: FontStyle,
    pub h1: FontStyle,
    pub h2: FontStyle,
    pub h3: FontStyle,
    pub h4: FontStyle,
    pub h5: FontStyle,
    pub h6: FontStyle,
    pub code: FontStyle,
    pub blockquote: FontStyle,
    pub footnote: FontStyle,
    pub footnote_number_color: Option<String>,
    pub footnote_gap: Option<f64>,
    pub highlight: Option<HighlightStyle>,
    pub strikethrough: Option<StrikethroughStyle>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for FontOptions {
    fn default() -> Self {
        let body = FontStyle::default();
        let heading = |size: f64, weight: &str| FontStyle::new("Arial", size, weight, "normal");
        let mut code = body.clone();
        code.font_family = "Courier New".into();
        code.font_size = 10.0;
        let mut blockquote = body.clone();
        blockquote.font_style = "italic".into();
        blockquote.color = "#555555".into();
        let mut footnote = body.clone();
        footnote.font_size = 9.0;
        footnote.line_height = Some(1.3);
        Self {
            body,
            h1: heading(24.0, "bold"),
            h2: heading(20.0, "bold"),
            h3: heading(16.0, "bold"),
            h4: heading(14.0, "bold"),
            h5: heading(12.0, "bold"),
            h6: heading(12.0, "normal"),
            code,
            blockquote,
            footnote,
            footnote_number_color: None,
            footnote_gap: None,
            highlight: Some(HighlightStyle {
                text_color: "#000000".into(),
                background_color: "#ffff00".into(),
            }),
            strikethrough: Some(StrikethroughStyle {
                text_color: "#888888".into(),
                line_color: "#888888".into(),
            }),
            extra: Map::new(),
        }
    }
}

impl FontOptions {
    /// Heading style for levels 1–6 (anything else clamps).
    pub fn heading(&self, level: u32) -> &FontStyle {
        match level {
            0 | 1 => &self.h1,
            2 => &self.h2,
            3 => &self.h3,
            4 => &self.h4,
            5 => &self.h5,
            _ => &self.h6,
        }
    }

    /// Every family referenced by the typography settings.
    pub fn families(&self) -> Vec<&str> {
        [
            &self.body, &self.h1, &self.h2, &self.h3, &self.h4, &self.h5, &self.h6, &self.code,
            &self.blockquote, &self.footnote,
        ]
        .iter()
        .map(|s| s.font_family.as_str())
        .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeaderFooterContent {
    pub left: String,
    pub center: String,
    pub right: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HeaderFooterSection {
    pub enabled: bool,
    pub height: f64,
    pub verso: HeaderFooterContent,
    pub recto: HeaderFooterContent,
    pub show_on_first_page: bool,
    pub font: FontStyle,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for HeaderFooterSection {
    fn default() -> Self {
        let font = FontStyle { font_size: 10.0, ..FontStyle::default() };
        Self {
            enabled: false,
            height: 24.0,
            verso: HeaderFooterContent::default(),
            recto: HeaderFooterContent::default(),
            show_on_first_page: false,
            font,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeaderFooterOptions {
    pub header: HeaderFooterSection,
    pub footer: HeaderFooterSection,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for HeaderFooterOptions {
    fn default() -> Self {
        let footer = HeaderFooterSection {
            enabled: true,
            verso: HeaderFooterContent { left: "{{pageNumber}}".into(), ..Default::default() },
            recto: HeaderFooterContent { right: "{{pageNumber}}".into(), ..Default::default() },
            ..Default::default()
        };
        Self { header: HeaderFooterSection::default(), footer, extra: Map::new() }
    }
}
