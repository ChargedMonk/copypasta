use crate::clipboard::item::{ClipboardItem, PayloadStorage};
use std::path::Path;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_C, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

pub fn set_clipboard_from_item(item: &ClipboardItem, base_dir: &Path) -> anyhow::Result<()> {
    unsafe {
        OpenClipboard(None)?;
        EmptyClipboard()?;

        for payload in &item.formats {
            let bytes_owned;
            let bytes: &[u8] = match &payload.storage {
                PayloadStorage::Inline(b) => b.as_slice(),
                PayloadStorage::File { rel_path, .. } => {
                    let abs = base_dir.join(rel_path);
                    bytes_owned = std::fs::read(abs)?;
                    bytes_owned.as_slice()
                }
            };
            if bytes.is_empty() {
                continue;
            }

            let hglobal: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
            let ptr = GlobalLock(hglobal) as *mut u8;
            if ptr.is_null() {
                let _ = GlobalUnlock(hglobal);
                continue;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            let _ = GlobalUnlock(hglobal);

            // On success, the system owns `hglobal` and we must not free it.
            let _ = SetClipboardData(payload.format, Some(HANDLE(hglobal.0)));
        }

        let _ = CloseClipboard();
    }
    Ok(())
}

pub fn paste_ctrl_v_into(target_hwnd: HWND) -> anyhow::Result<()> {
    unsafe {
        let _ = SetForegroundWindow(target_hwnd);

        send_ctrl_key(VK_V)?;
    }
    Ok(())
}

pub fn copy_ctrl_c_into(target_hwnd: HWND) -> anyhow::Result<()> {
    let target_hwnd_raw = target_hwnd.0 as usize;
    std::thread::spawn(move || {
        wait_for_hotkey_chord_release();

        unsafe {
            let target_hwnd = HWND(target_hwnd_raw as *mut std::ffi::c_void);
            let _ = SetForegroundWindow(target_hwnd);
            if let Err(err) = send_ctrl_key(VK_C) {
                tracing::warn!(error = ?err, "deferred Ctrl+C failed");
            }
        }
    });

    Ok(())
}

fn wait_for_hotkey_chord_release() {
    for _ in 0..50 {
        if !is_key_down(VK_CONTROL)
            && !is_key_down(VK_MENU)
            && !is_key_down(VK_LWIN)
            && !is_key_down(VK_RWIN)
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn is_key_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) & 0x8000u16 as i16) != 0 }
}

unsafe fn send_ctrl_key(vk: VIRTUAL_KEY) -> anyhow::Result<()> {
    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(vk, false),
        key_input(vk, true),
        key_input(VK_CONTROL, true),
    ];
    let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    if sent == 0 {
        anyhow::bail!("SendInput failed");
    }
    Ok(())
}

fn key_input(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    let mut ki = KEYBDINPUT::default();
    ki.wVk = vk;
    if key_up {
        ki.dwFlags = KEYEVENTF_KEYUP;
    }

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki },
    }
}
