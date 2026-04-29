use crate::clipboard::item::{ClipboardItem, PayloadStorage};
use std::cmp::min;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, SelectObject, SetBkMode,
    SetTextColor, StretchDIBits, BITMAPINFO, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE,
    DT_VCENTER, FF_DONTCARE, FW_BOLD, FW_NORMAL, HBRUSH, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS,
    SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SetFocus, VK_BACK, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, GetWindowLongPtrW,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    CREATESTRUCTW, GWLP_USERDATA, HMENU, SM_CXSCREEN, SM_CYSCREEN, SWP_NOZORDER, SW_SHOW,
    WA_INACTIVE, WINDOW_STYLE, WM_ACTIVATE, WM_CHAR, WM_COMMAND, WM_CREATE, WM_DESTROY,
    WM_DRAWITEM, WM_KEYDOWN, WM_MEASUREITEM, WM_VKEYTOITEM, WS_BORDER, WS_CHILD, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, LBN_DBLCLK, LBS_NOTIFY, LBS_OWNERDRAWFIXED, LBS_WANTKEYBOARDINPUT,
    LB_ADDSTRING, LB_GETCOUNT, LB_GETCURSEL, LB_RESETCONTENT, LB_SETCURSEL, LB_SETITEMHEIGHT,
    WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_SETFONT, WS_VSCROLL,
};

const PICKER_CLASS: PCWSTR = w!("CopypastaPickerWindow");
static PICKER_CLASS_REGISTERED: OnceLock<()> = OnceLock::new();
const PICKER_WIDTH: i32 = 560;
const PICKER_HEIGHT: i32 = 420;
const HEADER_ID: usize = 10;
const LIST_ID: usize = 11;
const FOOTER_ID: usize = 12;
const ROW_HEIGHT: i32 = 68;
const THUMB_SIZE: i32 = 44;
const COLOR_BG: COLORREF = rgb(248, 245, 239);
const COLOR_CARD: COLORREF = rgb(255, 252, 247);
const COLOR_SELECTED: COLORREF = rgb(224, 235, 224);
const COLOR_TEXT: COLORREF = rgb(36, 36, 36);
const COLOR_MUTED: COLORREF = rgb(111, 106, 99);
const COLOR_ACCENT: COLORREF = rgb(118, 150, 126);

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

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
    let base_dir = state.store.lock().base_dir().to_path_buf();

    unsafe {
        ensure_picker_class_registered()?;

        let picker_state = Box::new(PickerState {
            app_state: state_ptr as *const crate::state::AppState,
            target_hwnd,
            header_hwnd: HWND(std::ptr::null_mut()),
            list_hwnd: HWND(std::ptr::null_mut()),
            footer_hwnd: HWND(std::ptr::null_mut()),
            row_font: HFONT(std::ptr::null_mut()),
            meta_font: HFONT(std::ptr::null_mut()),
            brush_bg: HBRUSH(std::ptr::null_mut()),
            brush_card: HBRUSH(std::ptr::null_mut()),
            brush_selected: HBRUSH(std::ptr::null_mut()),
            base_dir,
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
    row_font: HFONT,
    meta_font: HFONT,
    brush_bg: HBRUSH,
    brush_card: HBRUSH,
    brush_selected: HBRUSH,
    base_dir: PathBuf,
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
                state.row_font = create_ui_font(18, FW_NORMAL.0 as i32);
                state.meta_font = create_ui_font(14, FW_NORMAL.0 as i32);
                state.brush_bg = CreateSolidBrush(COLOR_BG);
                state.brush_card = CreateSolidBrush(COLOR_CARD);
                state.brush_selected = CreateSolidBrush(COLOR_SELECTED);

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
                        | WINDOW_STYLE(
                            (LBS_NOTIFY | LBS_WANTKEYBOARDINPUT | LBS_OWNERDRAWFIXED) as u32,
                        ),
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

                apply_font(header_hwnd, state.row_font);
                apply_font(list_hwnd, state.row_font);
                apply_font(footer_hwnd, state.meta_font);
                let _ = SendMessageW(
                    list_hwnd,
                    LB_SETITEMHEIGHT,
                    Some(WPARAM(0)),
                    Some(LPARAM(ROW_HEIGHT as isize)),
                );

                refresh_visible_items(state);
                let _ = SetFocus(Some(hwnd));
                LRESULT(0)
            }
            WM_MEASUREITEM => {
                let measure = &mut *(lparam.0 as *mut MEASUREITEMSTRUCT);
                measure.itemHeight = ROW_HEIGHT as u32;
                LRESULT(1)
            }
            WM_DRAWITEM => {
                if draw_picker_item(hwnd, lparam) {
                    return LRESULT(1);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORLISTBOX => {
                let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PickerState;
                if !state_ptr.is_null() {
                    let state = &*state_ptr;
                    let hdc = windows::Win32::Graphics::Gdi::HDC(wparam.0 as *mut c_void);
                    let _ = SetBkMode(hdc, TRANSPARENT);
                    let _ = SetTextColor(hdc, COLOR_TEXT);
                    return LRESULT(state.brush_bg.0 as isize);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
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
                    let state = &*state_ptr;
                    delete_gdi_object(state.row_font);
                    delete_gdi_object(state.meta_font);
                    delete_gdi_object(state.brush_bg);
                    delete_gdi_object(state.brush_card);
                    delete_gdi_object(state.brush_selected);
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
            let _ = SendMessageW(
                state.list_hwnd,
                LB_ADDSTRING,
                Some(WPARAM(0)),
                Some(LPARAM(item.id as isize)),
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

unsafe fn create_ui_font(size: i32, weight: i32) -> HFONT {
    CreateFontW(
        -size,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
        w!("Segoe UI Variable"),
    )
}

unsafe fn apply_font(hwnd: HWND, font: HFONT) {
    if !hwnd.0.is_null() && !font.0.is_null() {
        let _ = SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
    }
}

unsafe fn delete_gdi_object<T>(object: T)
where
    T: Into<HGDIOBJ>,
{
    let object = object.into();
    if !object.0.is_null() {
        let _ = DeleteObject(object);
    }
}

unsafe fn draw_picker_item(hwnd: HWND, lparam: LPARAM) -> bool {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PickerState;
    if state_ptr.is_null() || lparam.0 == 0 {
        return false;
    }
    let state = &*state_ptr;
    let draw = &*(lparam.0 as *const DRAWITEMSTRUCT);
    if draw.itemID == u32::MAX {
        return true;
    }
    let Some(item) = selected_item(state, draw.itemID as usize) else {
        return true;
    };

    let selected = (draw.itemState.0 & ODS_SELECTED.0) != 0;
    let brush = if selected {
        state.brush_selected
    } else {
        state.brush_card
    };
    let _ = FillRect(draw.hDC, &draw.rcItem, brush);
    let _ = SetBkMode(draw.hDC, TRANSPARENT);

    let thumb = RECT {
        left: draw.rcItem.left + 12,
        top: draw.rcItem.top + 12,
        right: draw.rcItem.left + 12 + THUMB_SIZE,
        bottom: draw.rcItem.top + 12 + THUMB_SIZE,
    };
    draw_thumbnail_or_badge(draw.hDC, &thumb, item, &state.base_dir);

    let text_left = thumb.right + 12;
    let text_right = draw.rcItem.right - 12;
    let mut title_rect = RECT {
        left: text_left,
        top: draw.rcItem.top + 11,
        right: text_right,
        bottom: draw.rcItem.top + 36,
    };
    let mut meta_rect = RECT {
        left: text_left,
        top: draw.rcItem.top + 38,
        right: text_right,
        bottom: draw.rcItem.bottom - 8,
    };

    let old_font = SelectObject(draw.hDC, HGDIOBJ(state.row_font.0));
    let _ = SetTextColor(draw.hDC, COLOR_TEXT);
    draw_text(draw.hDC, &item.preview, &mut title_rect);
    let _ = SelectObject(draw.hDC, old_font);

    let old_font = SelectObject(draw.hDC, HGDIOBJ(state.meta_font.0));
    let _ = SetTextColor(draw.hDC, COLOR_MUTED);
    draw_text(draw.hDC, item_kind_label(item), &mut meta_rect);
    let _ = SelectObject(draw.hDC, old_font);
    true
}

unsafe fn draw_text(hdc: windows::Win32::Graphics::Gdi::HDC, text: &str, rect: &mut RECT) {
    let mut wide = to_wide(text);
    let _ = DrawTextW(
        hdc,
        &mut wide,
        rect,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

unsafe fn draw_thumbnail_or_badge(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    rect: &RECT,
    item: &ClipboardItem,
    base_dir: &Path,
) {
    if draw_dib_thumbnail(hdc, rect, item, base_dir) {
        return;
    }

    let badge_brush = CreateSolidBrush(rgb(232, 226, 216));
    let _ = FillRect(hdc, rect, badge_brush);
    delete_gdi_object(badge_brush);
    let mut badge_rect = *rect;
    let badge_font = CreateFontW(
        -13,
        0,
        0,
        0,
        FW_BOLD.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
        w!("Segoe UI Variable"),
    );
    let old_font = SelectObject(hdc, HGDIOBJ(badge_font.0));
    let _ = SetTextColor(hdc, COLOR_ACCENT);
    draw_text(hdc, item_badge(item), &mut badge_rect);
    let _ = SelectObject(hdc, old_font);
    delete_gdi_object(badge_font);
}

unsafe fn draw_dib_thumbnail(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    rect: &RECT,
    item: &ClipboardItem,
    base_dir: &Path,
) -> bool {
    let Some(bytes) = image_payload_bytes(item, base_dir) else {
        return false;
    };
    let Some((width, height, bits_offset)) = dib_info(&bytes) else {
        return false;
    };
    let bits = bytes.as_ptr().add(bits_offset) as *const c_void;
    let info = bytes.as_ptr() as *const BITMAPINFO;
    let _ = StretchDIBits(
        hdc,
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
        0,
        0,
        width,
        height,
        Some(bits),
        info,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
    true
}

fn image_payload_bytes(item: &ClipboardItem, base_dir: &Path) -> Option<Vec<u8>> {
    let payload = item.formats.iter().find(|payload| {
        payload.format == crate::clipboard::formats::CF_DIB
            || payload.format == crate::clipboard::formats::CF_DIBV5
    })?;
    match &payload.storage {
        PayloadStorage::Inline(bytes) => Some(bytes.clone()),
        PayloadStorage::File { rel_path, size } => {
            if *size > 8 * 1024 * 1024 {
                return None;
            }
            std::fs::read(base_dir.join(rel_path)).ok()
        }
    }
}

fn dib_info(bytes: &[u8]) -> Option<(i32, i32, usize)> {
    if bytes.len() < 40 {
        return None;
    }
    let header_size = u32::from_le_bytes(bytes[0..4].try_into().ok()?) as usize;
    if header_size < 40 || header_size > bytes.len() {
        return None;
    }
    let width = i32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let height = i32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let bit_count = u16::from_le_bytes(bytes[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
    let colors_used = u32::from_le_bytes(bytes[32..36].try_into().ok()?) as usize;
    let color_entries = if bit_count <= 8 {
        if colors_used == 0 {
            1usize << bit_count
        } else {
            colors_used
        }
    } else {
        0
    };
    let masks = if compression == 3 && header_size == 40 {
        12
    } else {
        0
    };
    let bits_offset = min(bytes.len(), header_size + masks + color_entries * 4);
    if width == 0 || height == 0 || bits_offset >= bytes.len() {
        return None;
    }
    Some((width.abs(), height.abs(), bits_offset))
}

fn item_kind_label(item: &ClipboardItem) -> &'static str {
    if item.formats.iter().any(|p| {
        p.format == crate::clipboard::formats::CF_DIB
            || p.format == crate::clipboard::formats::CF_DIBV5
    }) {
        "Image preview"
    } else if item.preview.starts_with("HTML:") {
        "HTML content"
    } else if item.preview.starts_with("Rich text:") {
        "Rich text"
    } else if item.preview.contains(" file") {
        "File drop"
    } else {
        "Text"
    }
}

fn item_badge(item: &ClipboardItem) -> &'static str {
    if item.formats.iter().any(|p| {
        p.format == crate::clipboard::formats::CF_DIB
            || p.format == crate::clipboard::formats::CF_DIBV5
    }) {
        "IMG"
    } else if item.preview.starts_with("HTML:") {
        "HTML"
    } else if item.preview.starts_with("Rich text:") {
        "RTF"
    } else if item.preview.contains(" file") {
        "FILE"
    } else {
        "TXT"
    }
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
