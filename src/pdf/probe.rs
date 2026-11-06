//! Explicit headless commands for stage 3. Never starts an event loop or renderer.

use std::{ffi::OsStr, fs, path::PathBuf};

use anyhow::{Context, Result, ensure};
use mupdf::pdf::PdfDocument;
use uuid::Uuid;

use super::{backend::page_geometry, save};

pub fn output_directory() -> Result<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".scratch/pdf-save-probe")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&path).context("Create project-local PDF probe directory")?;
    Ok(path)
}

pub fn run_cli() -> Result<bool> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let Some(command) = args.first() else {
        return Ok(false);
    };
    if command != OsStr::new("--pdf-save-probe") && command != OsStr::new("--pdf-inspect") {
        return Ok(false);
    }
    let inspect = command == OsStr::new("--pdf-inspect");
    ensure!(
        args.len() <= 2 && (!inspect || args.len() == 2),
        "Usage: --pdf-save-probe [source.pdf] | --pdf-inspect file.pdf"
    );
    let source = args.get(1).map(PathBuf::from);
    let report = std::thread::Builder::new().name("mupdf-probe".to_owned()).spawn(move || -> Result<String> {
        if inspect {
            let path = source.context("Missing inspect path")?;
            let document = PdfDocument::from_bytes(&save::read_pdf(&path)?)?;
            let data = save::read_edit_data(&document)?.context("No Naive Note edit data")?;
            return Ok(format!("Verified {} | {} page(s) | document {} | edit data v{} | {} stroke(s)", path.display(), document.page_count()?, data.document_id, data.format_version, data.strokes.len()));
        }
        let directory = output_directory()?;
        let bytes = if let Some(source) = source {
            save::read_pdf(&source)?
        } else {
            let mut document = PdfDocument::new();
            document.new_page_at(0, mupdf::Size::new(595.0,842.0))?;
            let mut bytes = Vec::new();
            document.write_to(&mut bytes)?;
            fs::write(directory.join("source.pdf"), &bytes)?;
            bytes
        };
        let document = PdfDocument::from_bytes(&bytes)?;
        let geometry = page_geometry(&document.load_pdf_page(0)?)?;
        let stroke = crate::ink::demo_stroke(geometry)?;
        let report = save::save_probe(&bytes, &stroke, &directory.join("pressure.pdf"))?;
        fs::write(directory.join("verification.txt"), format!("{report}\nStroke UUID: {}\nVisual / Edge / Acrobat / printing: pending user verification\n", stroke.id))?;
        Ok(report)
    })?.join().map_err(|_| anyhow::anyhow!("PDF probe worker panicked"))??;
    println!("{report}");
    Ok(true)
}
