// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Entry Store
//
// Holds the `ScoredEntry` values from the most recent search
// cycle so `GadgetHost::execute()` can look up the full entry
// by `(source, entry_id)` and pass it to the gadget. Cleared
// atomically at the start of each new `search()` call.
//
// Write path: the async `GadgetHost::search()` method inserts
// entries after each gadget's results arrive — one writer at
// a time from the search task.
//
// Read path: `GadgetHost::execute()` on a `spawn_blocking`
// thread looks up the entry to pass to the gadget.
//
// `RwLock<HashMap>` matches this single-writer / rare-reader
// pattern without pulling in a concurrent map dependency.
// =========================================================

use std::collections::HashMap;
use std::sync::RwLock;

use crate::commands::types::ScoredEntry;

pub struct EntryStore {
    inner: RwLock<HashMap<(String, String), ScoredEntry>>,
}

impl EntryStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Clear all entries. Called at the top of every `search()`
    /// so stale entries from the previous search do not persist.
    pub fn clear(&self) {
        let mut map = self.inner.write().expect("entry store not poisoned");
        map.clear();
    }

    /// Insert entries for a given source. Called after each
    /// gadget's search results arrive.
    pub fn insert(&self, source: &str, entries: &[ScoredEntry]) {
        let mut map = self.inner.write().expect("entry store not poisoned");
        for entry in entries {
            map.insert(
                (source.to_string(), entry.id.clone()),
                entry.clone(),
            );
        }
    }

    /// Look up an entry by `(source, entry_id)`. Returns a
    /// clone — the caller owns the returned value and the
    /// store lock is released immediately.
    pub fn get(&self, source: &str, entry_id: &str) -> Option<ScoredEntry> {
        let map = self.inner.read().expect("entry store not poisoned");
        map.get(&(source.to_string(), entry_id.to_string())).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unicode::Utf16Positions;

    fn entry(id: &str, score: u32) -> ScoredEntry {
        ScoredEntry {
            id: id.to_string(),
            title: id.to_string(),
            subtitle: None,
            icon: None,
            score,
            title_positions: Utf16Positions::empty(),
            subtitle_positions: Utf16Positions::empty(),
            actions: vec![],
            data: None,
        }
    }

    #[test]
    fn insert_and_get_round_trip() {
        let store = EntryStore::new();
        store.insert("gadget-a", &[entry("e1", 100), entry("e2", 50)]);
        let e1 = store.get("gadget-a", "e1").expect("e1 present");
        assert_eq!(e1.id, "e1");
        assert_eq!(e1.score, 100);
        let e2 = store.get("gadget-a", "e2").expect("e2 present");
        assert_eq!(e2.id, "e2");
        assert_eq!(e2.score, 50);
    }

    #[test]
    fn get_returns_none_for_missing_entry() {
        let store = EntryStore::new();
        store.insert("gadget-a", &[entry("e1", 100)]);
        assert!(store.get("gadget-a", "missing").is_none());
    }

    #[test]
    fn get_returns_none_for_missing_source() {
        let store = EntryStore::new();
        store.insert("gadget-a", &[entry("e1", 100)]);
        assert!(store.get("other-gadget", "e1").is_none());
    }

    #[test]
    fn clear_empties_the_store() {
        let store = EntryStore::new();
        store.insert("gadget-a", &[entry("e1", 100)]);
        store.clear();
        assert!(store.get("gadget-a", "e1").is_none());
    }

    #[test]
    fn same_id_different_sources_stored_independently() {
        let store = EntryStore::new();
        store.insert("gadget-a", &[entry("shared-id", 100)]);
        store.insert("gadget-b", &[entry("shared-id", 200)]);
        let a = store.get("gadget-a", "shared-id").expect("a present");
        let b = store.get("gadget-b", "shared-id").expect("b present");
        assert_eq!(a.score, 100);
        assert_eq!(b.score, 200);
    }

    #[test]
    fn insert_overwrites_previous_entries_for_same_key() {
        let store = EntryStore::new();
        store.insert("gadget-a", &[entry("e1", 100)]);
        store.insert("gadget-a", &[entry("e1", 999)]);
        let e = store.get("gadget-a", "e1").expect("e1 present");
        assert_eq!(e.score, 999);
    }

    #[test]
    fn data_field_round_trips() {
        let store = EntryStore::new();
        let mut e = entry("e1", 50);
        e.data = Some(r#"{"url":"https://example.com"}"#.to_string());
        store.insert("gadget-a", &[e]);
        let retrieved = store.get("gadget-a", "e1").expect("e1 present");
        assert_eq!(
            retrieved.data.as_deref(),
            Some(r#"{"url":"https://example.com"}"#)
        );
    }
}
