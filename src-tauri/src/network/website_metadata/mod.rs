// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Website Metadata Service
//
// Host-level shared service that fetches, caches, and serves
// website metadata (title, description, favicon) to plugins.
//
// Two access patterns:
//
// - `get(domain)` — synchronous, blocks until fetched or failed.
//   Used by plugins where the user expects a brief wait (open-url).
//
// - `try_cached(domain)` — returns cached data instantly or
//   triggers a background fetch. Used by plugins where blocking
//   is unacceptable (bangs, catalog entries).
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

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use serde_json::Value;

use crate::network::Http;
use crate::search::types::EntryIcon;
use crate::settings_notifier::SettingsNotifier;
use crate::storage::SqlStorage;

use self::favicon_store::FaviconStore;
use self::fetch::FetchError;

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
pub struct WebsiteMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub favicon: EntryIcon,
}

/// Three-state result from a metadata lookup.
///
/// Plugins use this to decide how to render results:
/// - `Found` → show enriched result with favicon and title
/// - `ReachableNoData` → domain responded but no useful metadata
///   (show a globe icon or similar)
/// - `Unreachable` → couldn't reach the domain at all
///   (keep the default plugin icon)
pub enum MetadataResult {
    /// Successfully extracted some metadata (fields may be partial).
    Found(WebsiteMetadata),
    /// HTTP response was received, but nothing extractable.
    ReachableNoData,
    /// DNS failure, timeout, connection refused, or similar.
    Unreachable,
}

// =========================================================
// Service
// =========================================================

pub struct WebsiteMetadataService {
    db: SqlStorage,
    favicons: FaviconStore,
    http: Http,

    /// Domains that recently failed — avoids repeated network
    /// attempts for unreachable hosts. Cleared on app restart.
    negative_cache: Mutex<HashMap<String, Instant>>,

    /// Domains currently being fetched in background threads
    /// (triggered by `try_cached`). Prevents duplicate spawns.
    in_flight: Mutex<HashSet<String>>,

    /// Cache TTL in days, updated reactively from settings.
    cache_ttl_days: Arc<AtomicU32>,

    retention_condvar: Arc<Condvar>,
    retention_shutdown: Arc<Mutex<bool>>,
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

        let favicons = FaviconStore::new(cache_dir.join("favicons"));

        let http = Http::builder()
            .default_timeout(Duration::from_secs(10))
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
            in_flight: Mutex::new(HashSet::new()),
            cache_ttl_days,
            retention_condvar: Arc::new(Condvar::new()),
            retention_shutdown: Arc::new(Mutex::new(false)),
        })
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

    /// Fetch metadata for a domain, blocking until complete.
    ///
    /// Checks the cache first. On cache miss, fetches from the
    /// network and stores the result before returning. Failed
    /// fetches are recorded in the negative cache.
    pub fn get(&self, domain: &str) -> MetadataResult {
        // 1. Check negative cache.
        if self.is_negatively_cached(domain) {
            return MetadataResult::Unreachable;
        }

        // 2. Check SQLite cache.
        let ttl_days = self.cache_ttl_days.load(Ordering::Relaxed).max(1);
        if let Some(entry) = cache::lookup(&self.db, domain, ttl_days) {
            return self.cached_entry_to_result(entry);
        }

        // 3. Fetch from network.
        self.fetch_and_cache(domain)
    }

    /// Return cached metadata immediately, or `Unreachable` with a
    /// background fetch triggered on cache miss.
    ///
    /// Non-blocking: the caller always gets an immediate response.
    /// On cache miss, a background thread is spawned to fetch the
    /// domain. The result will be available on the next call.
    pub fn try_cached(self: &Arc<Self>, domain: &str) -> MetadataResult {
        // 1. Check negative cache.
        if self.is_negatively_cached(domain) {
            return MetadataResult::Unreachable;
        }

        // 2. Check SQLite cache.
        let ttl_days = self.cache_ttl_days.load(Ordering::Relaxed).max(1);
        if let Some(entry) = cache::lookup(&self.db, domain, ttl_days) {
            return self.cached_entry_to_result(entry);
        }

        // 3. Cache miss — spawn background fetch if not already in-flight.
        self.spawn_background_fetch(domain);
        MetadataResult::Unreachable
    }

    /// Gather cache statistics for the settings UI.
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

        let favicon = match (&entry.favicon_key, &entry.favicon_ext) {
            (Some(key), Some(ext)) => self
                .favicons
                .resolve(key, ext)
                .map(EntryIcon::AssetIcon)
                .unwrap_or_else(|| EntryIcon::HeroIcon("globe-alt".to_string())),
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
            format!("https://{domain}/favicon.svg"),
            format!("https://{domain}/favicon.png"),
            format!("https://{domain}/favicon.ico"),
        ];

        for favicon_url in &candidates {
            if let Some(stored) = self.fetch_and_store_favicon(favicon_url) {
                return (
                    Some(favicon_url.clone()),
                    Some(stored.key),
                    Some(stored.ext),
                    EntryIcon::AssetIcon(stored.path),
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

    /// Fetch metadata and favicon from the network, store in cache,
    /// and return the result.
    fn fetch_and_cache(&self, domain: &str) -> MetadataResult {
        // Fetch page metadata.
        let page_metadata = match fetch::fetch_page_metadata(&self.http, domain) {
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
                    Some(stored) => (
                        Some(stored.key),
                        Some(stored.ext),
                        EntryIcon::AssetIcon(stored.path),
                    ),
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

    /// Spawn a background thread to fetch metadata for a domain.
    ///
    /// Deduplicates: if the domain is already being fetched, this
    /// is a no-op. The thread removes the domain from `in_flight`
    /// on completion (success or failure).
    fn spawn_background_fetch(self: &Arc<Self>, domain: &str) {
        let mut in_flight = self.in_flight.lock().expect("in_flight not poisoned");
        if in_flight.contains(domain) {
            return;
        }
        in_flight.insert(domain.to_string());
        drop(in_flight);

        let service = Arc::clone(self);
        let domain_owned = domain.to_string();

        // Must use spawn_blocking (not std::thread::spawn) because
        // the HTTP client internally calls block_on(), which requires
        // the Tokio reactor to be available on the current thread.
        tauri::async_runtime::spawn_blocking(move || {
            service.fetch_and_cache(&domain_owned);

            service
                .in_flight
                .lock()
                .expect("in_flight not poisoned")
                .remove(&domain_owned);
        });
    }
}
