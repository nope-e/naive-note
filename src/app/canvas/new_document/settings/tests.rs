//! Unit conversion and page geometry only; no UI, layout or window.

use super::*;

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

#[test]
fn presets_make_physical_pages_with_a_zero_origin() {
    let mut settings = PageSettings::default();
    for (index, preset) in PRESETS.iter().enumerate() {
        settings.select(index);
        let page = settings.geometry().unwrap();
        close(page.paper_size().width, preset.width_mm * 72.0 / 25.4);
        close(page.paper_size().height, preset.height_mm * 72.0 / 25.4);
        assert_eq!(page.crop_box().min, crate::document::Point::new(0.0, 0.0));
    }
}

#[test]
fn unit_switches_preserve_the_physical_page_and_dpi_only_scales_pixel_readouts() {
    let mut settings = PageSettings::default();
    let original = settings.geometry().unwrap().paper_size();
    let pixels = settings.dimensions_px();
    settings.dpi = 150;
    assert_eq!(settings.geometry().unwrap().paper_size(), original);
    close(settings.dimensions_px().0, pixels.0 / 2.0);
    for unit in [Unit::Inches, Unit::Pixels, Unit::Millimeters] {
        settings.set_unit(unit);
        let actual = settings.geometry().unwrap().paper_size();
        close(actual.width, original.width);
        close(actual.height, original.height);
    }
}

#[test]
fn pixel_dimensions_and_dpi_determine_physical_size_and_landscape_presets_keep_orientation() {
    let mut settings = PageSettings {
        width: 2400.0,
        height: 1200.0,
        unit: Unit::Pixels,
        dpi: 300,
        selected: None,
    };
    close(settings.geometry().unwrap().paper_size().width, 8.0 * 72.0);
    settings.dpi = 150;
    close(settings.geometry().unwrap().paper_size().width, 16.0 * 72.0);
    settings.select(0);
    let (width, height) = settings.dimensions_mm();
    close(width, 420.0);
    close(height, 297.0);
}

#[test]
fn invalid_sizes_cannot_be_created() {
    for width in [0.0, -1.0, 0.99, 5000.01, f64::NAN, f64::INFINITY] {
        assert!(
            PageSettings {
                width,
                ..PageSettings::default()
            }
            .geometry()
            .is_err()
        );
    }
    for dpi in [0, 35, 2401, u32::MAX] {
        assert!(
            PageSettings {
                dpi,
                ..PageSettings::default()
            }
            .geometry()
            .is_err()
        );
    }
}
