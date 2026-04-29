use crate::clipboard::item::{ClipboardFormatPayload, PayloadStorage};
use blake3::Hasher;
use std::sync::OnceLock;
use windows::core::w;
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

const CF_TEXT: u32 = 1;
const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;
const CF_HDROP: u32 = 15;

static HTML_FORMAT: OnceLock<u32> = OnceLock::new();
static RTF_FORMAT: OnceLock<u32> = OnceLock::new();

pub struct CapturedClipboard {
    pub fingerprint: [u8; 32],
    pub preview: String,
    pub formats: Vec<ClipboardFormatPayload>,
}

pub fn capture_clipboard() -> anyhow::Result<Option<CapturedClipboard>> {
    unsafe {
        if !open_clipboard_with_retry() {
            return Ok(None);
        }

        let html_id = html_format_id();
        let rtf_id = rtf_format_id();

        let mut formats = Vec::new();

        // Prefer stable canonical formats first; we also request them to defeat delayed rendering.
        maybe_push_global(CF_UNICODETEXT, &mut formats);
        maybe_push_global(CF_TEXT, &mut formats);
        if html_id != 0 {
            maybe_push_global(html_id, &mut formats);
        }
        if rtf_id != 0 {
            maybe_push_global(rtf_id, &mut formats);
        }
        maybe_push_global(CF_DIBV5, &mut formats);
        maybe_push_global(CF_DIB, &mut formats);
        maybe_push_global(CF_HDROP, &mut formats);

        // If we didn’t get anything from our prioritized list, fall back to “whatever is there”
        // for global-memory-backed formats.
        if formats.is_empty() {
            let mut f = 0u32;
            loop {
                f = windows::Win32::System::DataExchange::EnumClipboardFormats(f);
                if f == 0 {
                    break;
                }
                maybe_push_global(f, &mut formats);
                if formats.len() >= 16 {
                    break;
                }
            }
        }

        let preview = pick_preview(&formats, html_id, rtf_id);
        let fingerprint = pick_fingerprint(&formats, html_id);

        let _ = CloseClipboard();

        if formats.is_empty() {
            return Ok(None);
        }

        Ok(Some(CapturedClipboard {
            fingerprint,
            preview,
            formats,
        }))
    }
}

fn html_format_id() -> u32 {
    *HTML_FORMAT.get_or_init(|| unsafe { RegisterClipboardFormatW(w!("HTML Format")) })
}

fn rtf_format_id() -> u32 {
    *RTF_FORMAT.get_or_init(|| unsafe { RegisterClipboardFormatW(w!("Rich Text Format")) })
}

unsafe fn open_clipboard_with_retry() -> bool {
    for _ in 0..12 {
        if OpenClipboard(None).is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    false
}

unsafe fn maybe_push_global(format: u32, out: &mut Vec<ClipboardFormatPayload>) {
    if IsClipboardFormatAvailable(format).is_err() {
        return;
    }

    let handle = match GetClipboardData(format) {
        Ok(h) => h,
        Err(_) => return,
    };

    if let Some(bytes) = try_read_global_bytes(HGLOBAL(handle.0)) {
        out.push(ClipboardFormatPayload {
            format,
            storage: PayloadStorage::Inline(bytes),
        });
    }
}

unsafe fn try_read_global_bytes(hglobal: HGLOBAL) -> Option<Vec<u8>> {
    let size = GlobalSize(hglobal);
    if size == 0 {
        return None;
    }
    let ptr = GlobalLock(hglobal) as *const u8;
    if ptr.is_null() {
        return None;
    }
    let slice = std::slice::from_raw_parts(ptr, size);
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(slice);
    let _ = GlobalUnlock(hglobal);
    Some(out)
}

fn pick_preview(formats: &[ClipboardFormatPayload], html_id: u32, rtf_id: u32) -> String {
    if let Some(utf16) = formats.iter().find(|p| p.format == CF_UNICODETEXT) {
        if let Some(s) = try_decode_utf16z(payload_bytes(utf16)) {
            return make_preview(&normalize_text(&s));
        }
    }
    if html_id != 0 && formats.iter().any(|p| p.format == html_id) {
        return "<html>".to_string();
    }
    if rtf_id != 0 && formats.iter().any(|p| p.format == rtf_id) {
        return "<rtf>".to_string();
    }
    if formats
        .iter()
        .any(|p| p.format == CF_DIB || p.format == CF_DIBV5)
    {
        return "<image>".to_string();
    }
    if formats.iter().any(|p| p.format == CF_HDROP) {
        return "<files>".to_string();
    }
    "<clipboard>".to_string()
}

fn pick_fingerprint(formats: &[ClipboardFormatPayload], html_id: u32) -> [u8; 32] {
    if let Some(utf16) = formats.iter().find(|p| p.format == CF_UNICODETEXT) {
        if let Some(s) = try_decode_utf16z(payload_bytes(utf16)) {
            let normalized = normalize_text(&s);
            return fingerprint_bytes(normalized.as_bytes());
        }
    }
    if html_id != 0 {
        if let Some(html) = formats.iter().find(|p| p.format == html_id) {
            return fingerprint_bytes(payload_bytes(html));
        }
    }
    if let Some(img) = formats
        .iter()
        .find(|p| p.format == CF_DIBV5 || p.format == CF_DIB)
    {
        return fingerprint_bytes(payload_bytes(img));
    }
    if let Some(files) = formats.iter().find(|p| p.format == CF_HDROP) {
        return fingerprint_bytes(payload_bytes(files));
    }

    // Last resort: hash the full set.
    let mut h = Hasher::new();
    for p in formats {
        h.update(&p.format.to_le_bytes());
        let bytes = payload_bytes(p);
        h.update(&(bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    *h.finalize().as_bytes()
}

fn payload_bytes(p: &ClipboardFormatPayload) -> &[u8] {
    match &p.storage {
        PayloadStorage::Inline(b) => b.as_slice(),
        PayloadStorage::File { .. } => &[],
    }
}

fn fingerprint_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(bytes);
    *hasher.finalize().as_bytes()
}

fn try_decode_utf16z(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }
    let mut u16s = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        u16s.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    let nul = u16s.iter().position(|&u| u == 0).unwrap_or(u16s.len());
    Some(String::from_utf16_lossy(&u16s[..nul]))
}

fn normalize_text(s: &str) -> String {
    let s = s.replace("\r\n", "\n");
    s.trim_end_matches(['\u{0}', '\n', '\r', ' ', '\t'])
        .to_string()
}

fn make_preview(s: &str) -> String {
    let mut line = s.lines().next().unwrap_or("").to_string();
    if line.len() > 120 {
        line.truncate(120);
        line.push('…');
    }
    if line.is_empty() {
        "<empty>".to_string()
    } else {
        line
    }
}
