use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PayloadStorage {
    Inline(Vec<u8>),
    File { rel_path: String, size: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardFormatPayload {
    pub format: u32,
    pub storage: PayloadStorage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: u64,
    pub created_unix_ms: i64,
    pub fingerprint: [u8; 32],
    pub preview: String,
    pub formats: Vec<ClipboardFormatPayload>,
}
