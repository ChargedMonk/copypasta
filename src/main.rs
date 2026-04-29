#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    enable_dpi_awareness();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,copypasta=info".into()),
        )
        .init();

    copypasta::app::run().context("app run failed")?;
    Ok(())
}

#[cfg(windows)]
fn enable_dpi_awareness() {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
    };

    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err() {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        }
    }
}

#[cfg(not(windows))]
fn enable_dpi_awareness() {}
