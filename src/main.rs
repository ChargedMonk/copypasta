#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod clipboard;
mod history;
mod hotkeys;
mod paste;
mod settings;
mod state;
mod tray;
mod ui;
mod win;

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,copypasta=info".into()),
        )
        .init();

    app::run().context("app run failed")?;
    Ok(())
}
