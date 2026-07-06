// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Rate-limit cache for expensive fetches on the synchronous
//! per-keystroke `search()` path.
//!
//! The launcher invokes a gadget's `search()` synchronously on
//! every keystroke. Repeating an expensive fetch on each call
//! is wasteful and exposes the user to the fetch's latency on a
//! slow or hung source. The cache holds the last fetched value
//! for up to `ttl` since the last successful fetch — *not*
//! since the last access — so this is rate-limiting, not
//! debounce. A mutating action calls
//! [`RateLimitCache::invalidate`] to force the next read to
//! refetch.

use std::cell::RefCell;
use std::time::{Duration, Instant};

/// Default cache TTL. 1 second balances launcher
/// responsiveness against source load: rapid keystrokes
/// share a single fetch result, and a paused user sees
/// fresh state within one second of resuming.
pub const DEFAULT_TTL: Duration = Duration::from_secs(1);

/// Generic rate-limit cache. `T` is whatever payload the
/// fetch closure returns; the cache stores it verbatim and
/// `Clone`s on read so callers can consume the value
/// independently of the cache slot's lifetime.
pub struct RateLimitCache<T: Clone> {
    ttl: Duration,
    /// `Some(when, value)` after the first successful fetch;
    /// `None` between construction and first fetch (and after
    /// `invalidate`).
    slot: RefCell<Option<(Instant, T)>>,
}

impl<T: Clone> RateLimitCache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            slot: RefCell::new(None),
        }
    }

    /// Return the cached value if it was fetched less than
    /// `ttl` ago; otherwise call `fetch`, store the result,
    /// and return it.
    ///
    /// `fetch` is invoked only when the cache slot is empty
    /// or expired — callers can pay the cost of the closure
    /// on every call without worrying about rebuilding it for
    /// nothing.
    pub fn get_or_fetch<F>(&self, fetch: F) -> T
    where
        F: FnOnce() -> T,
    {
        if let Some((when, value)) = self.slot.borrow().as_ref()
            && when.elapsed() < self.ttl
        {
            return value.clone();
        }

        let value = fetch();
        *self.slot.borrow_mut() = Some((Instant::now(), value.clone()));
        value
    }

    /// Drop the cached value so the next `get_or_fetch` will
    /// invoke its fetch closure regardless of how recently
    /// the previous fetch ran. Called after every mutating
    /// action so post-action state shows up on the next
    /// keystroke without waiting for TTL.
    pub fn invalidate(&self) {
        *self.slot.borrow_mut() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::thread::sleep;

    #[test]
    fn first_call_invokes_fetch() {
        let cache: RateLimitCache<u32> = RateLimitCache::new(Duration::from_secs(1));
        let invocations = Cell::new(0);
        let v = cache.get_or_fetch(|| {
            invocations.set(invocations.get() + 1);
            7
        });
        assert_eq!(v, 7);
        assert_eq!(invocations.get(), 1);
    }

    #[test]
    fn second_call_within_ttl_uses_cache() {
        let cache: RateLimitCache<u32> = RateLimitCache::new(Duration::from_secs(1));
        let invocations = Cell::new(0);
        for _ in 0..5 {
            cache.get_or_fetch(|| {
                invocations.set(invocations.get() + 1);
                7
            });
        }
        assert_eq!(invocations.get(), 1);
    }

    #[test]
    fn call_after_ttl_expiry_refetches() {
        // Tight TTL so the test stays fast.
        let cache: RateLimitCache<u32> = RateLimitCache::new(Duration::from_millis(20));
        let invocations = Cell::new(0);
        cache.get_or_fetch(|| {
            invocations.set(invocations.get() + 1);
            1
        });
        sleep(Duration::from_millis(40));
        cache.get_or_fetch(|| {
            invocations.set(invocations.get() + 1);
            2
        });
        assert_eq!(invocations.get(), 2);
    }

    #[test]
    fn invalidate_forces_refetch_on_next_call() {
        let cache: RateLimitCache<u32> = RateLimitCache::new(Duration::from_secs(60));
        let invocations = Cell::new(0);
        cache.get_or_fetch(|| {
            invocations.set(invocations.get() + 1);
            1
        });
        cache.invalidate();
        cache.get_or_fetch(|| {
            invocations.set(invocations.get() + 1);
            2
        });
        assert_eq!(invocations.get(), 2);
    }

    #[test]
    fn ttl_resets_from_last_fetch_not_last_access() {
        // Rate-limit semantics: the TTL counts down from the
        // moment of the *fetch*, not from the most recent
        // access. Repeated cache hits do not extend the
        // window.
        let cache: RateLimitCache<u32> = RateLimitCache::new(Duration::from_millis(40));
        let invocations = Cell::new(0);

        cache.get_or_fetch(|| {
            invocations.set(invocations.get() + 1);
            1
        });
        sleep(Duration::from_millis(20));
        cache.get_or_fetch(|| {
            invocations.set(invocations.get() + 1);
            2
        });
        sleep(Duration::from_millis(30)); // total: 50 ms > 40 ms TTL
        cache.get_or_fetch(|| {
            invocations.set(invocations.get() + 1);
            3
        });

        // Three fetches expected: initial, expired-after-first
        // attempt at 20 ms? no — the second access at +20ms
        // is within TTL, so cache hit (1 invocation total).
        // The third access at +50 ms is past TTL and refetches
        // (2 invocations total).
        assert_eq!(invocations.get(), 2);
    }

    #[test]
    fn cached_value_clones_per_read() {
        // Confirm the cache returns owned `T` values so
        // multiple readers can consume independently.
        let cache: RateLimitCache<String> =
            RateLimitCache::new(Duration::from_secs(60));
        let a = cache.get_or_fetch(|| "hello".to_string());
        let b = cache.get_or_fetch(|| "world".to_string());
        // Both reads see the first fetch (cache hit).
        assert_eq!(a, "hello");
        assert_eq!(b, "hello");
        // Independent ownership: dropping one doesn't affect
        // the other.
        drop(a);
        assert_eq!(b, "hello");
    }
}
