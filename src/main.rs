#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod document;
mod ink;
mod pdf;
mod platform;
mod render;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if pdf::probe::run_cli()? {
        return Ok(());
    }
    app::run()
}
