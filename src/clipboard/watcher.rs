use windows::Win32::Foundation::HWND;
use windows::Win32::System::DataExchange::AddClipboardFormatListener;

pub fn register(hwnd: HWND) -> anyhow::Result<()> {
    unsafe { AddClipboardFormatListener(hwnd)? };
    tracing::info!("clipboard listener registered");
    Ok(())
}

pub fn on_clipboard_update(hwnd: HWND) {
    let state_ptr = unsafe { crate::win::window::get_userdata(hwnd) };
    if state_ptr.is_null() {
        return;
    }
    let state = unsafe { &*(state_ptr as *const crate::state::AppState) };

    let now = unix_ms_now();
    if *state.capture_paused.lock() {
        return;
    }
    if now < *state.suppress_clipboard_until_unix_ms.lock() {
        return;
    }

    match crate::clipboard::formats::capture_clipboard() {
        Ok(Some(captured)) => {
            let mut store = state.store.lock();
            match store.persist_captured(
                now,
                captured.fingerprint,
                &captured.preview,
                &captured.formats,
            ) {
                Ok(item) => {
                    let mut history = state.history.lock();
                    history.add_or_bump_full(item);
                    tracing::info!(history_len = history.len(), "captured clipboard");
                }
                Err(err) => tracing::warn!(error = ?err, "persist failed"),
            }
        }
        Ok(None) => {}
        Err(err) => tracing::warn!(error = ?err, "failed to capture clipboard"),
    }
}

fn unix_ms_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_millis() as i64
}
