//! Use the installed Windows CJK font for Unicode file names and OS errors.

use std::{path::PathBuf, sync::Arc};

pub(super) fn configure(context: &egui::Context) {
    let windows =
        std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let path = windows.join("Fonts/msyh.ttc");
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mut fonts = egui::FontDefinitions::default();
            let name = "windows-cjk".to_owned();
            fonts
                .font_data
                .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.clone());
            }
            context.set_fonts(fonts);
        }
        Err(error) => log::warn!(
            "Cannot load UI CJK fallback font ({}): {error}",
            path.display()
        ),
    }
}
