use crate::clipboard::item::ClipboardItem;
use std::ffi::c_void;
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, VK_BACK, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, GetWindowLongPtrW,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    CREATESTRUCTW, GWLP_USERDATA, HMENU, SM_CXSCREEN, SM_CYSCREEN, SWP_NOZORDER, SW_SHOW,
    WA_INACTIVE, WINDOW_STYLE, WM_ACTIVATE, WM_CHAR, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN,
    WM_VKEYTOITEM, WS_BORDER, WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, LBN_DBLCLK, LBS_NOTIFY, LBS_WANTKEYBOARDINPUT, LB_ADDSTRING, LB_GETCOUNT,
    LB_GETCURSEL, LB_RESETCONTENT, LB_SETCURSEL, WS_VSCROLL,
};

const PICKER_CLASS: PCWSTR = w!("CopypastaPickerWindow");
static PICKER_CLASS_REGISTERED: OnceLock<()> = OnceLock::new();
const PICKER_WIDTH: i32 = 560;
const PICKER_HEIGHT: i32 = 420;
const HEADER_ID: usize = 10;
const LIST_ID: usize = 11;
const FOOTER_ID: usize = 12;

pub fn open_picker(main_hwnd: HWND) -> anyhow::Result<()> {
    let state_ptr = unsafe { crate::win::window::get_userdata(main_hwnd) };
    if state_ptr.is_null() {
        return Ok(());
    }
    let state = unsafe { &*(state_ptr as *const crate::state::AppState) };

    let snapshot: Vec<ClipboardItem> = {
        let history = state.history.lock();
        history.items().take(50).cloned().collect()
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
            header_hwnd: HWND(std::ptr::null_mut()),
            list_hwnd: HWND(std::ptr::null_mut()),
            footer_hwnd: HWND(std::ptr::null_mut()),
            all_items: snapshot,
            visible_items: Vec::new(),
            search_text: String::new(),
        });
        let picker_state_ptr = Box::into_raw(picker_state);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            PICKER_CLASS,
            w!("Copypasta"),
            WS_POPUP | WS_BORDER,
            0,
            0,
            PICKER_WIDTH,
            PICKER_HEIGHT,
            None,
            None,
            Some(current_hinstance()?),
            Some(picker_state_ptr as *mut c_void),
        )?;

        // Center the picker.
        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);
        let x = (sw - PICKER_WIDTH) / 2;
        let y = (sh - PICKER_HEIGHT) / 3;
        let _ = SetWindowPos(hwnd, None, x, y, PICKER_WIDTH, PICKER_HEIGHT, SWP_NOZORDER);

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
    header_hwnd: HWND,
    list_hwnd: HWND,
    footer_hwnd: HWND,
    all_items: Vec<ClipboardItem>,
    visible_items: Vec<usize>,
    search_text: String,
}

extern "system" fn picker_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                let state_ptr = cs.lpCreateParams as *mut PickerState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);

                let state = &mut *state_ptr;

                let header_hwnd = CreateWindowExW(
                    Default::default(),
                    w!("STATIC"),
                    w!("Search clipboard history"),
                    WS_CHILD | WS_VISIBLE,
                    14,
                    12,
                    532,
                    24,
                    Some(hwnd),
                    Some(HMENU(HEADER_ID as *mut c_void)),
                    Some(current_hinstance().unwrap_or(HINSTANCE(std::ptr::null_mut()))),
                    None,
                )
                .unwrap_or(HWND(std::ptr::null_mut()));

                let list_hwnd = CreateWindowExW(
                    Default::default(),
                    w!("LISTBOX"),
                    w!(""),
                    WS_CHILD
                        | WS_VISIBLE
                        | WS_VSCROLL
                        | WINDOW_STYLE((LBS_NOTIFY | LBS_WANTKEYBOARDINPUT) as u32),
                    14,
                    42,
                    532,
                    330,
                    Some(hwnd),
                    Some(HMENU(LIST_ID as *mut c_void)),
                    Some(current_hinstance().unwrap_or(HINSTANCE(std::ptr::null_mut()))),
                    None,
                )
                .unwrap_or(HWND(std::ptr::null_mut()));

                let footer_hwnd = CreateWindowExW(
                    Default::default(),
                    w!("STATIC"),
                    w!("Type to search   Enter paste   Del delete   Esc close"),
                    WS_CHILD | WS_VISIBLE,
                    14,
                    382,
                    532,
                    22,
                    Some(hwnd),
                    Some(HMENU(FOOTER_ID as *mut c_void)),
                    Some(current_hinstance().unwrap_or(HINSTANCE(std::ptr::null_mut()))),
                    None,
                )
                .unwrap_or(HWND(std::ptr::null_mut()));

                state.header_hwnd = header_hwnd;
                state.list_hwnd = list_hwnd;
                state.footer_hwnd = footer_hwnd;

                refresh_visible_items(state);
                let _ = SetFocus(Some(hwnd));
                LRESULT(0)
            }
            WM_ACTIVATE => {
                if (wparam.0 & 0xffff) as u32 == WA_INACTIVE {
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN => {
                if handle_picker_key(hwnd, wparam.0 as u16) {
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CHAR => {
                if handle_picker_char(hwnd, wparam.0 as u32) {
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_VKEYTOITEM => {
                if handle_picker_key(hwnd, (wparam.0 & 0xffff) as u16) {
                    return LRESULT(-2);
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

unsafe fn handle_picker_key(hwnd: HWND, vk: u16) -> bool {
    if vk == VK_ESCAPE.0 {
        let _ = DestroyWindow(hwnd);
        return true;
    }
    if vk == VK_RETURN.0 {
        do_select_and_paste(hwnd);
        return true;
    }
    if vk == VK_DELETE.0 {
        delete_selected_item(hwnd);
        return true;
    }
    if vk == VK_UP.0 {
        move_selection(hwnd, -1);
        return true;
    }
    if vk == VK_DOWN.0 {
        move_selection(hwnd, 1);
        return true;
    }
    false
}

enum SearchEdit {
    Append(char),
    Backspace,
}

unsafe fn handle_picker_char(hwnd: HWND, ch: u32) -> bool {
    if ch == VK_BACK.0 as u32 {
        update_search(hwnd, SearchEdit::Backspace);
        return true;
    }
    let Some(ch) = char::from_u32(ch) else {
        return false;
    };
    if ch.is_control() {
        return false;
    }
    update_search(hwnd, SearchEdit::Append(ch));
    true
}

unsafe fn update_search(hwnd: HWND, edit: SearchEdit) {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PickerState;
    if state_ptr.is_null() {
        return;
    }
    let state = &mut *state_ptr;
    match edit {
        SearchEdit::Append(ch) => state.search_text.push(ch),
        SearchEdit::Backspace => {
            state.search_text.pop();
        }
    }
    refresh_visible_items(state);
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
    let Some(item) = selected_item(state, sel as usize).cloned() else {
        return;
    };

    let app = &*state.app_state;
    let now = unix_ms_now();
    *app.suppress_clipboard_until_unix_ms.lock() = now + 500;

    let store = app.store.lock();
    if crate::paste::set_clipboard_from_item(&item, store.base_dir()).is_ok() {
        let _ = crate::paste::paste_ctrl_v_into(state.target_hwnd);
        app.history.lock().bump_by_fingerprint(item.fingerprint);
    }

    let _ = DestroyWindow(hwnd);
}

unsafe fn delete_selected_item(hwnd: HWND) {
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
    let Some(item) = selected_item(state, sel as usize).cloned() else {
        return;
    };

    let app = &*state.app_state;
    if let Err(err) = app.store.lock().delete_item(item.id) {
        tracing::warn!(error = ?err, item_id = item.id, "delete clipboard item failed");
        return;
    }
    app.history.lock().remove_by_id(item.id);
    state.all_items.retain(|existing| existing.id != item.id);
    if state.all_items.is_empty() {
        let _ = DestroyWindow(hwnd);
        return;
    }

    refresh_visible_items(state);
    let count = SendMessageW(
        state.list_hwnd,
        LB_GETCOUNT,
        Some(WPARAM(0)),
        Some(LPARAM(0)),
    )
    .0 as i32;
    if count <= 0 {
        return;
    }
    let next = (sel as usize).min((count - 1) as usize);
    let _ = SendMessageW(
        state.list_hwnd,
        LB_SETCURSEL,
        Some(WPARAM(next)),
        Some(LPARAM(0)),
    );
}

unsafe fn move_selection(hwnd: HWND, delta: i32) {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PickerState;
    if state_ptr.is_null() {
        return;
    }
    let state = &mut *state_ptr;
    if state.list_hwnd.0.is_null() {
        return;
    }

    let count = SendMessageW(
        state.list_hwnd,
        LB_GETCOUNT,
        Some(WPARAM(0)),
        Some(LPARAM(0)),
    )
    .0 as i32;
    if count <= 0 {
        return;
    }

    let current = SendMessageW(
        state.list_hwnd,
        LB_GETCURSEL,
        Some(WPARAM(0)),
        Some(LPARAM(0)),
    )
    .0 as i32;
    let current = if current < 0 { 0 } else { current };
    let next = (current + delta).clamp(0, count - 1);

    let _ = SendMessageW(
        state.list_hwnd,
        LB_SETCURSEL,
        Some(WPARAM(next as usize)),
        Some(LPARAM(0)),
    );
}

unsafe fn refresh_visible_items(state: &mut PickerState) {
    let query = state.search_text.trim().to_ascii_lowercase();
    state.visible_items = state
        .all_items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            if query.is_empty() || item.preview.to_ascii_lowercase().contains(&query) {
                Some(idx)
            } else {
                None
            }
        })
        .collect();

    if !state.header_hwnd.0.is_null() {
        let label = if state.search_text.is_empty() {
            format!(
                "Search clipboard history  -  {} items",
                state.all_items.len()
            )
        } else {
            format!(
                "Search: {}  -  {} matches",
                state.search_text,
                state.visible_items.len()
            )
        };
        let wide = to_wide(&label);
        let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(
            state.header_hwnd,
            PCWSTR(wide.as_ptr()),
        );
    }

    if state.list_hwnd.0.is_null() {
        return;
    }
    let _ = SendMessageW(
        state.list_hwnd,
        LB_RESETCONTENT,
        Some(WPARAM(0)),
        Some(LPARAM(0)),
    );
    for idx in &state.visible_items {
        if let Some(item) = state.all_items.get(*idx) {
            let wide = to_wide(&item.preview);
            let _ = SendMessageW(
                state.list_hwnd,
                LB_ADDSTRING,
                Some(WPARAM(0)),
                Some(LPARAM(wide.as_ptr() as isize)),
            );
        }
    }
    if !state.visible_items.is_empty() {
        let _ = SendMessageW(
            state.list_hwnd,
            LB_SETCURSEL,
            Some(WPARAM(0)),
            Some(LPARAM(0)),
        );
    }
}

fn selected_item(state: &PickerState, visible_idx: usize) -> Option<&ClipboardItem> {
    let item_idx = *state.visible_items.get(visible_idx)?;
    state.all_items.get(item_idx)
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
