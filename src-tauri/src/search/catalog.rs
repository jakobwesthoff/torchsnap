// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Registry
//
// Holds all registered plugins (both catalog and query) and
// provides the unified search function with prefix-based
// routing (ADR 0012):
//
// - No prefix match → nucleo over catalog entries + always-on
//   query plugins
// - Prefix match → exclusive routing to the owning query plugin,
//   catalog plugins skipped entirely
// =========================================================

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use super::types::{ActionId, PostAction, ScoredEntry, SearchResult};
use crate::plugins::{CatalogPlugin, QueryPlugin};

use std::sync::Arc;
use std::thread;

use rayon::prelude::*;

pub struct CatalogRegistry {
    catalog_plugins: Vec<Arc<dyn CatalogPlugin>>,
    query_plugins: Vec<Arc<dyn QueryPlugin>>,
}

impl CatalogRegistry {
    pub fn new() -> Self {
        Self {
            catalog_plugins: Vec::new(),
            query_plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn CatalogPlugin>) {
        self.catalog_plugins.push(Arc::from(plugin));
    }

    pub fn register_query(&mut self, plugin: Box<dyn QueryPlugin>) {
        self.query_plugins.push(Arc::from(plugin));
    }

    /// Call `setup()` on every registered plugin in parallel.
    ///
    /// Uses a rayon thread pool with a rolling window — as soon as
    /// one plugin finishes, the next one starts. Returns immediately;
    /// a coordinator thread manages the pool in the background.
    ///
    /// Both catalog and query plugins are set up together in the
    /// same pool.
    pub fn setup_all(&self) {
        // Collect setup closures from both plugin types into a single
        // vec so rayon can schedule them as a unified work pool.
        let mut setup_fns: Vec<Box<dyn FnOnce() + Send>> = Vec::new();

        for p in &self.catalog_plugins {
            let p = Arc::clone(p);
            setup_fns.push(Box::new(move || p.setup()));
        }
        for p in &self.query_plugins {
            let p = Arc::clone(p);
            setup_fns.push(Box::new(move || p.setup()));
        }

        thread::spawn(move || {
            // Use 70% of available cores for plugin setup, leaving
            // headroom for the UI thread and system tasks. Plugin
            // setup is mostly I/O-bound so full core saturation
            // would waste resources.
            let cores = thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            let num_threads = (cores * 7 / 10).max(1);

            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .expect("rayon setup thread pool");

            pool.install(|| {
                setup_fns.into_par_iter().for_each(|f| f());
            });
        });
    }

    /// Search all plugins against the given query.
    ///
    /// Returns the scored entries and an optional active plugin ID.
    /// When the active plugin is `Some`, the frontend should mount
    /// that plugin's custom UI component instead of the standard
    /// `ResultList` (ADR 0013).
    ///
    /// Routing follows ADR 0012:
    /// - If the query starts with a registered prefix → exclusive
    ///   routing to that query plugin only
    /// - Otherwise → nucleo over catalog entries + always-on query
    ///   plugins, merged by score
    pub fn search(&self, query: &str) -> SearchResult {
        // TODO: Empty query could show recent/pinned items in the future.
        // For now, return nothing — the launcher should feel clean on open.
        if query.is_empty() {
            return SearchResult::empty();
        }

        // -------------------------------------------------------
        // Prefix routing: check if the query matches a registered
        // prefix. Longest match wins to handle overlapping prefixes
        // (e.g., ":" vs ":e" — the longer one takes priority).
        // -------------------------------------------------------
        if let Some((plugin, prefix)) = self.find_prefix_match(query) {
            let stripped = &query[prefix.len()..];
            let source = plugin.id().to_string();
            let response = plugin.search(stripped, Some(prefix));

            // When the plugin requests custom UI, signal its ID to
            // the frontend. For standard results, no active plugin.
            let custom_plugin_view = if response.is_custom_ui() {
                Some(source.clone())
            } else {
                None
            };

            let entries = response
                .into_results()
                .into_iter()
                .map(|r| r.into_scored_entry(source.clone()))
                .collect();

            return SearchResult {
                entries,
                custom_plugin_view,
                matched_prefix: Some(prefix.to_string()),
            };
        }

        // -------------------------------------------------------
        // No prefix match: run catalog plugins (nucleo) + always-on
        // query plugins, merge results. CustomUI from non-exclusive
        // plugins is treated as standard Results.
        // -------------------------------------------------------
        let mut results = self.search_catalogs(query);

        // Always-on query plugins (no prefixes registered).
        for plugin in &self.query_plugins {
            if !plugin.prefixes().is_empty() {
                continue;
            }
            let source = plugin.id().to_string();
            for result in plugin.search(query, None).into_results() {
                results.push(result.into_scored_entry(source.clone()));
            }
        }

        results.sort_by(|a, b| b.score.cmp(&a.score));
        SearchResult {
            entries: results,
            custom_plugin_view: None,
            matched_prefix: None,
        }
    }

    /// Find the query plugin whose prefix matches the start of the
    /// query. Returns the plugin and the matched prefix string.
    /// Longest prefix wins; first plugin wins on ties.
    fn find_prefix_match<'a>(&'a self, query: &str) -> Option<(&'a Arc<dyn QueryPlugin>, &'a str)> {
        let mut best: Option<(&Arc<dyn QueryPlugin>, &str)> = None;
        let mut best_len = 0;

        for plugin in &self.query_plugins {
            for &prefix in plugin.prefixes() {
                if prefix.len() > best_len && query.starts_with(prefix) {
                    best = Some((plugin, prefix));
                    best_len = prefix.len();
                }
            }
        }

        best
    }

    /// Run nucleo fuzzy matching across all catalog plugin entries.
    fn search_catalogs(&self, query: &str) -> Vec<ScoredEntry> {
        // Matcher allocates ~135KB of scratch space. Creating it per
        // search call is acceptable for small catalogs (sub-ms). When
        // catalogs grow large, switch to `Nucleo<T>` async worker.
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

        let mut results = Vec::new();
        let mut char_buf = Vec::new();
        let mut title_indices = Vec::new();

        for plugin in &self.catalog_plugins {
            let source = plugin.id().to_string();

            for entry in plugin.entries() {
                // Match against title — this produces the highlight
                // positions shown in the UI.
                title_indices.clear();
                let title_haystack = Utf32Str::new(&entry.title, &mut char_buf);
                let title_score = pattern.indices(title_haystack, &mut matcher, &mut title_indices);

                // Match against keywords as a fallback. If the title
                // didn't match, try "{title} {keywords}" to catch
                // aliases like "exit" matching "Quit Torchsnap".
                // We don't track keyword positions for highlighting —
                // only the title positions matter for display.
                let score = match title_score {
                    Some(s) => Some(s),
                    None if !entry.keywords.is_empty() => {
                        let combined = format!("{} {}", entry.title, entry.keywords.join(" "));
                        let combined_haystack = Utf32Str::new(&combined, &mut char_buf);
                        pattern.score(combined_haystack, &mut matcher)
                    }
                    None => None,
                };

                if let Some(score) = score {
                    title_indices.sort_unstable();
                    title_indices.dedup();

                    results.push(ScoredEntry {
                        id: entry.id,
                        title: entry.title,
                        subtitle: entry.subtitle,
                        icon: entry.icon,
                        score,
                        title_positions: title_indices.clone(),
                        subtitle_positions: vec![],
                        source: source.clone(),
                        actions: entry.actions,
                    });
                }
            }
        }

        results
    }

    /// Execute an action on an entry, routing to the owning plugin.
    ///
    /// Searches both catalog and query plugins by source ID.
    /// Returns the plugin's `PostAction` so the caller can decide
    /// whether to dismiss the launcher.
    pub fn execute(
        &self,
        source: &str,
        entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        // Check catalog plugins first.
        if let Some(plugin) = self.catalog_plugins.iter().find(|p| p.id() == source) {
            return plugin.execute(entry_id, action_id, app);
        }

        // Then query plugins.
        if let Some(plugin) = self.query_plugins.iter().find(|p| p.id() == source) {
            return plugin.execute(entry_id, action_id, app);
        }

        anyhow::bail!("unknown plugin source: {source}");
    }

    /// Return all entries from all catalog plugins with score 0 and no
    /// highlight positions. Will be used for the empty-query home
    /// screen (recent/pinned items) once that feature is built.
    #[allow(dead_code)]
    fn all_entries_unscored(&self) -> Vec<ScoredEntry> {
        let mut results = Vec::new();

        for plugin in &self.catalog_plugins {
            let source = plugin.id().to_string();
            for entry in plugin.entries() {
                results.push(ScoredEntry {
                    id: entry.id,
                    title: entry.title,
                    subtitle: entry.subtitle,
                    icon: entry.icon,
                    score: 0,
                    title_positions: vec![],
                    subtitle_positions: vec![],
                    source: source.clone(),
                    actions: entry.actions,
                });
            }
        }

        results
    }
}
