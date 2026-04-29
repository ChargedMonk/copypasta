use crate::clipboard::item::ClipboardItem;
use std::collections::VecDeque;

pub mod store;

pub struct History {
    max_items: usize,
    next_id: u64,
    items: VecDeque<ClipboardItem>,
}

impl History {
    pub fn new(max_items: usize) -> Self {
        Self {
            max_items,
            next_id: 1,
            items: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn items(&self) -> impl Iterator<Item = &ClipboardItem> {
        self.items.iter()
    }

    pub fn bump_by_fingerprint(&mut self, fingerprint: [u8; 32]) -> bool {
        if let Some(existing_idx) = self
            .items
            .iter()
            .position(|it| it.fingerprint == fingerprint)
        {
            if existing_idx != 0 {
                if let Some(item) = self.items.remove(existing_idx) {
                    self.items.push_front(item);
                }
            }
            return true;
        }
        false
    }

    pub fn add_or_bump_full(&mut self, item: ClipboardItem) {
        if self.bump_by_fingerprint(item.fingerprint) {
            return;
        }
        self.next_id = self.next_id.max(item.id.saturating_add(1));
        self.items.push_front(item);
        while self.items.len() > self.max_items {
            self.items.pop_back();
        }
    }
}
