#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
    app::run()
}
