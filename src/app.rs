use crate::win::window::HiddenWindow;
use anyhow::Context;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

pub fn run() -> anyhow::Result<()> {
    let state = Box::new(crate::state::AppState::new()?);
    let state_ptr = Box::into_raw(state);

    let window = HiddenWindow::create(state_ptr.cast()).context("create hidden window")?;
    let hotkeys = crate::hotkeys::register(window.hwnd());
    tracing::info!(
        registered_hotkeys = hotkeys.registered.len(),
        failed_hotkeys = hotkeys.failed.len(),
        "hotkey registration completed"
    );
    crate::clipboard::watcher::register(window.hwnd()).context("register clipboard listener")?;

    // Standard Win32 message loop.
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // Window has exited; reclaim state pointer.
    unsafe {
        drop(Box::from_raw(state_ptr));
    }

    Ok(())
}

// Central wndproc is in win::window so we can grow it.
#[allow(dead_code)]
pub(crate) extern "system" fn _noop_wndproc(
    _hwnd: HWND,
    _msg: u32,
    _wparam: WPARAM,
    _lparam: LPARAM,
) -> LRESULT {
    LRESULT(0)
}
