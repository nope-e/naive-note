//! Installed Windows fallback fonts for CJK text and UI symbols.

use std::{path::PathBuf, sync::Arc};

pub(super) fn configure(context: &egui::Context) {
    let windows =
        std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let mut fonts = egui::FontDefinitions::default();
    for (name, file) in [
        ("windows-cjk", "msyh.ttc"),
        // egui's built-in color picker uses U+1F4CB (clipboard) for its copy button.
        ("windows-symbols", "seguisym.ttf"),
    ] {
        let path = windows.join("Fonts").join(file);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let name = name.to_owned();
                fonts
                    .font_data
                    .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts.families.entry(family).or_default().push(name.clone());
                }
            }
            Err(error) => log::warn!(
                "Cannot load UI fallback font ({}): {error}",
                path.display()
            ),
        }
    }
    context.set_fonts(fonts);
}
