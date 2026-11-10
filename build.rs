use std::{env, fs, io, path::PathBuf};

use resvg::{tiny_skia, usvg};

const ICON_SVG: &str = "assets/icons/app.svg";
const ICON_SIZES: &[u32] = &[16, 20, 24, 32, 40, 48, 64, 96, 128, 256];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed={ICON_SVG}");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }

    let svg = fs::read(ICON_SVG)?;
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default())?;
    let mut frames = Vec::with_capacity(ICON_SIZES.len());
    for &size in ICON_SIZES {
        let mut pixmap = tiny_skia::Pixmap::new(size, size)
            .ok_or_else(|| io::Error::other("allocate application icon"))?;
        let scale = size as f32 / tree.size().width();
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        frames.push(pixmap.encode_png()?);
    }

    // ICO header, directory, then PNG frames. A zero dimension denotes 256 px.
    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(ICON_SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + ICON_SIZES.len() as u32 * 16;
    for (&size, frame) in ICON_SIZES.iter().zip(&frames) {
        ico.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += frame.len() as u32;
    }
    for frame in frames {
        ico.extend_from_slice(&frame);
    }

    let icon_path = PathBuf::from(env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?).join("app.ico");
    fs::write(&icon_path, ico)?;
    winresource::WindowsResource::new()
        .set_icon_with_id(icon_path.to_str().ok_or("non-Unicode icon path")?, "1")
        .set("ProductName", "Naive Note")
        .set("FileDescription", "Naive Note")
        .set("OriginalFilename", "naive-note.exe")
        .compile()?;
    Ok(())
}
