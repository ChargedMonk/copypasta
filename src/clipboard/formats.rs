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

pub const CF_TEXT: u32 = 1;
pub const CF_UNICODETEXT: u32 = 13;
pub const CF_DIB: u32 = 8;
pub const CF_DIBV5: u32 = 17;
pub const CF_HDROP: u32 = 15;

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
    if html_id != 0 {
        if let Some(html) = formats.iter().find(|p| p.format == html_id) {
            if let Some(s) = try_decode_utf8(payload_bytes(html)) {
                let text = html_to_preview_text(&s);
                if !text.is_empty() {
                    return format!("HTML: {}", make_preview(&text));
                }
            }
            return "HTML content".to_string();
        }
    }
    if rtf_id != 0 {
        if let Some(rtf) = formats.iter().find(|p| p.format == rtf_id) {
            if let Some(s) = try_decode_utf8(payload_bytes(rtf)) {
                let text = rtf_to_preview_text(&s);
                if !text.is_empty() {
                    return format!("Rich text: {}", make_preview(&text));
                }
            }
            return "Rich text".to_string();
        }
    }
    if formats
        .iter()
        .any(|p| p.format == CF_DIB || p.format == CF_DIBV5)
    {
        return "Image content".to_string();
    }
    if let Some(files) = formats.iter().find(|p| p.format == CF_HDROP) {
        if let Some(label) = files_to_preview_text(payload_bytes(files)) {
            return label;
        }
        return "Files".to_string();
    }
    "Clipboard content".to_string()
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

fn try_decode_utf8(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes)
        .map(|s| s.trim_end_matches('\0').to_string())
        .ok()
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

fn html_to_preview_text(html: &str) -> String {
    let fragment = html_fragment(html).unwrap_or(html);
    let mut out = String::with_capacity(fragment.len().min(256));
    let mut in_tag = false;
    let mut entity = String::new();
    let mut in_entity = false;

    for ch in fragment.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
                out.push(' ');
            }
            continue;
        }
        if in_entity {
            if ch == ';' {
                out.push_str(match entity.as_str() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "nbsp" => " ",
                    _ => "",
                });
                entity.clear();
                in_entity = false;
            } else if entity.len() < 12 {
                entity.push(ch);
            } else {
                entity.clear();
                in_entity = false;
            }
            continue;
        }
        match ch {
            '<' => in_tag = true,
            '&' => in_entity = true,
            _ => out.push(ch),
        }
    }

    collapse_spaces(&out)
}

fn html_fragment(html: &str) -> Option<&str> {
    let start = read_html_offset(html, "StartFragment:")?;
    let end = read_html_offset(html, "EndFragment:")?;
    if start < end && end <= html.len() {
        Some(&html[start..end])
    } else {
        None
    }
}

fn read_html_offset(html: &str, label: &str) -> Option<usize> {
    let start = html.find(label)? + label.len();
    let rest = &html[start..];
    let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn rtf_to_preview_text(rtf: &str) -> String {
    let mut out = String::new();
    let mut chars = rtf.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let mut word = String::new();
                while let Some(next) = chars.peek() {
                    if next.is_ascii_alphabetic() {
                        word.push(*next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if word == "par" || word == "line" {
                    out.push(' ');
                }
                while let Some(next) = chars.peek() {
                    if next.is_ascii_digit() || *next == '-' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                if matches!(chars.peek(), Some(' ')) {
                    chars.next();
                }
            }
            '{' | '}' => {}
            '\r' | '\n' => out.push(' '),
            _ => out.push(ch),
        }
    }
    collapse_spaces(&out)
}

fn files_to_preview_text(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 20 {
        return None;
    }
    let offset = u32::from_le_bytes(bytes[0..4].try_into().ok()?) as usize;
    let wide = u32::from_le_bytes(bytes[16..20].try_into().ok()?) != 0;
    if offset >= bytes.len() {
        return None;
    }

    let names = if wide {
        parse_wide_file_list(&bytes[offset..])
    } else {
        parse_ansi_file_list(&bytes[offset..])
    };
    if names.is_empty() {
        return None;
    }

    let shown: Vec<&str> = names.iter().take(2).map(String::as_str).collect();
    let suffix = if names.len() > shown.len() {
        format!(" +{} more", names.len() - shown.len())
    } else {
        String::new()
    };
    Some(format!(
        "{} file{}: {}{}",
        names.len(),
        if names.len() == 1 { "" } else { "s" },
        shown.join(", "),
        suffix
    ))
}

fn parse_wide_file_list(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = Vec::new();
    for chunk in bytes.chunks_exact(2) {
        let value = u16::from_le_bytes([chunk[0], chunk[1]]);
        if value == 0 {
            if current.is_empty() {
                break;
            }
            names.push(file_name_only(&String::from_utf16_lossy(&current)));
            current.clear();
        } else {
            current.push(value);
        }
    }
    names
}

fn parse_ansi_file_list(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for part in bytes.split(|b| *b == 0) {
        if part.is_empty() {
            break;
        }
        names.push(file_name_only(&String::from_utf8_lossy(part)));
    }
    names
}

fn file_name_only(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

fn collapse_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inline(format: u32, bytes: Vec<u8>) -> ClipboardFormatPayload {
        ClipboardFormatPayload {
            format,
            storage: PayloadStorage::Inline(bytes),
        }
    }

    fn utf16z(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn wide_file_list(names: &[&str]) -> Vec<u8> {
        let mut bytes = vec![0u8; 20];
        bytes[0..4].copy_from_slice(&20u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        for name in names {
            bytes.extend(name.encode_utf16().flat_map(u16::to_le_bytes));
            bytes.extend(0u16.to_le_bytes());
        }
        bytes.extend(0u16.to_le_bytes());
        bytes
    }

    #[test]
    fn utf16_decoder_stops_at_nul() {
        let mut bytes = utf16z("hello");
        bytes.extend(utf16z("ignored"));

        assert_eq!(try_decode_utf16z(&bytes), Some("hello".to_string()));
    }

    #[test]
    fn normalize_text_unifies_newlines_and_trims_clipboard_padding() {
        assert_eq!(normalize_text("one\r\ntwo\0 \t\r\n"), "one\ntwo");
    }

    #[test]
    fn make_preview_uses_first_line_and_empty_marker() {
        assert_eq!(make_preview("first\nsecond"), "first");
        assert_eq!(make_preview(""), "<empty>");
    }

    #[test]
    fn html_preview_strips_tags_and_decodes_common_entities() {
        assert_eq!(
            html_to_preview_text("<p>Hello&nbsp;<b>world</b> &amp; friends</p>"),
            "Hello world & friends"
        );
    }

    #[test]
    fn rtf_preview_strips_control_words() {
        assert_eq!(
            rtf_to_preview_text(r"{\rtf1\ansi Hello\par bold text}"),
            "Hello bold text"
        );
    }

    #[test]
    fn file_preview_summarizes_file_drop() {
        assert_eq!(
            files_to_preview_text(&wide_file_list(&["a.txt", "b.png", "c.pdf"])),
            Some("3 files: a.txt, b.png +1 more".to_string())
        );
    }

    #[test]
    fn pick_preview_prefers_unicode_text_over_rich_formats() {
        let formats = vec![
            inline(1000, b"<b>HTML</b>".to_vec()),
            inline(CF_UNICODETEXT, utf16z("plain text")),
        ];

        assert_eq!(pick_preview(&formats, 1000, 1001), "plain text");
    }

    #[test]
    fn fingerprint_normalizes_unicode_text() {
        let a = vec![inline(CF_UNICODETEXT, utf16z("same\r\ntext\n"))];
        let b = vec![inline(CF_UNICODETEXT, utf16z("same\ntext"))];

        assert_eq!(pick_fingerprint(&a, 0), pick_fingerprint(&b, 0));
    }

    #[test]
    fn fallback_fingerprint_includes_format_boundaries() {
        let a = vec![inline(2000, b"abc".to_vec())];
        let b = vec![inline(2001, b"abc".to_vec())];

        assert_ne!(pick_fingerprint(&a, 0), pick_fingerprint(&b, 0));
    }
}
