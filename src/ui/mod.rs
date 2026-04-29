use crate::clipboard::item::ClipboardItem;
use std::ffi::c_void;
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, GetWindowLongPtrW,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    CREATESTRUCTW, GWLP_USERDATA, HMENU, SM_CXSCREEN, SM_CYSCREEN, SWP_NOZORDER, SW_SHOW,
    WINDOW_STYLE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN, WS_BORDER, WS_CHILD,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, LBN_DBLCLK, LBS_NOTIFY, LB_ADDSTRING, LB_GETCURSEL, LB_SETCURSEL,
    WS_VSCROLL,
};

const PICKER_CLASS: PCWSTR = w!("CopypastaPickerWindow");
static PICKER_CLASS_REGISTERED: OnceLock<()> = OnceLock::new();

pub fn open_picker(main_hwnd: HWND) -> anyhow::Result<()> {
    let state_ptr = unsafe { crate::win::window::get_userdata(main_hwnd) };
    if state_ptr.is_null() {
        return Ok(());
    }
    let state = unsafe { &*(state_ptr as *const crate::state::AppState) };

    let snapshot: Vec<ClipboardItem> = {
        let history = state.history.lock();
        history.items().cloned().take(50).collect()
    };
    if snapshot.is_empty() {
        return Ok(());
    }

    let target_hwnd = unsafe { GetForegroundWindow() };

    unsafe {
        ensure_picker_class_registered()?;

        let picker_state = Box::new(PickerState {
            app_state: state_ptr as *const crate::state::AppState,
            target_hwnd,
            list_hwnd: HWND(std::ptr::null_mut()),
            snapshot,
        });
        let picker_state_ptr = Box::into_raw(picker_state);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            PICKER_CLASS,
            w!("Copypasta"),
            WS_POPUP | WS_BORDER,
            0,
            0,
            520,
            360,
            None,
            None,
            Some(current_hinstance()?),
            Some(picker_state_ptr as *mut c_void),
        )?;

        // Center the picker.
        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);
        let x = (sw - 520) / 2;
        let y = (sh - 360) / 3;
        let _ = SetWindowPos(hwnd, None, x, y, 520, 360, SWP_NOZORDER);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        Ok(())
    }
}

unsafe fn ensure_picker_class_registered() -> anyhow::Result<()> {
    if PICKER_CLASS_REGISTERED.get().is_some() {
        return Ok(());
    }

    let hinstance = current_hinstance()?;
    let wc = windows::Win32::UI::WindowsAndMessaging::WNDCLASSW {
        hInstance: hinstance,
        lpszClassName: PICKER_CLASS,
        lpfnWndProc: Some(picker_wndproc),
        ..Default::default()
    };
    let atom = RegisterClassW(&wc);
    if atom == 0 {
        // Assume already registered; that's OK.
    }
    let _ = PICKER_CLASS_REGISTERED.set(());
    Ok(())
}

unsafe fn current_hinstance() -> anyhow::Result<HINSTANCE> {
    let hmodule = GetModuleHandleW(None)?;
    Ok(HINSTANCE(hmodule.0))
}

struct PickerState {
    app_state: *const crate::state::AppState,
    target_hwnd: HWND,
    list_hwnd: HWND,
    snapshot: Vec<ClipboardItem>,
}

extern "system" fn picker_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                let state_ptr = cs.lpCreateParams as *mut PickerState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);

                let state = &mut *state_ptr;

                // Child listbox.
                let list_hwnd = CreateWindowExW(
                    Default::default(),
                    w!("LISTBOX"),
                    w!(""),
                    WS_CHILD | WS_VISIBLE | WS_VSCROLL | WINDOW_STYLE(LBS_NOTIFY as u32),
                    8,
                    8,
                    500,
                    340,
                    Some(hwnd),
                    Some(HMENU(1usize as *mut c_void)),
                    Some(current_hinstance().unwrap_or(HINSTANCE(std::ptr::null_mut()))),
                    None,
                )
                .unwrap_or(HWND(std::ptr::null_mut()));

                state.list_hwnd = list_hwnd;

                // Populate listbox with previews.
                for item in &state.snapshot {
                    let wide = to_wide(&item.preview);
                    let _ = SendMessageW(
                        list_hwnd,
                        LB_ADDSTRING,
                        Some(WPARAM(0)),
                        Some(LPARAM(wide.as_ptr() as isize)),
                    );
                }

                let _ = SendMessageW(list_hwnd, LB_SETCURSEL, Some(WPARAM(0)), Some(LPARAM(0)));
                let _ = SetFocus(Some(list_hwnd));
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                if vk == VK_ESCAPE.0 {
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                if vk == VK_RETURN.0 {
                    do_select_and_paste(hwnd);
                    return LRESULT(0);
                }
                // Let listbox handle arrow keys normally when focused.
                if vk == VK_UP.0 || vk == VK_DOWN.0 {
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_COMMAND => {
                // Double-click on listbox.
                let notify = ((wparam.0 >> 16) & 0xffff) as u16;
                if notify as u32 == LBN_DBLCLK {
                    do_select_and_paste(hwnd);
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_DESTROY => {
                // Drop PickerState.
                let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PickerState;
                if !state_ptr.is_null() {
                    drop(Box::from_raw(state_ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn do_select_and_paste(hwnd: HWND) {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PickerState;
    if state_ptr.is_null() {
        return;
    }
    let state = &mut *state_ptr;
    if state.list_hwnd.0.is_null() {
        return;
    }

    let sel = SendMessageW(
        state.list_hwnd,
        LB_GETCURSEL,
        Some(WPARAM(0)),
        Some(LPARAM(0)),
    )
    .0 as i32;
    if sel < 0 {
        return;
    }
    let idx = sel as usize;
    let Some(item) = state.snapshot.get(idx) else {
        return;
    };

    let app = &*state.app_state;
    let now = unix_ms_now();
    *app.suppress_clipboard_until_unix_ms.lock() = now + 500;

    let store = app.store.lock();
    if crate::paste::set_clipboard_from_item(item, store.base_dir()).is_ok() {
        let _ = crate::paste::paste_ctrl_v_into(state.target_hwnd);
        app.history.lock().bump_by_fingerprint(item.fingerprint);
    }

    let _ = DestroyWindow(hwnd);
}

fn unix_ms_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_millis() as i64
}

fn to_wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}
