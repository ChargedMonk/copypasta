use anyhow::Context;
use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyWindow, GetCursorPos, GetWindowLongPtrW, LoadIconW,
    PostMessageW, PostQuitMessage, SetForegroundWindow, TrackPopupMenu, GWLP_USERDATA, HMENU,
    IDI_APPLICATION, MF_STRING, TPM_LEFTALIGN, TPM_RETURNCMD, WM_APP, WM_COMMAND, WM_LBUTTONDBLCLK,
    WM_NULL, WM_RBUTTONUP,
};

pub const WM_TRAYICON: u32 = WM_APP + 1;
const TRAY_UID: u32 = 1;

const CMD_TOGGLE_PAUSE: u16 = 1001;
const CMD_CLEAR_HISTORY: u16 = 1002;
const CMD_EXIT: u16 = 1003;
const CMD_SETTINGS: u16 = 1004;

pub fn add(hwnd: HWND) -> anyhow::Result<()> {
    unsafe {
        let hicon = LoadIconW(None, IDI_APPLICATION).context("LoadIconW")?;
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_UID,
            uFlags: NIF_MESSAGE | NIF_TIP | NIF_ICON,
            uCallbackMessage: WM_TRAYICON,
            hIcon: hicon,
            ..Default::default()
        };

        let tip = "Copypasta";
        let mut wide: Vec<u16> = tip.encode_utf16().collect();
        wide.push(0);
        for (i, ch) in wide.into_iter().take(nid.szTip.len()).enumerate() {
            nid.szTip[i] = ch;
        }

        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            anyhow::bail!("Shell_NotifyIconW(NIM_ADD) failed");
        }
    }
    Ok(())
}

pub fn remove(hwnd: HWND) {
    unsafe {
        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_UID,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

pub fn handle_tray_message(hwnd: HWND, lparam: LPARAM) -> bool {
    unsafe {
        match lparam.0 as u32 {
            WM_RBUTTONUP => {
                show_menu(hwnd);
                true
            }
            WM_LBUTTONDBLCLK => {
                let _ = crate::ui::open_picker(hwnd);
                true
            }
            _ => false,
        }
    }
}

pub fn handle_command(hwnd: HWND, wparam: WPARAM) -> bool {
    let cmd = (wparam.0 & 0xffff) as u16;
    unsafe {
        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut c_void;
        if state_ptr.is_null() {
            return false;
        }
        let state = &*(state_ptr as *const crate::state::AppState);

        match cmd {
            CMD_TOGGLE_PAUSE => {
                let mut paused = state.capture_paused.lock();
                *paused = !*paused;
                true
            }
            CMD_CLEAR_HISTORY => {
                let _ = state.store.lock().clear_all();
                state.history.lock().clear();
                true
            }
            CMD_SETTINGS => {
                if let Err(err) = crate::settings::open_settings(hwnd) {
                    tracing::warn!(error = ?err, "open settings failed");
                }
                true
            }
            CMD_EXIT => {
                let _ = DestroyWindow(hwnd);
                PostQuitMessage(0);
                true
            }
            _ => false,
        }
    }
}

unsafe fn show_menu(hwnd: HWND) {
    let hmenu = CreatePopupMenu().unwrap_or(HMENU(std::ptr::null_mut()));
    if hmenu.0.is_null() {
        return;
    }

    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut c_void;
    let paused = if state_ptr.is_null() {
        false
    } else {
        let state = &*(state_ptr as *const crate::state::AppState);
        *state.capture_paused.lock()
    };

    let pause_label = if paused {
        "Resume capture"
    } else {
        "Pause capture"
    };
    let pause_w = to_wide(pause_label);
    let _ = AppendMenuW(
        hmenu,
        MF_STRING,
        CMD_TOGGLE_PAUSE as usize,
        PCWSTR(pause_w.as_ptr()),
    );
    let _ = AppendMenuW(
        hmenu,
        MF_STRING,
        CMD_CLEAR_HISTORY as usize,
        w!("Clear history"),
    );
    let _ = AppendMenuW(hmenu, MF_STRING, CMD_SETTINGS as usize, w!("Settings"));
    let _ = AppendMenuW(hmenu, MF_STRING, CMD_EXIT as usize, w!("Exit"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);

    let cmd_bool = TrackPopupMenu(
        hmenu,
        TPM_LEFTALIGN | TPM_RETURNCMD,
        pt.x,
        pt.y,
        None,
        hwnd,
        None,
    );
    let cmd = cmd_bool.0 as u16;

    // Required for proper menu dismissal.
    let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
    let _ = SetFocus(Some(hwnd));

    if cmd != 0 {
        // Relay as WM_COMMAND to reuse existing handler logic.
        let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(cmd as usize), LPARAM(0));
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}
