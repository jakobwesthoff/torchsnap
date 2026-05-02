// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Coalescing Dispatcher
//
// Serializes and deduplicates settings change dispatch for a
// single plugin. The host enqueues `(key, value)` pairs as
// settings change events arrive; the dispatcher ensures:
//
// 1. Only one callback runs at a time (serialization via
//    `work` mutex with `try_lock`)
// 2. Rapid changes to the same key are coalesced — only the
//    latest value is dispatched (dedup via retain + push)
// 3. Changes to different keys preserve chronological order
// 4. Changes that arrive while a callback is running are
//    picked up in the next loop iteration
//
// Threading model:
//
// - `enqueue()` is called from the Tauri event listener
//   (main thread). It only locks `pending` briefly.
// - `dispatch()` is called from a `spawn_blocking` thread.
//   It holds `work` for the duration of the callback but
//   only locks `pending` to drain the queue.
// - The two mutexes never nest: `pending` is released before
//   `work` calls the callback, so no deadlock is possible.
// =========================================================

use std::sync::Mutex;

use serde_json::Value;

pub struct CoalescingDispatcher {
    /// Serializes dispatch execution. Held for the duration of
    /// callback processing. `try_lock` ensures concurrent
    /// `dispatch()` calls return immediately instead of blocking.
    work: Mutex<()>,

    /// Queued `(key, value)` pairs awaiting dispatch. When a key
    /// is enqueued that already exists, the old entry is removed
    /// and the new one is appended to the back — preserving
    /// chronological order while deduplicating same-key changes.
    pending: Mutex<Vec<(String, Value)>>,
}

impl CoalescingDispatcher {
    pub fn new() -> Self {
        Self {
            work: Mutex::new(()),
            pending: Mutex::new(Vec::new()),
        }
    }

    /// Add a settings change to the pending queue. If an entry
    /// for the same key already exists, it is replaced and moved
    /// to the back of the queue (reflecting chronological order).
    ///
    /// This method is cheap and non-blocking with respect to the
    /// dispatch callback — it only briefly locks the `pending`
    /// mutex.
    pub fn enqueue(&self, key: String, value: Value) {
        let mut pending = self.pending.lock().expect("pending not poisoned");
        pending.retain(|(k, _)| k != &key);
        pending.push((key, value));
    }

    /// Process all pending changes by calling `callback` for each
    /// `(key, value)` pair. If another `dispatch()` call is already
    /// running, this returns immediately — the active dispatch loop
    /// will pick up any newly enqueued items.
    ///
    /// The dispatch loop drains the pending queue, processes each
    /// entry, then checks again for items that arrived during
    /// processing. This repeats until the queue is empty.
    pub fn dispatch(&self, callback: impl Fn(&str, &Value)) {
        // Non-blocking: if another dispatch is already running,
        // it will pick up our enqueued items in its next iteration.
        let Ok(_guard) = self.work.try_lock() else {
            return;
        };

        loop {
            let batch = {
                let mut pending = self.pending.lock().expect("pending not poisoned");
                std::mem::take(&mut *pending)
            };

            if batch.is_empty() {
                break;
            }

            for (key, value) in &batch {
                callback(key, value);
            }
        }
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    // =====================================================
    // Helpers
    // =====================================================

    /// Collect dispatched `(key, value)` pairs into a shared Vec.
    fn collecting_callback(
        collected: &Arc<Mutex<Vec<(String, Value)>>>,
    ) -> impl Fn(&str, &Value) + '_ {
        move |key, value| {
            collected
                .lock()
                .unwrap()
                .push((key.to_string(), value.clone()));
        }
    }

    fn json(v: impl Into<Value>) -> Value {
        v.into()
    }

    // =====================================================
    // Expected cases
    // =====================================================

    #[test]
    fn single_entry_dispatched() {
        let d = CoalescingDispatcher::new();
        let collected = Arc::new(Mutex::new(Vec::new()));

        d.enqueue("key".into(), json(1));
        d.dispatch(collecting_callback(&collected));

        let result = collected.lock().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ("key".into(), json(1)));
    }

    #[test]
    fn multiple_entries_dispatched_in_order() {
        let d = CoalescingDispatcher::new();
        let collected = Arc::new(Mutex::new(Vec::new()));

        d.enqueue("a".into(), json(1));
        d.enqueue("b".into(), json(2));
        d.enqueue("c".into(), json(3));
        d.dispatch(collecting_callback(&collected));

        let result = collected.lock().unwrap();
        let keys: Vec<&str> = result.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["a", "b", "c"]);
    }

    #[test]
    fn same_key_deduplicates() {
        let d = CoalescingDispatcher::new();
        let collected = Arc::new(Mutex::new(Vec::new()));

        d.enqueue("k".into(), json(1));
        d.enqueue("k".into(), json(2));
        d.dispatch(collecting_callback(&collected));

        let result = collected.lock().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ("k".into(), json(2)));
    }

    #[test]
    fn same_key_moves_to_end() {
        let d = CoalescingDispatcher::new();
        let collected = Arc::new(Mutex::new(Vec::new()));

        d.enqueue("a".into(), json(1));
        d.enqueue("b".into(), json(2));
        d.enqueue("a".into(), json(3));
        d.dispatch(collecting_callback(&collected));

        let result = collected.lock().unwrap();
        let keys: Vec<&str> = result.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["b", "a"]);
        assert_eq!(result[1].1, json(3));
    }

    #[test]
    fn empty_dispatch_is_noop() {
        let d = CoalescingDispatcher::new();
        let called = Arc::new(Mutex::new(false));

        d.dispatch({
            let called = Arc::clone(&called);
            move |_, _| {
                *called.lock().unwrap() = true;
            }
        });

        assert!(!*called.lock().unwrap());
    }

    #[test]
    fn pending_during_dispatch_picked_up() {
        // The callback enqueues a new item during processing.
        // The dispatch loop should pick it up in the next iteration.
        let d = Arc::new(CoalescingDispatcher::new());
        let collected = Arc::new(Mutex::new(Vec::new()));

        d.enqueue("first".into(), json(1));

        let d_inner = Arc::clone(&d);
        let collected_inner = Arc::clone(&collected);
        let enqueued = Arc::new(Mutex::new(false));
        let enqueued_inner = Arc::clone(&enqueued);

        d.dispatch(move |key, value| {
            collected_inner
                .lock()
                .unwrap()
                .push((key.to_string(), value.clone()));

            // Enqueue a second item during the first callback,
            // but only once to avoid infinite loop.
            let mut flag = enqueued_inner.lock().unwrap();
            if !*flag {
                *flag = true;
                d_inner.enqueue("second".into(), json(2));
            }
        });

        let result = collected.lock().unwrap();
        let keys: Vec<&str> = result.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["first", "second"]);
    }

    // =====================================================
    // Edge cases
    // =====================================================

    #[test]
    fn concurrent_enqueue_during_dispatch() {
        // A background thread enqueues items while the dispatch
        // callback is running. The dispatch loop should pick them
        // up after the current batch completes.
        let d = Arc::new(CoalescingDispatcher::new());
        let collected = Arc::new(Mutex::new(Vec::new()));

        d.enqueue("initial".into(), json(0));

        let d_for_thread = Arc::clone(&d);
        let enqueued = Arc::new(Mutex::new(false));
        let enqueued_inner = Arc::clone(&enqueued);

        let collected_inner = Arc::clone(&collected);
        d.dispatch(move |key, value| {
            collected_inner
                .lock()
                .unwrap()
                .push((key.to_string(), value.clone()));

            // On the first callback, spawn a thread that enqueues
            // and give it time to complete.
            let mut flag = enqueued_inner.lock().unwrap();
            if !*flag {
                *flag = true;
                let d = Arc::clone(&d_for_thread);
                std::thread::spawn(move || {
                    d.enqueue("from-thread".into(), json(42));
                })
                .join()
                .unwrap();
            }
        });

        let result = collected.lock().unwrap();
        let keys: Vec<&str> = result.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["initial", "from-thread"]);
    }

    #[test]
    fn rapid_same_key_only_latest_survives() {
        let d = CoalescingDispatcher::new();
        let collected = Arc::new(Mutex::new(Vec::new()));

        for i in 0..100 {
            d.enqueue("rapid".into(), json(i));
        }
        d.dispatch(collecting_callback(&collected));

        let result = collected.lock().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ("rapid".into(), json(99)));
    }

    #[test]
    fn dispatch_while_dispatch_running_returns() {
        // Two threads call dispatch. The second should return
        // immediately (try_lock fails). The first processes
        // everything.
        let d = Arc::new(CoalescingDispatcher::new());
        let call_count = Arc::new(Mutex::new(0u32));

        d.enqueue("item".into(), json(1));

        let d1 = Arc::clone(&d);
        let d2 = Arc::clone(&d);
        let count1 = Arc::clone(&call_count);
        let count2 = Arc::clone(&call_count);

        // Thread 1: dispatch with a slow callback
        let t1 = std::thread::spawn(move || {
            d1.dispatch(move |_, _| {
                *count1.lock().unwrap() += 1;
                std::thread::sleep(Duration::from_millis(50));
            });
        });

        // Give thread 1 time to acquire the work mutex
        std::thread::sleep(Duration::from_millis(10));

        // Thread 2: dispatch should return immediately
        let t2 = std::thread::spawn(move || {
            d2.dispatch(move |_, _| {
                *count2.lock().unwrap() += 1;
            });
        });

        t1.join().unwrap();
        t2.join().unwrap();

        // Only thread 1's callback should have fired
        assert_eq!(*call_count.lock().unwrap(), 1);
    }

    #[test]
    fn interleaved_keys_preserve_order() {
        let d = CoalescingDispatcher::new();
        let collected = Arc::new(Mutex::new(Vec::new()));

        d.enqueue("a".into(), json(1));
        d.enqueue("b".into(), json(2));
        d.enqueue("c".into(), json(3));
        d.enqueue("b".into(), json(4));
        d.enqueue("a".into(), json(5));
        d.dispatch(collecting_callback(&collected));

        let result = collected.lock().unwrap();
        let entries: Vec<(&str, i64)> = result
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_i64().unwrap()))
            .collect();
        // c stays in place, b moved after c, a moved to end
        assert_eq!(entries, vec![("c", 3), ("b", 4), ("a", 5)]);
    }

    #[test]
    fn dispatch_after_drain_is_empty() {
        let d = CoalescingDispatcher::new();
        let collected = Arc::new(Mutex::new(Vec::new()));

        d.enqueue("x".into(), json(1));
        d.dispatch(collecting_callback(&collected));
        assert_eq!(collected.lock().unwrap().len(), 1);

        // Second dispatch with nothing new should be a no-op
        collected.lock().unwrap().clear();
        d.dispatch(collecting_callback(&collected));
        assert!(collected.lock().unwrap().is_empty());
    }
}
