// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Coalescing Dispatcher
//
// Serializes and deduplicates settings change dispatch for a
// single gadget. The host enqueues `(key, value)` pairs as
// settings change events arrive; the dispatcher ensures:
//
// 1. Only one drain runs at a time. A `dispatch()` that finds
//    a drain running returns at once and leaves its changes to
//    that drain.
// 2. Rapid changes to the same key are coalesced: only the
//    latest value is dispatched (dedup via retain + push).
// 3. Changes to different keys preserve chronological order.
// 4. Changes that arrive while a callback is running are
//    picked up in the next loop iteration.
//
// The queue and the `draining` flag live under one mutex, and a
// drain clears `draining` only in the critical section that
// finds the queue empty; nothing after that point touches the
// flag. A change is therefore either in the queue when the drain
// looks, or enqueued after the drain stopped, in which case the
// enqueuing caller's own `dispatch()` starts a new drain. No
// change is left in the queue with no drain to take it.
//
// Threading model:
//
// - Both `enqueue()` and `dispatch()` run on the thread of the
//   caller: the `settings-changed` listener (a tokio worker) or
//   a Retry (a `spawn_blocking` thread). Whichever caller drains
//   runs every queued change on its own thread.
// - The queue lock is never held while a callback runs, so a
//   callback may enqueue.
// =========================================================

use std::sync::Mutex;

use serde_json::Value;

pub struct CoalescingDispatcher<K> {
    queue: Mutex<Queue<K>>,

    #[cfg(test)]
    drained: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

struct Queue<K> {
    /// Queued `(key, value)` pairs awaiting dispatch. When a key
    /// is enqueued that already exists, the old entry is removed
    /// and the new one is appended to the back, preserving
    /// chronological order while deduplicating same-key changes.
    items: Vec<(K, Value)>,

    /// Whether a `dispatch()` is draining the queue.
    draining: bool,
}

impl<K: PartialEq> CoalescingDispatcher<K> {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Queue {
                items: Vec::new(),
                draining: false,
            }),
            #[cfg(test)]
            drained: Mutex::new(None),
        }
    }

    /// The queue lock is held only for bookkeeping, never while a
    /// callback runs, so a panicking callback cannot poison it.
    fn lock_queue(&self) -> std::sync::MutexGuard<'_, Queue<K>> {
        self.queue
            .lock()
            .expect("queue lock is not held while callbacks run")
    }

    /// Run `hook` once, on the draining thread, right after the drain
    /// loop released the queue in the iteration that found it empty.
    /// Tests use it to enqueue at the moment a drain ends.
    #[cfg(test)]
    fn on_drained(&self, hook: impl FnOnce() + Send + 'static) {
        *self.drained.lock().expect("hook not poisoned") = Some(Box::new(hook));
    }

    #[cfg(test)]
    fn run_drained_hook(&self) {
        let hook = self.drained.lock().expect("hook not poisoned").take();
        if let Some(hook) = hook {
            hook();
        }
    }

    /// Add a settings change to the pending queue. If an entry
    /// for the same key already exists, it is replaced and moved
    /// to the back of the queue (reflecting chronological order).
    ///
    /// This method is cheap and does not wait for a running
    /// callback; it only briefly locks the queue.
    pub fn enqueue(&self, key: K, value: Value) {
        let mut queue = self.lock_queue();
        queue.items.retain(|(k, _)| k != &key);
        queue.items.push((key, value));
    }

    /// Process all pending changes by calling `callback` for each
    /// `(key, value)` pair. If another `dispatch()` call is already
    /// draining, this returns immediately and that drain processes
    /// the queued changes with its own callback. Every caller of one
    /// dispatcher must therefore pass a callback that handles every
    /// key the queue can hold.
    ///
    /// The dispatch loop drains the queue, processes each entry,
    /// then checks again for items that arrived during processing.
    /// This repeats until the queue is empty.
    ///
    /// A panicking callback is not caught: `draining` stays set,
    /// every later dispatch returns at once, and the gadget gets no
    /// further changes. Isolating gadget panics is todo
    /// 01KWG1PH0QCDQTARA5JCW7ABYK-keep-a-panicking-gadget-from-aborting-the-whole-search-and-execute.md.
    pub fn dispatch(&self, callback: impl Fn(&K, &Value)) {
        {
            let mut queue = self.lock_queue();
            if queue.draining {
                return;
            }
            queue.draining = true;
        }

        loop {
            let batch = {
                let mut queue = self.lock_queue();
                if queue.items.is_empty() {
                    // Stopping is decided under the same lock that
                    // `enqueue()` takes, so no change can slip in
                    // between the empty check and the flag.
                    queue.draining = false;
                    None
                } else {
                    Some(std::mem::take(&mut queue.items))
                }
            };

            let Some(batch) = batch else {
                #[cfg(test)]
                self.run_drained_hook();
                return;
            };

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
    use std::collections::HashMap;
    use std::panic::AssertUnwindSafe;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier, mpsc};
    use std::time::Duration;

    use super::*;

    // =====================================================
    // Helpers
    // =====================================================

    /// Collect dispatched `(key, value)` pairs into a shared Vec.
    fn collecting_callback(
        collected: &Arc<Mutex<Vec<(String, Value)>>>,
    ) -> impl Fn(&String, &Value) + '_ {
        move |key, value| {
            collected
                .lock()
                .unwrap()
                .push((key.to_string(), value.clone()));
        }
    }

    fn dispatcher() -> CoalescingDispatcher<String> {
        CoalescingDispatcher::new()
    }

    fn json(v: impl Into<Value>) -> Value {
        v.into()
    }

    // =====================================================
    // Expected cases
    // =====================================================

    #[test]
    fn single_entry_dispatched() {
        let d = dispatcher();
        let collected = Arc::new(Mutex::new(Vec::new()));

        d.enqueue("key".into(), json(1));
        d.dispatch(collecting_callback(&collected));

        let result = collected.lock().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ("key".into(), json(1)));
    }

    #[test]
    fn multiple_entries_dispatched_in_order() {
        let d = dispatcher();
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
        let d = dispatcher();
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
        let d = dispatcher();
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
        let d = dispatcher();
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
        let d = Arc::new(dispatcher());
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
        let d = Arc::new(dispatcher());
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
        let d = dispatcher();
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
        let d = Arc::new(dispatcher());
        let collected = Arc::new(Mutex::new(Vec::new()));

        // A drain that already ended on another thread must leave
        // nothing behind that lets a later dispatch run next to the
        // one that is running.
        {
            let d = Arc::clone(&d);
            std::thread::spawn(move || {
                d.enqueue("warm-up".into(), json(0));
                d.dispatch(|_, _| {});
            })
            .join()
            .unwrap();
        }

        // The drainer's first callback announces itself and waits
        // until the test releases it.
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        d.enqueue("first".into(), json(1));
        let drainer = {
            let d = Arc::clone(&d);
            let collected = Arc::clone(&collected);
            std::thread::spawn(move || {
                d.dispatch(|key, value| {
                    collected
                        .lock()
                        .unwrap()
                        .push((key.to_string(), value.clone()));
                    if key == "first" {
                        started_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                    }
                });
            })
        };
        started_rx.recv().unwrap();

        // A second caller enqueues and dispatches while the drainer is
        // held. Its dispatch must return without running its own
        // callback; the timeout turns a blocking dispatch into a
        // failure instead of a hang.
        let (returned_tx, returned_rx) = mpsc::channel();
        let second_ran = Arc::new(AtomicBool::new(false));
        {
            let d = Arc::clone(&d);
            let second_ran = Arc::clone(&second_ran);
            std::thread::spawn(move || {
                d.enqueue("second".into(), json(2));
                d.dispatch(|_, _| second_ran.store(true, Ordering::SeqCst));
                returned_tx.send(()).unwrap();
            });
        }
        returned_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("a second dispatch returns while the first one runs");

        release_tx.send(()).unwrap();
        drainer.join().unwrap();

        assert!(!second_ran.load(Ordering::SeqCst));
        let result = collected.lock().unwrap();
        let keys: Vec<&str> = result.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["first", "second"]);
    }

    #[test]
    fn a_change_enqueued_as_a_drain_ends_is_dispatched() {
        let d = Arc::new(dispatcher());
        let collected = Arc::new(Mutex::new(Vec::new()));
        d.enqueue("first".into(), json(1));

        // At the moment the drain has found the queue empty, another
        // thread enqueues a change and dispatches. The drain has not
        // returned yet, so that dispatch leaves the change to it.
        let other = Arc::clone(&d);
        let other_collected = Arc::clone(&collected);
        d.on_drained(move || {
            std::thread::spawn(move || {
                other.enqueue("late".into(), json(2));
                other.dispatch(collecting_callback(&other_collected));
            })
            .join()
            .unwrap();
        });

        d.dispatch(collecting_callback(&collected));

        let result = collected.lock().unwrap();
        let keys: Vec<&str> = result.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["first", "late"]);
    }

    #[test]
    fn every_last_value_arrives_when_many_threads_enqueue_and_dispatch() {
        const THREADS: usize = 8;
        const KEYS: i64 = 4;
        const WRITES: i64 = 100;

        let d = Arc::new(dispatcher());
        let delivered = Arc::new(Mutex::new(HashMap::<String, Vec<i64>>::new()));
        let start = Arc::new(Barrier::new(THREADS));

        // Each thread owns its keys, so the last value written to a key
        // is known: the thread's last write to it.
        let workers: Vec<_> = (0..THREADS)
            .map(|thread| {
                let d = Arc::clone(&d);
                let delivered = Arc::clone(&delivered);
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    for write in 0..WRITES {
                        d.enqueue(format!("t{thread}-k{}", write % KEYS), json(write));
                        d.dispatch(|key, value| {
                            delivered
                                .lock()
                                .unwrap()
                                .entry(key.to_string())
                                .or_default()
                                .push(value.as_i64().unwrap());
                        });
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }

        let delivered = delivered.lock().unwrap();
        for thread in 0..THREADS {
            for key_index in 0..KEYS {
                let key = format!("t{thread}-k{key_index}");
                let values = &delivered[&key];
                assert!(
                    values.windows(2).all(|pair| pair[0] < pair[1]),
                    "{key}: {values:?}"
                );
                assert_eq!(values.last(), Some(&(WRITES - KEYS + key_index)), "{key}");
            }
        }
        d.dispatch(|key, _| panic!("queue should be empty, found {key}"));
    }

    // =====================================================
    // Panics
    //
    // A panicking callback is not isolated here (todo
    // 01KWG1PH0QCDQTARA5JCW7ABYK-keep-a-panicking-gadget-from-aborting-the-whole-search-and-execute.md);
    // the dispatcher must only not make it worse than a dead
    // dispatcher for that gadget.
    // =====================================================

    #[test]
    fn a_panicking_callback_leaves_enqueue_and_dispatch_callable() {
        let d = dispatcher();
        d.enqueue("boom".into(), json(1));

        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            d.dispatch(|_, _| panic!("gadget bug"));
        }));
        assert!(outcome.is_err());

        d.enqueue("after".into(), json(2));
        d.dispatch(|_, _| {});
    }

    #[test]
    fn interleaved_keys_preserve_order() {
        let d = dispatcher();
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
        let d = dispatcher();
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
