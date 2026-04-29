use crate::history::store::Store;
use crate::history::History;
use parking_lot::Mutex;

pub struct AppState {
    pub history: Mutex<History>,
    pub suppress_clipboard_until_unix_ms: Mutex<i64>,
    pub store: Mutex<Store>,
    pub capture_paused: Mutex<bool>,
}

impl AppState {
    pub fn new() -> anyhow::Result<Self> {
        let store = Store::open()?;
        let recent = store.load_recent(50).unwrap_or_default();

        let mut history = History::new(50);
        for item in recent.into_iter().rev() {
            history.add_or_bump_full(item);
        }

        Ok(Self {
            history: Mutex::new(history),
            suppress_clipboard_until_unix_ms: Mutex::new(0),
            store: Mutex::new(store),
            capture_paused: Mutex::new(false),
        })
    }
}
