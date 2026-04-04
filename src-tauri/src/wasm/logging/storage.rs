// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Log Storage
//
// Trait-based log storage with a ring buffer implementation.
// The ring buffer uses a VecDeque with a fixed capacity —
// oldest items are evicted when full.
//
// The `seq` field on each LogItem is monotonically increasing,
// which enables O(log n) binary search in `entries_after`.
// =========================================================

use std::collections::VecDeque;

use super::{LogItem, DEFAULT_RING_BUFFER_CAPACITY};

// =========================================================
// LogStorage Trait
// =========================================================

/// Abstraction over log item storage backends.
///
/// Implementations must maintain items in `seq` order
/// (ascending). The `seq` field is assigned before `push`
/// is called.
pub trait LogStorage: Send + Sync {
    /// Append an item. If at capacity, the oldest item is
    /// evicted.
    fn push(&mut self, item: LogItem);

    /// Return items with `seq > after_seq`, up to `limit`.
    fn entries_after(&self, after_seq: u64, limit: usize) -> Vec<LogItem>;

    /// Return the most recent `count` items.
    fn tail(&self, count: usize) -> Vec<LogItem>;

    /// Remove all stored items.
    fn clear(&mut self);

    /// Number of items currently stored.
    fn len(&self) -> usize;

    /// Whether the storage is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total number of items ever pushed, including those
    /// that have been evicted.
    fn total_pushed(&self) -> u64;
}

// =========================================================
// Ring Buffer Storage
// =========================================================

/// Fixed-capacity circular buffer backed by `VecDeque`.
///
/// When the buffer is full, the oldest item is evicted on
/// each push. All items are kept in `seq` order, enabling
/// binary search for `entries_after`.
pub struct RingBufferStorage {
    buffer: VecDeque<LogItem>,
    capacity: usize,
    total_pushed: u64,
}

impl RingBufferStorage {
    /// Create a new ring buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
            total_pushed: 0,
        }
    }

    /// Binary search for the position of the first item with
    /// `seq > target_seq`. Returns the index to start reading
    /// from, or `buffer.len()` if no such item exists.
    fn search_after(&self, target_seq: u64) -> usize {
        // The seq values are monotonically increasing, so we
        // can use partition_point (binary search) to find the
        // first item exceeding the target.
        self.buffer.partition_point(|item| item.seq <= target_seq)
    }
}

impl Default for RingBufferStorage {
    fn default() -> Self {
        Self::new(DEFAULT_RING_BUFFER_CAPACITY)
    }
}

impl LogStorage for RingBufferStorage {
    fn push(&mut self, item: LogItem) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
        self.total_pushed += 1;
    }

    fn entries_after(&self, after_seq: u64, limit: usize) -> Vec<LogItem> {
        let start = self.search_after(after_seq);
        self.buffer
            .iter()
            .skip(start)
            .take(limit)
            .cloned()
            .collect()
    }

    fn tail(&self, count: usize) -> Vec<LogItem> {
        let skip = self.buffer.len().saturating_sub(count);
        self.buffer.iter().skip(skip).cloned().collect()
    }

    fn clear(&mut self) {
        self.buffer.clear();
    }

    fn len(&self) -> usize {
        self.buffer.len()
    }

    fn total_pushed(&self) -> u64 {
        self.total_pushed
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::wasm::logging::{LogItemKind, LogLevel, LogSource};

    /// Helper to create a log item with a given seq number.
    fn item(seq: u64) -> LogItem {
        LogItem {
            seq,
            timestamp: SystemTime::now(),
            source: LogSource::Host,
            kind: LogItemKind::Message {
                level: LogLevel::Info,
                message: format!("msg-{seq}"),
                metadata: vec![],
                span_id: None,
            },
        }
    }

    #[test]
    fn push_and_len() {
        let mut storage = RingBufferStorage::new(5);
        assert!(storage.is_empty());

        storage.push(item(1));
        storage.push(item(2));
        assert_eq!(storage.len(), 2);
        assert_eq!(storage.total_pushed(), 2);
    }

    #[test]
    fn eviction_at_capacity() {
        let mut storage = RingBufferStorage::new(3);

        storage.push(item(1));
        storage.push(item(2));
        storage.push(item(3));
        assert_eq!(storage.len(), 3);

        // Pushing a 4th item evicts the oldest (seq=1).
        storage.push(item(4));
        assert_eq!(storage.len(), 3);
        assert_eq!(storage.total_pushed(), 4);

        let all = storage.tail(10);
        let seqs: Vec<u64> = all.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![2, 3, 4]);
    }

    #[test]
    fn entries_after_basic() {
        let mut storage = RingBufferStorage::new(10);
        for i in 1..=5 {
            storage.push(item(i));
        }

        // Everything after seq 0 — should return all items.
        let result = storage.entries_after(0, 100);
        assert_eq!(result.len(), 5);

        // Everything after seq 3 — should return seq 4 and 5.
        let result = storage.entries_after(3, 100);
        let seqs: Vec<u64> = result.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![4, 5]);

        // Everything after seq 5 — should return nothing.
        let result = storage.entries_after(5, 100);
        assert!(result.is_empty());
    }

    #[test]
    fn entries_after_with_limit() {
        let mut storage = RingBufferStorage::new(10);
        for i in 1..=5 {
            storage.push(item(i));
        }

        let result = storage.entries_after(0, 2);
        let seqs: Vec<u64> = result.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2]);
    }

    #[test]
    fn entries_after_with_eviction() {
        let mut storage = RingBufferStorage::new(3);
        for i in 1..=5 {
            storage.push(item(i));
        }

        // Buffer contains [3, 4, 5]. Asking for items after
        // seq 1 (which was evicted) should return everything
        // still in the buffer.
        let result = storage.entries_after(1, 100);
        let seqs: Vec<u64> = result.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![3, 4, 5]);

        // Asking for items after seq 3 should return [4, 5].
        let result = storage.entries_after(3, 100);
        let seqs: Vec<u64> = result.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![4, 5]);
    }

    #[test]
    fn tail_returns_most_recent() {
        let mut storage = RingBufferStorage::new(10);
        for i in 1..=5 {
            storage.push(item(i));
        }

        let result = storage.tail(3);
        let seqs: Vec<u64> = result.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![3, 4, 5]);
    }

    #[test]
    fn tail_more_than_available() {
        let mut storage = RingBufferStorage::new(10);
        storage.push(item(1));
        storage.push(item(2));

        let result = storage.tail(100);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn clear_empties_buffer() {
        let mut storage = RingBufferStorage::new(10);
        for i in 1..=5 {
            storage.push(item(i));
        }

        storage.clear();
        assert!(storage.is_empty());
        // total_pushed is not reset by clear — it's a lifetime
        // counter for dropped-message accounting.
        assert_eq!(storage.total_pushed(), 5);
    }

    #[test]
    fn default_capacity() {
        let storage = RingBufferStorage::default();
        assert_eq!(storage.capacity, DEFAULT_RING_BUFFER_CAPACITY);
    }
}
