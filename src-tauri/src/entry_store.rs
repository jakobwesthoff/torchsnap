// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Entry Store
//
// Holds the `ScoredEntry` values from the most recent search
// cycle so `GadgetHost::execute()` can look up the full entry
// by `(source, entry_id)` and pass it to the gadget.
//
// Write path: `GadgetHost::search()` starts a generation with
// `begin_search()` and inserts each gadget's results as they
// arrive. Every keystroke starts a new `search()` without
// cancelling the previous one, so results of an older search
// can still arrive after a newer one has begun. Each insert
// carries the generation it was produced for, and the store
// drops inserts from any generation but the current one.
//
// Read path: `GadgetHost::execute()` on a `spawn_blocking`
// thread looks up the entry to pass to the gadget.
//
// `RwLock` matches this single-writer / rare-reader pattern
// without pulling in a concurrent map dependency. The
// generation lives under the same lock as the map, so the
// check and the insert cannot interleave with a new search.
// =========================================================

use std::collections::HashMap;
use std::sync::RwLock;

use crate::commands::types::ScoredEntry;

/// Identifies one `search()` call. Returned by
/// [`EntryStore::begin_search`] and passed back with every
/// insert for that search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchGeneration(u64);

struct Inner {
    generation: SearchGeneration,
    entries: HashMap<(String, String), ScoredEntry>,
}

pub struct EntryStore {
    inner: RwLock<Inner>,
}

impl EntryStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                generation: SearchGeneration(0),
                entries: HashMap::new(),
            }),
        }
    }

    /// Start a new search: clear all entries and make the
    /// returned generation the only one whose inserts are kept.
    pub fn begin_search(&self) -> SearchGeneration {
        let mut inner = self.inner.write().expect("entry store not poisoned");
        inner.generation = SearchGeneration(inner.generation.0 + 1);
        inner.entries.clear();
        inner.generation
    }

    /// Insert entries for a given source, produced by the search
    /// of `generation`. Dropped when a newer search has started.
    pub fn insert(&self, generation: SearchGeneration, source: &str, entries: &[ScoredEntry]) {
        let mut inner = self.inner.write().expect("entry store not poisoned");
        if inner.generation != generation {
            return;
        }
        for entry in entries {
            inner
                .entries
                .insert((source.to_string(), entry.id.clone()), entry.clone());
        }
    }

    /// Look up an entry by `(source, entry_id)`. Returns a
    /// clone — the caller owns the returned value and the
    /// store lock is released immediately.
    pub fn get(&self, source: &str, entry_id: &str) -> Option<ScoredEntry> {
        let inner = self.inner.read().expect("entry store not poisoned");
        inner
            .entries
            .get(&(source.to_string(), entry_id.to_string()))
            .cloned()
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
        let generation = store.begin_search();
        store.insert(generation, "gadget-a", &[entry("e1", 100), entry("e2", 50)]);
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
        let generation = store.begin_search();
        store.insert(generation, "gadget-a", &[entry("e1", 100)]);
        assert!(store.get("gadget-a", "missing").is_none());
    }

    #[test]
    fn get_returns_none_for_missing_source() {
        let store = EntryStore::new();
        let generation = store.begin_search();
        store.insert(generation, "gadget-a", &[entry("e1", 100)]);
        assert!(store.get("other-gadget", "e1").is_none());
    }

    #[test]
    fn begin_search_empties_the_store() {
        let store = EntryStore::new();
        let generation = store.begin_search();
        store.insert(generation, "gadget-a", &[entry("e1", 100)]);
        store.begin_search();
        assert!(store.get("gadget-a", "e1").is_none());
    }

    #[test]
    fn begin_search_returns_a_new_generation_each_time() {
        let store = EntryStore::new();
        let first = store.begin_search();
        let second = store.begin_search();
        assert_ne!(first, second);
    }

    #[test]
    fn insert_for_an_older_generation_is_dropped() {
        let store = EntryStore::new();
        let old = store.begin_search();
        store.begin_search();
        store.insert(old, "gadget-a", &[entry("e1", 100)]);
        assert!(store.get("gadget-a", "e1").is_none());
    }

    // The interleaving of two overlapping searches: S1 starts, S2
    // starts and clears, then a slow gadget's S1 results arrive
    // after S2's. Only S2's entries may remain.
    #[test]
    fn overlapping_searches_keep_only_the_newer_results() {
        let store = EntryStore::new();
        let s1 = store.begin_search();
        store.insert(s1, "catalog", &[entry("old-catalog", 10)]);
        let s2 = store.begin_search();
        store.insert(s2, "gadget-a", &[entry("shared", 200)]);
        store.insert(
            s1,
            "gadget-a",
            &[entry("shared", 100), entry("old-only", 5)],
        );

        let shared = store.get("gadget-a", "shared").expect("s2 entry present");
        assert_eq!(shared.score, 200, "s1 must not overwrite s2's entry");
        assert!(store.get("gadget-a", "old-only").is_none());
        assert!(store.get("catalog", "old-catalog").is_none());
    }

    #[test]
    fn same_id_different_sources_stored_independently() {
        let store = EntryStore::new();
        let generation = store.begin_search();
        store.insert(generation, "gadget-a", &[entry("shared-id", 100)]);
        store.insert(generation, "gadget-b", &[entry("shared-id", 200)]);
        let a = store.get("gadget-a", "shared-id").expect("a present");
        let b = store.get("gadget-b", "shared-id").expect("b present");
        assert_eq!(a.score, 100);
        assert_eq!(b.score, 200);
    }

    #[test]
    fn insert_overwrites_previous_entries_for_same_key() {
        let store = EntryStore::new();
        let generation = store.begin_search();
        store.insert(generation, "gadget-a", &[entry("e1", 100)]);
        store.insert(generation, "gadget-a", &[entry("e1", 999)]);
        let e = store.get("gadget-a", "e1").expect("e1 present");
        assert_eq!(e.score, 999);
    }

    #[test]
    fn data_field_round_trips() {
        let store = EntryStore::new();
        let generation = store.begin_search();
        let mut e = entry("e1", 50);
        e.data = Some(r#"{"url":"https://example.com"}"#.to_string());
        store.insert(generation, "gadget-a", &[e]);
        let retrieved = store.get("gadget-a", "e1").expect("e1 present");
        assert_eq!(
            retrieved.data.as_deref(),
            Some(r#"{"url":"https://example.com"}"#)
        );
    }
}
