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
        self.get_or_fetch_with_clock(Instant::now, fetch)
    }

    /// [`get_or_fetch`](Self::get_or_fetch) with an injectable
    /// clock, so tests can place reads at exact instants instead
    /// of sleeping. The clock is read twice: once to check
    /// expiry, and once after `fetch` returns to stamp the new
    /// value. Stamping after the fetch keeps a slow fetch from
    /// eating into the TTL window of its own result.
    fn get_or_fetch_with_clock<F>(&self, clock: impl Fn() -> Instant, fetch: F) -> T
    where
        F: FnOnce() -> T,
    {
        if let Some((when, value)) = self.slot.borrow().as_ref()
            && clock().saturating_duration_since(*when) < self.ttl
        {
            return value.clone();
        }

        let value = fetch();
        *self.slot.borrow_mut() = Some((clock(), value.clone()));
        value
    }

    /// Read at a fixed instant: the clock reports `now` both for
    /// the expiry check and for the stamp of a fresh fetch.
    #[cfg(test)]
    fn get_or_fetch_at<F>(&self, now: Instant, fetch: F) -> T
    where
        F: FnOnce() -> T,
    {
        self.get_or_fetch_with_clock(|| now, fetch)
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
        let cache: RateLimitCache<u32> = RateLimitCache::new(Duration::from_millis(20));
        let invocations = Cell::new(0);
        let t0 = Instant::now();
        cache.get_or_fetch_at(t0, || {
            invocations.set(invocations.get() + 1);
            1
        });
        let v = cache.get_or_fetch_at(t0 + Duration::from_millis(40), || {
            invocations.set(invocations.get() + 1);
            2
        });
        assert_eq!(v, 2);
        assert_eq!(invocations.get(), 2);
    }

    #[test]
    fn value_expires_exactly_at_ttl() {
        // The window is half-open: a read at `fetch + ttl`
        // already refetches, a read just before it does not.
        let ttl = Duration::from_millis(40);
        let cache: RateLimitCache<u32> = RateLimitCache::new(ttl);
        let t0 = Instant::now();
        cache.get_or_fetch_at(t0, || 1);
        let just_before = cache.get_or_fetch_at(t0 + ttl - Duration::from_nanos(1), || 2);
        let at_ttl = cache.get_or_fetch_at(t0 + ttl, || 3);
        assert_eq!(just_before, 1);
        assert_eq!(at_ttl, 3);
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
        let t0 = Instant::now();

        cache.get_or_fetch_at(t0, || {
            invocations.set(invocations.get() + 1);
            1
        });
        // +20 ms is inside the window: a cache hit. If hits
        // extended the window, the +50 ms read below would
        // still hit (20 + 40 > 50).
        let hit = cache.get_or_fetch_at(t0 + Duration::from_millis(20), || {
            invocations.set(invocations.get() + 1);
            2
        });
        let refetched = cache.get_or_fetch_at(t0 + Duration::from_millis(50), || {
            invocations.set(invocations.get() + 1);
            3
        });

        assert_eq!(hit, 1);
        assert_eq!(refetched, 3);
        assert_eq!(invocations.get(), 2);
    }

    #[test]
    fn ttl_starts_when_the_fetch_finishes() {
        // A slow fetch must not eat into its own TTL window: the
        // value is stamped after `fetch` returns. The fetch here
        // advances the clock by 30 ms, so the value is stamped at
        // +30 ms and still valid at +60 ms (< 30 + 40).
        let cache: RateLimitCache<u32> = RateLimitCache::new(Duration::from_millis(40));
        let t0 = Instant::now();
        let clock = Cell::new(t0);

        cache.get_or_fetch_with_clock(
            || clock.get(),
            || {
                clock.set(t0 + Duration::from_millis(30));
                1
            },
        );
        clock.set(t0 + Duration::from_millis(60));
        let v = cache.get_or_fetch_with_clock(|| clock.get(), || 2);

        assert_eq!(v, 1);
    }

    #[test]
    fn cached_value_clones_per_read() {
        // Confirm the cache returns owned `T` values so
        // multiple readers can consume independently.
        let cache: RateLimitCache<String> = RateLimitCache::new(Duration::from_secs(60));
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
