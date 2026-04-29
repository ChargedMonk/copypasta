use std::ffi::c_void;
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SetWindowLongPtrW,
    SetWindowTextW, ShowWindow, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, HMENU, SW_SHOW,
    WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN, WNDCLASSW, WS_CHILD,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

const SETTINGS_CLASS: PCWSTR = w!("RipMultiPasteSettingsWindow");
static SETTINGS_CLASS_REGISTERED: OnceLock<()> = OnceLock::new();

const CMD_RECORD_OPEN: usize = 1001;
const CMD_RECORD_CAPTURE: usize = 1002;
const CMD_RESET: usize = 1003;
const CMD_SAVE: usize = 1004;

#[derive(Clone, Copy)]
enum HotkeyAction {
    OpenPicker,
    Capture,
}

struct SettingsState {
    main_hwnd: HWND,
    config: crate::hotkeys::HotkeyConfig,
    original_config: crate::hotkeys::HotkeyConfig,
    recording: Option<HotkeyAction>,
    open_label_hwnd: HWND,
    capture_label_hwnd: HWND,
    status_hwnd: HWND,
}

pub fn open_settings(main_hwnd: HWND) -> anyhow::Result<()> {
    unsafe {
        ensure_settings_class_registered()?;

        let config = crate::hotkeys::HotkeyConfig::load_or_default();
        let state = Box::new(SettingsState {
            main_hwnd,
            original_config: config.clone(),
            config,
            recording: None,
            open_label_hwnd: HWND(std::ptr::null_mut()),
            capture_label_hwnd: HWND(std::ptr::null_mut()),
            status_hwnd: HWND(std::ptr::null_mut()),
        });
        let state_ptr = Box::into_raw(state);

        let hwnd = CreateWindowExW(
            Default::default(),
            SETTINGS_CLASS,
            w!("Rip Multi Paste Settings"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            460,
            250,
            Some(main_hwnd),
            None,
            Some(current_hinstance()?),
            Some(state_ptr as *mut c_void),
        )?;

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetFocus(Some(hwnd));
        Ok(())
    }
}

unsafe fn ensure_settings_class_registered() -> anyhow::Result<()> {
    if SETTINGS_CLASS_REGISTERED.get().is_some() {
        return Ok(());
    }

    let hinstance = current_hinstance()?;
    let wc = WNDCLASSW {
        hInstance: hinstance,
        lpszClassName: SETTINGS_CLASS,
        lpfnWndProc: Some(settings_wndproc),
        ..Default::default()
    };
    let atom = RegisterClassW(&wc);
    if atom == 0 {
        // Treat an already registered class as success during dev reloads.
    }
    let _ = SETTINGS_CLASS_REGISTERED.set(());
    Ok(())
}

unsafe fn current_hinstance() -> anyhow::Result<HINSTANCE> {
    let hmodule = GetModuleHandleW(None)?;
    Ok(HINSTANCE(hmodule.0))
}

extern "system" fn settings_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                let state_ptr = cs.lpCreateParams as *mut SettingsState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
                create_settings_controls(hwnd, &mut *state_ptr);
                LRESULT(0)
            }
            WM_COMMAND => {
                let cmd = wparam.0 & 0xffff;
                if handle_command(hwnd, cmd) {
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_KEYDOWN => {
                if handle_recording_key(hwnd, wparam.0 as u32) {
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                let state_ptr =
                    windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
                        as *mut SettingsState;
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

unsafe fn create_settings_controls(hwnd: HWND, state: &mut SettingsState) {
    let hinstance = current_hinstance().unwrap_or(HINSTANCE(std::ptr::null_mut()));

    create_static(hwnd, "Open picker", 20, 24, 120, 22, hinstance);
    state.open_label_hwnd = create_static(
        hwnd,
        &state.config.open_picker.label(),
        150,
        24,
        160,
        22,
        hinstance,
    );
    create_button(
        hwnd,
        "Record",
        CMD_RECORD_OPEN,
        (330, 20, 90, 28),
        hinstance,
    );

    create_static(hwnd, "Capture selection", 20, 68, 120, 22, hinstance);
    state.capture_label_hwnd = create_static(
        hwnd,
        &state.config.capture.label(),
        150,
        68,
        160,
        22,
        hinstance,
    );
    create_button(
        hwnd,
        "Record",
        CMD_RECORD_CAPTURE,
        (330, 64, 90, 28),
        hinstance,
    );

    create_button(hwnd, "Reset", CMD_RESET, (150, 120, 90, 30), hinstance);
    create_button(hwnd, "Save", CMD_SAVE, (250, 120, 90, 30), hinstance);
    state.status_hwnd = create_static(
        hwnd,
        "Choose Record, press a shortcut, then Save.",
        20,
        166,
        400,
        24,
        hinstance,
    );
}

unsafe fn create_static(
    parent: HWND,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    hinstance: HINSTANCE,
) -> HWND {
    let text = to_wide(text);
    CreateWindowExW(
        Default::default(),
        w!("STATIC"),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        width,
        height,
        Some(parent),
        None,
        Some(hinstance),
        None,
    )
    .unwrap_or(HWND(std::ptr::null_mut()))
}

unsafe fn create_button(
    parent: HWND,
    text: &str,
    id: usize,
    rect: (i32, i32, i32, i32),
    hinstance: HINSTANCE,
) -> HWND {
    let text = to_wide(text);
    let (x, y, width, height) = rect;
    CreateWindowExW(
        Default::default(),
        w!("BUTTON"),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0),
        x,
        y,
        width,
        height,
        Some(parent),
        Some(HMENU(id as *mut c_void)),
        Some(hinstance),
        None,
    )
    .unwrap_or(HWND(std::ptr::null_mut()))
}

unsafe fn handle_command(hwnd: HWND, cmd: usize) -> bool {
    let Some(state) = settings_state(hwnd) else {
        return false;
    };

    match cmd {
        CMD_RECORD_OPEN => {
            state.recording = Some(HotkeyAction::OpenPicker);
            set_status(state, "Press a shortcut for Open picker...");
            let _ = SetFocus(Some(hwnd));
            true
        }
        CMD_RECORD_CAPTURE => {
            state.recording = Some(HotkeyAction::Capture);
            set_status(state, "Press a shortcut for Capture selection...");
            let _ = SetFocus(Some(hwnd));
            true
        }
        CMD_RESET => {
            state.config.reset_defaults();
            state.recording = None;
            update_hotkey_labels(state);
            set_status(state, "Defaults restored. Save to apply.");
            true
        }
        CMD_SAVE => {
            save_hotkeys(state);
            true
        }
        _ => false,
    }
}

unsafe fn handle_recording_key(hwnd: HWND, vk: u32) -> bool {
    let Some(state) = settings_state(hwnd) else {
        return false;
    };
    let Some(action) = state.recording else {
        return false;
    };
    let Some(spec) = hotkey_spec_from_key(vk) else {
        set_status(
            state,
            "Use a letter, number, or function key with a modifier.",
        );
        return true;
    };

    match action {
        HotkeyAction::OpenPicker => state.config.open_picker = spec,
        HotkeyAction::Capture => state.config.capture = spec,
    }
    state.recording = None;
    update_hotkey_labels(state);
    set_status(state, "Shortcut captured. Save to apply.");
    true
}

unsafe fn save_hotkeys(state: &mut SettingsState) {
    if let Err(err) = state.config.validate() {
        set_status(state, &format!("Invalid shortcut: {err}"));
        return;
    }

    let previous = state.original_config.clone();
    let report = crate::hotkeys::apply_config(state.main_hwnd, &state.config);
    if !report.required_hotkeys_available() {
        let _ = crate::hotkeys::apply_config(state.main_hwnd, &previous);
        set_status(state, "Shortcut unavailable. Previous hotkeys restored.");
        return;
    }

    if let Err(err) = state.config.save() {
        let _ = crate::hotkeys::apply_config(state.main_hwnd, &previous);
        set_status(state, &format!("Could not save hotkeys: {err}"));
        return;
    }

    state.original_config = state.config.clone();
    set_status(state, "Hotkeys saved and applied.");
}

unsafe fn hotkey_spec_from_key(vk: u32) -> Option<crate::hotkeys::HotkeySpec> {
    if is_modifier_key(vk) {
        return None;
    }

    let key = key_name(vk)?;
    let mut modifiers = Vec::new();
    if is_key_down(VK_LWIN.0 as i32) || is_key_down(VK_RWIN.0 as i32) {
        modifiers.push("win".to_string());
    }
    if is_key_down(VK_CONTROL.0 as i32) {
        modifiers.push("ctrl".to_string());
    }
    if is_key_down(VK_MENU.0 as i32) {
        modifiers.push("alt".to_string());
    }
    if is_key_down(VK_SHIFT.0 as i32) {
        modifiers.push("shift".to_string());
    }
    if modifiers.is_empty() {
        return None;
    }

    Some(crate::hotkeys::HotkeySpec::from_owned(modifiers, key))
}

fn key_name(vk: u32) -> Option<String> {
    if (b'A' as u32..=b'Z' as u32).contains(&vk) || (b'0' as u32..=b'9' as u32).contains(&vk) {
        return Some((vk as u8 as char).to_string());
    }
    if (0x70..=0x87).contains(&vk) {
        return Some(format!("F{}", vk - 0x70 + 1));
    }
    None
}

fn is_modifier_key(vk: u32) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C)
}

unsafe fn is_key_down(vk: i32) -> bool {
    (GetKeyState(vk) & 0x8000u16 as i16) != 0
}

unsafe fn settings_state(hwnd: HWND) -> Option<&'static mut SettingsState> {
    let ptr = windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
        as *mut SettingsState;
    if ptr.is_null() {
        None
    } else {
        Some(&mut *ptr)
    }
}

unsafe fn update_hotkey_labels(state: &SettingsState) {
    set_text(state.open_label_hwnd, &state.config.open_picker.label());
    set_text(state.capture_label_hwnd, &state.config.capture.label());
}

unsafe fn set_status(state: &SettingsState, text: &str) {
    set_text(state.status_hwnd, text);
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    if hwnd.0.is_null() {
        return;
    }
    let wide = to_wide(text);
    let _ = SetWindowTextW(hwnd, PCWSTR(wide.as_ptr()));
}

fn to_wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}
