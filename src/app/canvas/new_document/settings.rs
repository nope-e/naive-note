//! Physical page settings; DPI converts pixel dimensions without changing PDF scale.

use anyhow::{Result, ensure};

use crate::document::{PageGeometry, Rect, Rotation};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Unit {
    #[default]
    Millimeters,
    Inches,
    Pixels,
}

impl Unit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Millimeters => "毫米",
            Self::Inches => "英寸",
            Self::Pixels => "像素",
        }
    }

    fn millimeters_per_unit(self, dpi: u32) -> f64 {
        match self {
            Self::Millimeters => 1.0,
            Self::Inches => 25.4,
            Self::Pixels => 25.4 / f64::from(dpi),
        }
    }
}

pub(super) struct Preset {
    pub name: &'static str,
    pub width_mm: f64,
    pub height_mm: f64,
}

pub(super) const PRESETS: [Preset; 9] = [
    Preset {
        name: "A3",
        width_mm: 297.0,
        height_mm: 420.0,
    },
    Preset {
        name: "A4",
        width_mm: 210.0,
        height_mm: 297.0,
    },
    Preset {
        name: "A5",
        width_mm: 148.0,
        height_mm: 210.0,
    },
    Preset {
        name: "A6",
        width_mm: 105.0,
        height_mm: 148.0,
    },
    Preset {
        name: "B4",
        width_mm: 250.0,
        height_mm: 353.0,
    },
    Preset {
        name: "B5",
        width_mm: 176.0,
        height_mm: 250.0,
    },
    Preset {
        name: "Letter",
        width_mm: 215.9,
        height_mm: 279.4,
    },
    Preset {
        name: "Legal",
        width_mm: 215.9,
        height_mm: 355.6,
    },
    Preset {
        name: "Tabloid",
        width_mm: 279.4,
        height_mm: 431.8,
    },
];

pub(super) struct PageSettings {
    pub width: f64,
    pub height: f64,
    pub unit: Unit,
    pub dpi: u32,
    pub selected: Option<usize>,
}

impl Default for PageSettings {
    fn default() -> Self {
        Self {
            width: 210.0,
            height: 297.0,
            unit: Unit::Millimeters,
            dpi: 300,
            selected: Some(1),
        }
    }
}

impl PageSettings {
    pub fn dimensions_mm(&self) -> (f64, f64) {
        let scale = self.unit.millimeters_per_unit(self.dpi);
        (self.width * scale, self.height * scale)
    }

    pub fn dimensions_px(&self) -> (f64, f64) {
        let (width, height) = self.dimensions_mm();
        let scale = f64::from(self.dpi) / 25.4;
        (width * scale, height * scale)
    }

    pub fn set_unit(&mut self, unit: Unit) {
        let (width, height) = self.dimensions_mm();
        let scale = unit.millimeters_per_unit(self.dpi);
        self.width = width / scale;
        self.height = height / scale;
        self.unit = unit;
    }

    pub fn select(&mut self, index: usize) {
        let landscape = self.width > self.height;
        let preset = &PRESETS[index];
        let scale = self.unit.millimeters_per_unit(self.dpi);
        let (width, height) = if landscape {
            (preset.height_mm, preset.width_mm)
        } else {
            (preset.width_mm, preset.height_mm)
        };
        self.width = width / scale;
        self.height = height / scale;
        self.selected = Some(index);
    }

    pub fn geometry(&self) -> Result<PageGeometry> {
        ensure!((36..=2400).contains(&self.dpi), "DPI 应在 36–2400 之间");
        let (width, height) = self.dimensions_mm();
        ensure!(
            width.is_finite()
                && height.is_finite()
                && (1.0..=5000.0).contains(&width)
                && (1.0..=5000.0).contains(&height),
            "纸张宽高应在 1–5000 毫米之间"
        );
        PageGeometry::new(
            Rect::new(0.0, 0.0, width * 72.0 / 25.4, height * 72.0 / 25.4),
            Rotation::R0,
            1.0,
        )
    }
}

#[cfg(test)]
mod tests;
