// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Website Metadata Service
//
// Host-level shared service that fetches, caches, and serves
// website metadata (title, description, favicon) to plugins.
//
// Two access modes via `lookup(domain, mode)`:
//
// - `LookupMode::Blocking` — waits until the host has an
//   answer (cache hit, fresh fetch, or coalesced in-flight
//   request). Used where the result gates result existence
//   or rendering decisions for the current cycle.
//
// - `LookupMode::Cached` — returns cached data instantly,
//   or `Pending` after scheduling a background fetch. Used
//   on hot paths like per-keystroke search where a network
//   round-trip is unacceptable.
//
// Favicon images are stored as files via `FaviconStore` (rasters
// converted to WebP, SVGs stored as-is). Page metadata is cached
// in SQLite under $APPCACHE with configurable TTL. Failed fetches
// (unreachable domains) are held in an in-memory negative cache
// for 30 minutes to avoid repeated network attempts.
// =========================================================

mod cache;
mod favicon_store;
mod fetch;
mod html_fields;
mod metadata;
pub mod protocol;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use serde_json::Value;
use url::Url;

use crate::commands::types::EntryIcon;
use crate::network::Http;
use crate::settings::notifier::SettingsNotifier;
use crate::storage::SqlStorage;

use self::favicon_store::FaviconStore;
use self::fetch::FetchError;
use self::protocol::host_favicon_url;

#[cfg(test)]
mod service_tests;

// =========================================================
// Constants
// =========================================================

/// How long failed fetch attempts are suppressed before retrying.
const NEGATIVE_CACHE_TTL: Duration = Duration::from_secs(30 * 60);

/// How often the retention cleanup thread wakes up to evict
/// expired entries and orphaned favicon files.
const RETENTION_INTERVAL: Duration = Duration::from_secs(30 * 60);

// =========================================================
// Public types
// =========================================================

/// Metadata about a website, ready for display in search results.
#[derive(Clone)]
pub struct WebsiteMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub favicon: EntryIcon,
}

/// Internal three-state result of a network fetch.
///
/// Used between `fetch_coalesced` and the public API. The public
/// `LookupResult` adds a `Pending` variant for the non-blocking path;
/// `MetadataResult` deliberately excludes it so the leader/subscriber
/// machinery has a tighter type that cannot represent "not yet tried".
#[derive(Clone)]
enum MetadataResult {
    /// Successfully extracted some metadata (fields may be partial).
    Found(WebsiteMetadata),
    /// HTTP response was received, but nothing extractable.
    ReachableNoData,
    /// DNS failure, timeout, connection refused, or similar.
    Unreachable,
}

/// Selects whether `lookup` blocks on the network or returns
/// immediately on cache miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupMode {
    /// Cache-first; never blocks. On cold miss returns `Pending` and
    /// schedules a background fetch — the next call sees `Hit` (or
    /// the appropriate other variant). Use in latency-sensitive paths.
    Cached,
    /// Cache-first; on cold miss blocks until the fetch completes
    /// or coalesces with any in-flight request for the same domain.
    /// Use when the result is load-bearing for the current render.
    Blocking,
}

/// Public result of a `lookup` call.
///
/// Identical to `MetadataResult` plus a `Pending` variant returned
/// only by `LookupMode::Cached` on cold misses.
#[derive(Clone)]
pub enum LookupResult {
    Hit(WebsiteMetadata),
    ReachableNoData,
    Unreachable,
    /// Only ever returned in `Cached` mode. A background fetch has
    /// been scheduled (or coalesced with an existing in-flight one);
    /// callers should retry on the next user-input cycle.
    Pending,
}

/// Errors returned by `lookup` for client-side mistakes that the
/// host can detect without touching the network.
#[derive(Debug, Clone)]
pub enum LookupError {
    /// Domain string is empty, oversized (>253 bytes per RFC 1035),
    /// or contains characters that are not valid in a bare domain
    /// (scheme separator `://`, path `/`, port `:`, whitespace, etc.).
    InvalidDomain(String),
}

// =========================================================
// Single-flight coalescing primitive
// =========================================================

/// Shared state between a fetch leader and its subscribers.
///
/// The leader runs the network fetch exactly once. Concurrent
/// callers for the same domain register as subscribers and
/// `wait` on the condvar until the leader publishes via
/// `notify_all`. The result is cloned to each subscriber.
struct FetchInFlight {
    /// `None` while the fetch is in progress; `Some(_)` after
    /// the leader publishes. Subscribers spin in a `wait` loop
    /// until this becomes `Some` (handles spurious wakeups).
    result: Mutex<Option<MetadataResult>>,
    done: Condvar,
}

/// Drop guard for the fetch leader.
///
/// If the leader's body panics or returns early without publishing,
/// the guard fills the result slot with `Unreachable`, notifies
/// any parked subscribers, and removes the entry from `in_flight`.
/// This prevents subscribers from blocking forever on a leader
/// that never finishes.
struct LeaderGuard<'a> {
    domain: &'a str,
    handle: Arc<FetchInFlight>,
    in_flight: &'a Mutex<HashMap<String, Arc<FetchInFlight>>>,
    /// Set to `true` after a successful publish so the `Drop`
    /// impl knows the cleanup has already happened.
    published: bool,
}

impl Drop for LeaderGuard<'_> {
    fn drop(&mut self) {
        // Always evict the entry from `in_flight`, even on panic —
        // a leftover entry would block all future lookups for the
        // domain forever.
        if let Ok(mut map) = self.in_flight.lock() {
            map.remove(self.domain);
        }

        if self.published {
            return;
        }

        // Leader unwound without publishing. Substitute Unreachable
        // so subscribers get a definitive answer instead of parking
        // indefinitely.
        if let Ok(mut slot) = self.handle.result.lock() {
            if slot.is_none() {
                *slot = Some(MetadataResult::Unreachable);
            }
        }
        self.handle.done.notify_all();
    }
}

// =========================================================
// Service
// =========================================================

pub struct WebsiteMetadataService {
    db: SqlStorage,
    /// Shared so the host's `torchsnap-favicon://` protocol
    /// handler can serve images from the same store the
    /// service writes to.
    favicons: Arc<FaviconStore>,
    http: Http,

    /// Domains that recently failed — avoids repeated network
    /// attempts for unreachable hosts. Cleared on app restart.
    negative_cache: Mutex<HashMap<String, Instant>>,

    /// Single-flight coalescing map.
    ///
    /// The first caller for a domain becomes the leader and runs
    /// the actual fetch; concurrent callers for the same domain
    /// register as subscribers and park on the shared `Condvar`
    /// until the leader publishes. This is a kernel-level wait
    /// (futex on Linux, ulock on macOS) — no busy-spinning.
    in_flight: Mutex<HashMap<String, Arc<FetchInFlight>>>,

    /// Cache TTL in days, updated reactively from settings.
    cache_ttl_days: Arc<AtomicU32>,

    retention_condvar: Arc<Condvar>,
    retention_shutdown: Arc<Mutex<bool>>,

    /// Constructs absolute URLs for fetching page metadata and favicons.
    /// Production default builds `https://{domain}{path}`. Tests override
    /// to point at an httpmock server (HTTP-only) via `with_url_builder`.
    url_for_path: Box<dyn Fn(&str, &str) -> String + Send + Sync>,

    /// Test-only hooks. Lets tests inject a delayed panic into
    /// `fetch_and_cache_inner` so the leader unwinds while subscribers
    /// are parked, exercising `LeaderGuard`'s panic-recovery path.
    #[cfg(test)]
    test_panic_after: Mutex<HashMap<String, Duration>>,
}

impl WebsiteMetadataService {
    /// Create the service. Opens the SQLite cache and configures
    /// the HTTP client, but does not spawn background threads yet.
    /// Call `start_retention()` after wrapping in `Arc`.
    pub fn new(
        cache_dir: PathBuf,
        notifier: &SettingsNotifier,
        initial_ttl_days: u32,
    ) -> anyhow::Result<Self> {
        let db = SqlStorage::open(cache_dir.join("metadata.sqlite3"), &[cache::MIGRATION_001])
            .context("open website metadata cache database")?;

        let favicons = Arc::new(FaviconStore::new(cache_dir.join("favicons")));

        let http = Http::builder()
            .default_timeout(Duration::from_secs(2))
            .max_size(512 * 1024)
            .build()
            .context("build website metadata HTTP client")?;

        // Shared atomic for the TTL value — updated by a watcher
        // thread and read by the service on every cache lookup.
        let cache_ttl_days = Arc::new(AtomicU32::new(initial_ttl_days));

        // Watch for TTL changes from the settings UI.
        let ttl_for_thread = Arc::clone(&cache_ttl_days);
        let mut ttl_watch = notifier.watch_with_initial::<u32>(
            "websiteMetadata.cacheTtlDays",
            Value::from(initial_ttl_days),
        );
        std::thread::spawn(move || {
            while let Some(val) = ttl_watch.blocking_changed() {
                ttl_for_thread.store(val.max(1), Ordering::Relaxed);
            }
        });

        Ok(Self {
            db,
            favicons,
            http,
            negative_cache: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashMap::new()),
            cache_ttl_days,
            retention_condvar: Arc::new(Condvar::new()),
            retention_shutdown: Arc::new(Mutex::new(false)),
            url_for_path: Box::new(|domain, path| format!("https://{domain}{path}")),
            #[cfg(test)]
            test_panic_after: Mutex::new(HashMap::new()),
        })
    }

    /// Configure `fetch_and_cache_inner` to sleep for `after` and then
    /// panic when invoked for `domain`. Used to exercise leader-panic
    /// recovery in the coalescing layer.
    #[cfg(test)]
    pub(crate) fn set_test_panic_after(&self, domain: &str, after: Duration) {
        self.test_panic_after
            .lock()
            .expect("test_panic_after not poisoned")
            .insert(domain.to_string(), after);
    }

    /// Override the URL builder. Test-only: lets fetches target an
    /// httpmock server bound to localhost:port instead of `https://`.
    #[cfg(test)]
    pub(crate) fn with_url_builder(
        mut self,
        builder: impl Fn(&str, &str) -> String + Send + Sync + 'static,
    ) -> Self {
        self.url_for_path = Box::new(builder);
        self
    }

    // ---------------------------------------------------------
    // Lifecycle
    // ---------------------------------------------------------

    /// Start the retention cleanup loop. Must be called after the
    /// service is wrapped in `Arc`.
    pub fn start_retention(self: &Arc<Self>) {
        let service = Arc::clone(self);
        let condvar = Arc::clone(&self.retention_condvar);
        let shutdown = Arc::clone(&self.retention_shutdown);

        std::thread::spawn(move || {
            loop {
                let ttl_days = service.cache_ttl_days.load(Ordering::Relaxed).max(1);

                // Evict expired metadata rows and collect remaining
                // favicon keys for orphan cleanup.
                let valid_keys = cache::evict_expired(&service.db, ttl_days);
                service.favicons.cleanup(&valid_keys);

                // Wait for the cleanup interval or a shutdown signal.
                let guard = shutdown.lock().expect("shutdown mutex not poisoned");
                let (guard, _) = condvar
                    .wait_timeout(guard, RETENTION_INTERVAL)
                    .expect("condvar wait not poisoned");

                if *guard {
                    break;
                }
            }
        });
    }

    /// Signal background threads to shut down.
    pub fn teardown(&self) {
        *self
            .retention_shutdown
            .lock()
            .expect("shutdown mutex not poisoned") = true;
        self.retention_condvar.notify_all();
    }

    // ---------------------------------------------------------
    // Public API
    // ---------------------------------------------------------

    /// Look up metadata for a registrable domain, with mode-controlled
    /// blocking semantics.
    ///
    /// Pipeline:
    /// 1. Validate `domain` syntactically (no scheme, path, port, or
    ///    whitespace; non-empty; ≤253 bytes per RFC 1035).
    /// 2. Negative-cache hit → `Unreachable`.
    /// 3. SQLite cache hit → `Hit` / `ReachableNoData`.
    /// 4. Cache miss: branch on `mode`.
    ///    - `Blocking` → `fetch_coalesced` (waits or coalesces).
    ///    - `Cached` → spawn a background fetch via `spawn_blocking`
    ///      and return `Pending` immediately. Coalescing inside
    ///      `fetch_coalesced` deduplicates concurrent background
    ///      fetches and any in-flight blocking fetch.
    pub fn lookup(
        self: &Arc<Self>,
        domain: &str,
        mode: LookupMode,
    ) -> Result<LookupResult, LookupError> {
        validate_domain(domain)?;

        if self.is_negatively_cached(domain) {
            return Ok(LookupResult::Unreachable);
        }

        let ttl_days = self.cache_ttl_days.load(Ordering::Relaxed).max(1);
        if let Some(entry) = cache::lookup(&self.db, domain, ttl_days) {
            return Ok(metadata_to_lookup(self.cached_entry_to_result(entry)));
        }

        match mode {
            LookupMode::Blocking => Ok(metadata_to_lookup(self.fetch_coalesced(domain))),
            LookupMode::Cached => {
                // Schedule the fetch on the blocking pool; coalescing
                // makes this a no-op if a leader is already running
                // for the same domain.
                let svc = Arc::clone(self);
                let d = domain.to_string();
                tauri::async_runtime::spawn_blocking(move || {
                    let _ = svc.fetch_coalesced(&d);
                });
                Ok(LookupResult::Pending)
            }
        }
    }

    /// Gather cache statistics for the settings UI.
    /// Shared handle to the favicon store. Used by the
    /// `torchsnap-favicon://` Tauri protocol handler to read
    /// cached favicon bytes on demand.
    pub fn favicon_store(&self) -> Arc<FaviconStore> {
        Arc::clone(&self.favicons)
    }

    pub fn stats(&self) -> cache::CacheStats {
        let mut stats = cache::stats(&self.db);
        stats.favicon_bytes = self.favicons.disk_usage();
        stats
    }

    /// Clear all cached metadata and favicon files.
    pub fn clear_cache(&self) -> anyhow::Result<()> {
        cache::clear_all(&self.db)?;
        self.favicons.clear();

        // Also clear the negative cache so domains can be retried.
        self.negative_cache
            .lock()
            .expect("negative cache not poisoned")
            .clear();

        Ok(())
    }

    // ---------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------

    /// Check if a domain is in the negative cache and not yet expired.
    fn is_negatively_cached(&self, domain: &str) -> bool {
        let cache = self
            .negative_cache
            .lock()
            .expect("negative cache not poisoned");
        if let Some(timestamp) = cache.get(domain) {
            timestamp.elapsed() < NEGATIVE_CACHE_TTL
        } else {
            false
        }
    }

    /// Record a domain as unreachable in the negative cache.
    fn record_negative(&self, domain: &str) {
        self.negative_cache
            .lock()
            .expect("negative cache not poisoned")
            .insert(domain.to_string(), Instant::now());
    }

    /// Convert a cached DB entry into a `MetadataResult`.
    fn cached_entry_to_result(&self, entry: cache::CachedEntry) -> MetadataResult {
        if !entry.reachable {
            return MetadataResult::Unreachable;
        }

        // ReachableNoData: row exists with reachable=true but all
        // metadata fields are NULL.
        if entry.title.is_none() && entry.description.is_none() && entry.favicon_key.is_none() {
            return MetadataResult::ReachableNoData;
        }

        // Build the renderable URL directly from `(key, ext)` —
        // we don't need to resolve to a filesystem path here.
        // The host-favicon protocol handler resolves the URL on
        // demand when the launcher loads the `<img src>`. If the
        // file no longer exists on disk (cleanup raced ahead of
        // the cache row), the protocol handler returns 404 and
        // the `<img>` falls back to the alt text.
        let favicon = match (&entry.favicon_key, &entry.favicon_ext) {
            (Some(key), Some(ext)) => EntryIcon::AssetIcon(host_favicon_url(key, ext)),
            _ => EntryIcon::HeroIcon("globe-alt".to_string()),
        };

        MetadataResult::Found(WebsiteMetadata {
            title: entry.title,
            description: entry.description,
            favicon,
        })
    }

    /// Try well-known favicon paths as a last resort.
    ///
    /// Many SPAs and API-first sites don't serve HTML metadata at the
    /// root URL, but still have a valid favicon at a conventional path.
    /// We try paths in quality order: SVG > PNG > ICO.
    fn try_fallback_favicon(
        &self,
        domain: &str,
    ) -> (Option<String>, Option<String>, Option<String>, EntryIcon) {
        let candidates = [
            (self.url_for_path)(domain, "/favicon.svg"),
            (self.url_for_path)(domain, "/favicon.png"),
            (self.url_for_path)(domain, "/favicon.ico"),
        ];

        for favicon_url in &candidates {
            if let Some(stored) = self.fetch_and_store_favicon(favicon_url) {
                let icon = EntryIcon::AssetIcon(host_favicon_url(&stored.key, &stored.ext));
                return (
                    Some(favicon_url.clone()),
                    Some(stored.key),
                    Some(stored.ext),
                    icon,
                );
            }
        }

        (
            None,
            None,
            None,
            EntryIcon::HeroIcon("globe-alt".to_string()),
        )
    }

    /// Coalesced network fetch.
    ///
    /// The first caller for a given domain becomes the *leader* and runs
    /// `fetch_and_cache_inner` synchronously. Concurrent callers for the
    /// same domain become *subscribers*, parking on the leader's
    /// `Condvar` until the result is published. Subscribers receive a
    /// clone of the leader's result — no second network request fires.
    ///
    /// Coalescing applies even across modes (e.g. a background fetch
    /// triggered by `try_cached` and a foreground `get` for the same
    /// domain produce one HTTP call total).
    fn fetch_coalesced(&self, domain: &str) -> MetadataResult {
        // Phase 1: register as either leader or subscriber.
        let (handle, is_leader) = {
            let mut map = self.in_flight.lock().expect("in_flight not poisoned");
            if let Some(existing) = map.get(domain) {
                (Arc::clone(existing), false)
            } else {
                let h = Arc::new(FetchInFlight {
                    result: Mutex::new(None),
                    done: Condvar::new(),
                });
                map.insert(domain.to_string(), Arc::clone(&h));
                (h, true)
            }
        };

        if is_leader {
            // Drop guard ensures `in_flight` is cleared and subscribers
            // are unblocked even if `fetch_and_cache_inner` panics.
            let mut guard = LeaderGuard {
                domain,
                handle: Arc::clone(&handle),
                in_flight: &self.in_flight,
                published: false,
            };

            let result = self.fetch_and_cache_inner(domain);

            // Publish the result and wake subscribers BEFORE dropping
            // the guard so the guard's Drop sees `published = true`
            // and skips the panic-fallback path.
            {
                let mut slot = handle.result.lock().expect("result mutex not poisoned");
                *slot = Some(result.clone());
            }
            handle.done.notify_all();
            guard.published = true;

            result
        } else {
            // Subscriber: park on the condvar until the leader publishes.
            // The `while` loop guards against spurious wakeups (the kernel
            // may wake parked threads without a matching `notify_all`).
            let mut slot = handle.result.lock().expect("result mutex not poisoned");
            while slot.is_none() {
                slot = handle.done.wait(slot).expect("condvar wait not poisoned");
            }
            slot.as_ref().expect("set above the loop exit").clone()
        }
    }

    /// Inner fetch — performs the actual network work and writes to the
    /// cache. Called exactly once per concurrent burst by the leader in
    /// `fetch_coalesced`.
    fn fetch_and_cache_inner(&self, domain: &str) -> MetadataResult {
        // Test-only: optionally sleep then panic, to exercise the
        // `LeaderGuard` panic-recovery path with a window for
        // subscribers to register before the leader unwinds.
        #[cfg(test)]
        {
            let after = self
                .test_panic_after
                .lock()
                .expect("test_panic_after not poisoned")
                .get(domain)
                .copied();
            if let Some(d) = after {
                std::thread::sleep(d);
                panic!("test-induced panic for {domain}");
            }
        }

        // Build the page URL via the service's URL constructor — production
        // points at https; tests redirect to a mock HTTP server.
        let page_url_string = (self.url_for_path)(domain, "/");
        let base_url = match Url::parse(&page_url_string) {
            Ok(u) => u,
            Err(_) => {
                self.record_negative(domain);
                return MetadataResult::Unreachable;
            }
        };

        // Fetch page metadata.
        let page_metadata = match fetch::fetch_page_metadata(&self.http, &base_url) {
            Ok(meta) => meta,
            Err(FetchError::NotHtml { .. }) => {
                // Domain is reachable but returned non-HTML content (e.g.,
                // SPA backends, API-first sites). Still try well-known
                // favicon paths as a fallback.
                let (favicon_url, favicon_key, favicon_ext, favicon) =
                    self.try_fallback_favicon(domain);
                cache::store(
                    &self.db,
                    &cache::CacheEntry {
                        domain,
                        title: None,
                        description: None,
                        favicon_url: favicon_url.as_deref(),
                        favicon_key: favicon_key.as_deref(),
                        favicon_ext: favicon_ext.as_deref(),
                        reachable: true,
                    },
                );
                return if favicon_key.is_some() {
                    MetadataResult::Found(WebsiteMetadata {
                        title: None,
                        description: None,
                        favicon,
                    })
                } else {
                    MetadataResult::ReachableNoData
                };
            }
            Err(_) => {
                self.record_negative(domain);
                return MetadataResult::Unreachable;
            }
        };

        // If HTML extraction yielded nothing useful, try well-known
        // favicon paths before giving up.
        if page_metadata.title.is_none()
            && page_metadata.description.is_none()
            && page_metadata.favicon_url.is_none()
        {
            let (favicon_url, favicon_key, favicon_ext, favicon) =
                self.try_fallback_favicon(domain);
            cache::store(
                &self.db,
                &cache::CacheEntry {
                    domain,
                    title: None,
                    description: None,
                    favicon_url: favicon_url.as_deref(),
                    favicon_key: favicon_key.as_deref(),
                    favicon_ext: favicon_ext.as_deref(),
                    reachable: true,
                },
            );
            return if favicon_key.is_some() {
                MetadataResult::Found(WebsiteMetadata {
                    title: None,
                    description: None,
                    favicon,
                })
            } else {
                MetadataResult::ReachableNoData
            };
        }

        // Attempt to fetch and store the favicon image.
        let (favicon_key, favicon_ext, favicon) =
            if let Some(ref favicon_url) = page_metadata.favicon_url {
                match self.fetch_and_store_favicon(favicon_url) {
                    Some(stored) => {
                        let icon =
                            EntryIcon::AssetIcon(host_favicon_url(&stored.key, &stored.ext));
                        (Some(stored.key), Some(stored.ext), icon)
                    }
                    None => (None, None, EntryIcon::HeroIcon("globe-alt".to_string())),
                }
            } else {
                (None, None, EntryIcon::HeroIcon("globe-alt".to_string()))
            };

        // Store in SQLite.
        cache::store(
            &self.db,
            &cache::CacheEntry {
                domain,
                title: page_metadata.title.as_deref(),
                description: page_metadata.description.as_deref(),
                favicon_url: page_metadata.favicon_url.as_deref(),
                favicon_key: favicon_key.as_deref(),
                favicon_ext: favicon_ext.as_deref(),
                reachable: true,
            },
        );

        MetadataResult::Found(WebsiteMetadata {
            title: page_metadata.title,
            description: page_metadata.description,
            favicon,
        })
    }

    /// Fetch a favicon image and store it via the `FaviconStore`.
    fn fetch_and_store_favicon(&self, favicon_url: &str) -> Option<favicon_store::StoredFavicon> {
        let image_data = fetch::fetch_favicon_image(&self.http, favicon_url).ok()?;
        self.favicons.store(
            favicon_url,
            &image_data.image_data,
            &image_data.content_type,
        )
    }
}

// =========================================================
// Helpers
// =========================================================

/// Validate a domain string for syntactic correctness without
/// touching the network. PSL/eTLD+1 validation is the caller's
/// responsibility — the host is concerned only with rejecting
/// inputs that are clearly not bare domains.
fn validate_domain(domain: &str) -> Result<(), LookupError> {
    if domain.is_empty() {
        return Err(LookupError::InvalidDomain("empty".into()));
    }
    // RFC 1035 caps total domain length at 253 octets. Anything
    // longer is definitely malformed.
    if domain.len() > 253 {
        return Err(LookupError::InvalidDomain(format!(
            "exceeds 253 bytes ({} given)",
            domain.len()
        )));
    }
    // Reject obvious non-domain shapes: schemes, paths, ports,
    // whitespace, leading/trailing dots, or null bytes.
    if domain.contains("://")
        || domain.contains('/')
        || domain.contains(':')
        || domain.contains(' ')
        || domain.contains('\t')
        || domain.contains('\n')
        || domain.contains('\0')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err(LookupError::InvalidDomain(domain.to_string()));
    }
    Ok(())
}

/// Project an internal `MetadataResult` onto the public `LookupResult`.
fn metadata_to_lookup(r: MetadataResult) -> LookupResult {
    match r {
        MetadataResult::Found(m) => LookupResult::Hit(m),
        MetadataResult::ReachableNoData => LookupResult::ReachableNoData,
        MetadataResult::Unreachable => LookupResult::Unreachable,
    }
}
