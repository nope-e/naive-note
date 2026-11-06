//! Read-only page, zoom and pointer readouts.

use crate::document::{PageGeometry, Point};

pub(super) fn show(
    ui: &mut egui::Ui,
    page: PageGeometry,
    zoom: f64,
    fit_to_page: bool,
    pointer: Option<(Point, Point)>,
    document_label: &str,
    message: Option<&str>,
) {
    egui::Panel::bottom("status")
        .resizable(false)
        .exact_size(68.0)
        .show(ui, |ui| {
            // Keep the canvas size stable as pointer-coordinate text changes.
            let size = page.paper_size();
            ui.add(
                egui::Label::new(format!(
                    "{document_label} | {:.0} x {:.0} pt | {:.0}%{}",
                    size.width,
                    size.height,
                    zoom * 100.0,
                    if fit_to_page { " (fit)" } else { "" },
                ))
                .truncate(),
            );
            let coordinates = if let Some((paper, pdf)) = pointer {
                format!(
                    "Paper ({:.1}, {:.1}) pt | PDF ({:.1}, {:.1})",
                    paper.x, paper.y, pdf.x, pdf.y
                )
            } else {
                "Pointer outside paper".to_owned()
            };
            ui.add(egui::Label::new(egui::RichText::new(coordinates).monospace()).truncate());
            let message = message.unwrap_or("Open a PDF with Ctrl+O or drop a file here");
            ui.add(egui::Label::new(message).truncate())
                .on_hover_text(message);
        });
}
