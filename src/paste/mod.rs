use crate::clipboard::item::{ClipboardItem, PayloadStorage};
use std::path::Path;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_V,
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

        let inputs = [
            key_input(VK_CONTROL, false),
            key_input(VK_V, false),
            key_input(VK_V, true),
            key_input(VK_CONTROL, true),
        ];
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent == 0 {
            anyhow::bail!("SendInput failed");
        }
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
