//! Fill configuration (solid colours, gradients, image patterns).
//!
//! Mirrors `FillConfig` and friends from the TypeScript side. Every struct
//! keeps unknown keys in `extra` so values written by a newer frontend
//! survive a round trip through Rust untouched.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use serde_with::skip_serializing_none;

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStop {
    pub offset: f64,
    pub color: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearGradientConfig {
    #[serde(default)]
    pub angle: f64,
    #[serde(default)]
    pub stops: Vec<GradientStop>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadialGradientConfig {
    #[serde(default)]
    pub center_x: f64,
    #[serde(default)]
    pub center_y: f64,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub stops: Vec<GradientStop>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternConfig {
    #[serde(default)]
    pub image_file_id: String,
    pub repeat: Option<String>,
    pub scale: Option<f64>,
    pub offset_x: Option<f64>,
    pub offset_y: Option<f64>,
    pub rotation: Option<f64>,
}

/// `type` is one of `color`, `linearGradient`, `radialGradient`, `pattern`.
#[skip_serializing_none]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillConfig {
    #[serde(rename = "type")]
    pub fill_type: String,
    pub color: Option<String>,
    pub linear_gradient: Option<LinearGradientConfig>,
    pub radial_gradient: Option<RadialGradientConfig>,
    pub pattern: Option<PatternConfig>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl FillConfig {
    pub fn solid(color: impl Into<String>) -> Self {
        Self { fill_type: "color".into(), color: Some(color.into()), ..Default::default() }
    }
}
