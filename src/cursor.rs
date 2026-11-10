//! Persistent canvas cursor choices. Images are native OS cursors, independent of zoom.

mod image;

use serde::{Deserialize, Serialize};

pub(crate) use image::CursorImages;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CanvasCursor {
    Icon(egui::CursorIcon),
    Style(CursorStyle),
    Eraser,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorShape {
    System,
    #[default]
    Dot,
    Crosshair,
    Beam,
    TeardropNorthWest,
    TeardropNorth,
    TeardropNorthEast,
}

impl CursorShape {
    pub const ALL: [(Self, &'static str); 7] = [
        (Self::System, "系统箭头"),
        (Self::Dot, "红色圆点"),
        (Self::Crosshair, "十字"),
        (Self::Beam, "光标线"),
        (Self::TeardropNorthWest, "水滴 · 左上"),
        (Self::TeardropNorth, "水滴 · 向上"),
        (Self::TeardropNorthEast, "水滴 · 右上"),
    ];

    pub fn label(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(shape, _)| *shape == self)
            .unwrap()
            .1
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl CursorSize {
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Small, "小"),
        (Self::Medium, "中"),
        (Self::Large, "大"),
    ];

    fn radius(self) -> f64 {
        match self {
            Self::Small => 2.0,
            Self::Medium => 3.5,
            Self::Large => 5.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorStyle {
    pub shape: CursorShape,
    pub size: CursorSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CursorSettings {
    pub regular: CursorStyle,
    pub drawing: CursorStyle,
    pub show_drawing: bool,
}

impl Default for CursorSettings {
    fn default() -> Self {
        Self {
            regular: CursorStyle::default(),
            drawing: CursorStyle {
                shape: CursorShape::Dot,
                size: CursorSize::Small,
            },
            show_drawing: true,
        }
    }
}
