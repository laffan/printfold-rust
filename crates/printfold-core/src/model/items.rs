//! Page items: text boxes, shapes, images and text-flow regions.
//!
//! The TypeScript side models these as a discriminated union. Rust keeps a
//! single flat struct (every type-specific field optional) because the engine
//! mostly passes items through untouched; the few places that care branch on
//! [`PageItem::item_type`]. Unknown keys are preserved in `extra`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use serde_with::skip_serializing_none;

use super::document::{PolygonFlowLine, Section};
use super::fill::FillConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PageItemType {
    #[default]
    Text,
    Shape,
    Image,
    TextFlow,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrayDimension {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub count: f64,
    #[serde(default)]
    pub offset_x: f64,
    #[serde(default)]
    pub offset_y: f64,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

/// A text-flow polygon vertex in item-normalised coordinates (0..1).
#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolygonPoint {
    pub x: f64,
    pub y: f64,
    /// `corner` (default) or `smooth`.
    pub corner_type: Option<String>,
    pub handle_in: Option<Vec2>,
    pub handle_out: Option<Vec2>,
}

impl PolygonPoint {
    pub fn is_smooth(&self) -> bool {
        self.corner_type.as_deref() == Some("smooth")
    }
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageItem {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default)]
    pub item_type: PageItemType,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
    pub rotation: Option<f64>,
    pub opacity: Option<f64>,
    pub z_index: Option<f64>,

    // Shadow
    pub has_shadow: Option<bool>,
    pub shadow_color: Option<String>,
    pub shadow_blur: Option<f64>,
    pub shadow_offset_x: Option<f64>,
    pub shadow_offset_y: Option<f64>,
    pub shadow_opacity: Option<f64>,

    pub stroke_offset: Option<f64>,
    pub fill_offset: Option<f64>,
    pub array_dimensions: Option<Vec<ArrayDimension>>,

    // Text items
    pub content: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub font_weight: Option<String>,
    pub font_style: Option<String>,
    pub color: Option<String>,
    pub text_align: Option<String>,
    pub text_transform: Option<String>,

    // Shared fill/stroke
    pub fill: Option<FillConfig>,
    pub has_fill: Option<bool>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<f64>,
    pub has_stroke: Option<bool>,

    // Shapes
    pub shape_type: Option<String>,
    pub fill_color: Option<String>,

    // Images
    pub image_file_id: Option<String>,

    // Text-flow regions
    pub flow_shape: Option<String>,
    pub padding: Option<f64>,
    pub text_color: Option<String>,
    pub polygon_points: Option<Vec<PolygonPoint>>,
    pub flowed_sections: Option<Vec<Section>>,
    pub flowed_polygon_lines: Option<Vec<PolygonFlowLine>>,

    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl PageItem {
    pub fn is_text_flow(&self) -> bool {
        self.item_type == PageItemType::TextFlow
    }

    pub fn is_polygon_flow(&self) -> bool {
        self.is_text_flow()
            && self.flow_shape.as_deref() == Some("polygon")
            && self.polygon_points.as_ref().map(|p| p.len() >= 3).unwrap_or(false)
    }

    pub fn opacity_or_default(&self) -> f64 {
        self.opacity.unwrap_or(1.0)
    }
}

/// Item spanning a whole spread (legacy static spreads). Coordinates are
/// relative to the verso's left edge, so x runs from 0 to 2 * pageWidth.
pub type SpanningItem = PageItem;
