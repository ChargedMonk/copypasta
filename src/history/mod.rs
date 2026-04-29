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

    pub fn remove_by_id(&mut self, id: u64) -> bool {
        let Some(idx) = self.items.iter().position(|it| it.id == id) else {
            return false;
        };
        self.items.remove(idx);
        true
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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u64) -> ClipboardItem {
        ClipboardItem {
            id,
            created_unix_ms: id as i64,
            fingerprint: [id as u8; 32],
            preview: format!("item {id}"),
            formats: Vec::new(),
        }
    }

    #[test]
    fn remove_by_id_removes_matching_item_only() {
        let mut history = History::new(10);
        history.add_or_bump_full(item(1));
        history.add_or_bump_full(item(2));
        history.add_or_bump_full(item(3));

        assert!(history.remove_by_id(2));

        let ids: Vec<u64> = history.items().map(|item| item.id).collect();
        assert_eq!(ids, vec![3, 1]);
    }

    #[test]
    fn remove_by_id_reports_missing_item() {
        let mut history = History::new(10);
        history.add_or_bump_full(item(1));

        assert!(!history.remove_by_id(99));
        assert_eq!(history.len(), 1);
    }
}
