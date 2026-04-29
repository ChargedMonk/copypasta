use anyhow::Context;
use std::ffi::c_void;
use std::time::Instant;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow, GetWindowLongPtrW,
    PostQuitMessage, RegisterClassW, SetWindowLongPtrW, CREATESTRUCTW, GWLP_USERDATA,
    WM_CLIPBOARDUPDATE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_HOTKEY, WNDCLASSW,
    WS_OVERLAPPED,
};

const WINDOW_CLASS: PCWSTR = w!("RipMultiPasteHiddenWindow");

pub struct HiddenWindow {
    hwnd: HWND,
}

impl HiddenWindow {
    pub fn create(userdata: *mut c_void) -> anyhow::Result<Self> {
        unsafe {
            let hmodule = GetModuleHandleW(None).context("GetModuleHandleW")?;
            let hinstance = HINSTANCE(hmodule.0);

            let wc = WNDCLASSW {
                hInstance: hinstance,
                lpszClassName: WINDOW_CLASS,
                lpfnWndProc: Some(wndproc),
                ..Default::default()
            };

            let atom = RegisterClassW(&wc);
            if atom == 0 {
                // RegisterClassW fails if already registered in this process; for dev runs,
                // we treat that as ok and continue.
                // We'll still try to create the window below.
            }

            let hwnd = CreateWindowExW(
                Default::default(),
                WINDOW_CLASS,
                w!("Rip Multi Paste"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinstance),
                Some(userdata),
            )
            .context("CreateWindowExW")?;

            if hwnd.0.is_null() {
                anyhow::bail!("CreateWindowExW returned null HWND");
            }

            Ok(Self { hwnd })
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

impl Drop for HiddenWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.hwnd.0.is_null() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                // Store a null app pointer for now; later milestones attach state here.
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                if let Err(err) = crate::tray::add(hwnd) {
                    tracing::warn!(error = ?err, "tray init failed");
                }
                LRESULT(0)
            }
            crate::tray::WM_TRAYICON => {
                if crate::tray::handle_tray_message(hwnd, lparam) {
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_COMMAND => {
                if crate::tray::handle_command(hwnd, wparam) {
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CLIPBOARDUPDATE => {
                crate::clipboard::watcher::on_clipboard_update(hwnd);
                LRESULT(0)
            }
            WM_HOTKEY => {
                let id = wparam.0 as i32;
                let hotkey_received_at = Instant::now();
                tracing::info!(hotkey_id = id, "hotkey");
                match id {
                    crate::hotkeys::HK_OPEN_PICKER => {
                        if let Err(err) =
                            crate::ui::open_picker_from_hotkey(hwnd, hotkey_received_at)
                        {
                            tracing::warn!(error = ?err, "open picker failed");
                        }
                    }
                    crate::hotkeys::HK_CAPTURE => {
                        let target_hwnd = GetForegroundWindow();
                        if let Err(err) = crate::paste::copy_ctrl_c_into(target_hwnd) {
                            tracing::warn!(error = ?err, "copy hotkey failed");
                        }
                    }
                    crate::hotkeys::HK_QUIT_DEV => {
                        tracing::info!("quit hotkey pressed");
                        let _ = DestroyWindow(hwnd);
                        PostQuitMessage(0);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                tracing::info!("close requested");
                let _ = DestroyWindow(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_DESTROY => {
                crate::hotkeys::unregister(hwnd);
                crate::tray::remove(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

pub unsafe fn get_userdata(hwnd: HWND) -> *mut c_void {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut c_void
}
