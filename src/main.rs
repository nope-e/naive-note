#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod cursor;
mod document;
mod ink;
mod pdf;
mod platform;
mod render;
mod storage;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // Windows "Open with" passes the selected file as the first argument.
    // Keep the native path intact, including Unicode and spaces.
    let startup_pdf = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    app::run(startup_pdf)
}
