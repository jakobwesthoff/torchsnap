// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Log Storage
//
// Trait-based log storage with a ring buffer implementation.
// The ring buffer uses a VecDeque with a fixed capacity —
// oldest entries are evicted when full.
//
// The `seq` field on each LogEntry is monotonically increasing,
// which enables O(log n) binary search in `entries_after`.
// =========================================================

use std::collections::VecDeque;

use super::{LogEntry, DEFAULT_RING_BUFFER_CAPACITY};

// =========================================================
// LogStorage Trait
// =========================================================

/// Abstraction over log entry storage backends.
///
/// Implementations must maintain entries in `seq` order
/// (ascending). The `seq` field is assigned before `push`
/// is called.
pub trait LogStorage: Send + Sync {
    /// Append an entry. If at capacity, the oldest entry is
    /// evicted.
    fn push(&mut self, entry: LogEntry);

    /// Return entries with `seq > after_seq`, up to `limit`.
    fn entries_after(&self, after_seq: u64, limit: usize) -> Vec<LogEntry>;

    /// Return the most recent `count` entries.
    fn tail(&self, count: usize) -> Vec<LogEntry>;

    /// Remove all stored entries.
    fn clear(&mut self);

    /// Number of entries currently stored.
    fn len(&self) -> usize;

    /// Whether the storage is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total number of entries ever pushed, including those
    /// that have been evicted.
    fn total_pushed(&self) -> u64;
}

// =========================================================
// Ring Buffer Storage
// =========================================================

/// Fixed-capacity circular buffer backed by `VecDeque`.
///
/// When the buffer is full, the oldest entry is evicted on
/// each push. All entries are kept in `seq` order, enabling
/// binary search for `entries_after`.
pub struct RingBufferStorage {
    buffer: VecDeque<LogEntry>,
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

    /// Binary search for the position of the first entry with
    /// `seq > target_seq`. Returns the index to start reading
    /// from, or `buffer.len()` if no such entry exists.
    fn search_after(&self, target_seq: u64) -> usize {
        // The seq values are monotonically increasing, so we
        // can use partition_point (binary search) to find the
        // first entry exceeding the target.
        self.buffer.partition_point(|entry| entry.seq <= target_seq)
    }
}

impl Default for RingBufferStorage {
    fn default() -> Self {
        Self::new(DEFAULT_RING_BUFFER_CAPACITY)
    }
}

impl LogStorage for RingBufferStorage {
    fn push(&mut self, entry: LogEntry) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(entry);
        self.total_pushed += 1;
    }

    fn entries_after(&self, after_seq: u64, limit: usize) -> Vec<LogEntry> {
        let start = self.search_after(after_seq);
        self.buffer
            .iter()
            .skip(start)
            .take(limit)
            .cloned()
            .collect()
    }

    fn tail(&self, count: usize) -> Vec<LogEntry> {
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
    use crate::wasm::logging::{LogLevel, LogSource};

    /// Helper to create a log entry with a given seq number.
    fn entry(seq: u64) -> LogEntry {
        LogEntry {
            seq,
            timestamp: SystemTime::now(),
            level: LogLevel::Info,
            source: LogSource::Host,
            message: format!("msg-{seq}"),
            metadata: vec![],
            span_id: None,
            span: None,
        }
    }

    #[test]
    fn push_and_len() {
        let mut storage = RingBufferStorage::new(5);
        assert!(storage.is_empty());

        storage.push(entry(1));
        storage.push(entry(2));
        assert_eq!(storage.len(), 2);
        assert_eq!(storage.total_pushed(), 2);
    }

    #[test]
    fn eviction_at_capacity() {
        let mut storage = RingBufferStorage::new(3);

        storage.push(entry(1));
        storage.push(entry(2));
        storage.push(entry(3));
        assert_eq!(storage.len(), 3);

        // Pushing a 4th entry evicts the oldest (seq=1).
        storage.push(entry(4));
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
            storage.push(entry(i));
        }

        // Everything after seq 0 — should return all entries.
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
            storage.push(entry(i));
        }

        let result = storage.entries_after(0, 2);
        let seqs: Vec<u64> = result.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2]);
    }

    #[test]
    fn entries_after_with_eviction() {
        let mut storage = RingBufferStorage::new(3);
        for i in 1..=5 {
            storage.push(entry(i));
        }

        // Buffer contains [3, 4, 5]. Asking for entries after
        // seq 1 (which was evicted) should return everything
        // still in the buffer.
        let result = storage.entries_after(1, 100);
        let seqs: Vec<u64> = result.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![3, 4, 5]);

        // Asking for entries after seq 3 should return [4, 5].
        let result = storage.entries_after(3, 100);
        let seqs: Vec<u64> = result.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![4, 5]);
    }

    #[test]
    fn tail_returns_most_recent() {
        let mut storage = RingBufferStorage::new(10);
        for i in 1..=5 {
            storage.push(entry(i));
        }

        let result = storage.tail(3);
        let seqs: Vec<u64> = result.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![3, 4, 5]);
    }

    #[test]
    fn tail_more_than_available() {
        let mut storage = RingBufferStorage::new(10);
        storage.push(entry(1));
        storage.push(entry(2));

        let result = storage.tail(100);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn clear_empties_buffer() {
        let mut storage = RingBufferStorage::new(10);
        for i in 1..=5 {
            storage.push(entry(i));
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
