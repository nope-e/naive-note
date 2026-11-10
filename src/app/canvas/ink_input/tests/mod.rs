mod cursor;
mod modeling;
mod mouse;
mod navigation;
mod objects;
mod ownership;
mod samples;
mod selected_tool;
mod tool_switch;
mod ui;

use super::{CanvasInput, CanvasView};
use crate::document::{CanvasViewport, PageGeometry, Point, Rect, Rotation, StrokeRecord};
use tablet_input_windows::{PenPhase, PenSample, PenTilt};

fn setup() -> (CanvasInput, CanvasView, Point) {
    let mut view = CanvasView::new(vec![
        PageGeometry::new(Rect::new(20.0, 30.0, 220.0, 330.0), Rotation::R0, 1.5).unwrap(),
        PageGeometry::new(Rect::new(-10.0, 40.0, 390.0, 240.0), Rotation::R90, 2.0).unwrap(),
    ]);
    let viewport =
        CanvasViewport::new(Rect::new(0.0, 40.0, 800.0, 640.0), 1.5, (1200, 960)).unwrap();
    view.update_viewport(viewport);
    let position = viewport.logical_to_physical(viewport.logical.center());
    // Existing routing/coordinate tests exercise the unchanged Direct mode.
    let input = CanvasInput {
        smoothing: false,
        ..Default::default()
    };
    (input, view, position)
}

fn sample(phase: PenPhase, position: Point) -> PenSample {
    PenSample {
        pointer_id: 4,
        stroke_id: Some(7),
        time_us: 123_456,
        position: [position.x, position.y],
        pressure: None,
        pressure_level: Some(16_384),
        tilt_deg: PenTilt {
            x: Some(-30.0),
            y: None,
        },
        rotation_deg: Some(15.0),
        eraser: false,
        inverted: false,
        barrel_button: true,
        phase,
    }
}
