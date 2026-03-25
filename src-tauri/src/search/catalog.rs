// =========================================================
// Catalog Registry
//
// Holds all registered catalog plugins and provides the core
// search function: run nucleo fuzzy matching against every
// catalog entry, return scored results sorted by relevance.
// =========================================================

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use super::types::{ActionId, ScoredEntry};
use crate::plugins::CatalogPlugin;

pub struct CatalogRegistry {
    plugins: Vec<Box<dyn CatalogPlugin>>,
}

impl CatalogRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn CatalogPlugin>) {
        self.plugins.push(plugin);
    }

    /// Search all catalog entries against the given query.
    ///
    /// Empty query returns all entries with score 0 (home screen).
    /// Non-empty query uses nucleo fuzzy matching on the title and
    /// keywords, returning only entries that match.
    pub fn search(&self, query: &str) -> Vec<ScoredEntry> {
        if query.is_empty() {
            return self.all_entries_unscored();
        }

        // Matcher allocates ~135KB of scratch space. Creating it per
        // search call is acceptable for small catalogs (sub-ms). When
        // catalogs grow large, switch to `Nucleo<T>` async worker.
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

        let mut results = Vec::new();
        let mut char_buf = Vec::new();
        let mut title_indices = Vec::new();

        for plugin in &self.plugins {
            let source = plugin.id().to_string();

            for entry in plugin.entries() {
                // -------------------------------------------------------
                // Match against title — this produces the highlight
                // positions shown in the UI.
                // -------------------------------------------------------
                title_indices.clear();
                let title_haystack = Utf32Str::new(&entry.title, &mut char_buf);
                let title_score = pattern.indices(title_haystack, &mut matcher, &mut title_indices);

                // -------------------------------------------------------
                // Match against keywords as a fallback. If the title
                // didn't match, try "{title} {keywords}" to catch
                // aliases like "exit" matching "Quit Torchsnap".
                // We don't track keyword positions for highlighting —
                // only the title positions matter for display.
                // -------------------------------------------------------
                let score = match title_score {
                    Some(s) => Some(s),
                    None if !entry.keywords.is_empty() => {
                        let combined =
                            format!("{} {}", entry.title, entry.keywords.join(" "));
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

        results.sort_by(|a, b| b.score.cmp(&a.score));
        results
    }

    /// Execute an action on an entry, routing to the owning plugin.
    pub fn execute(
        &self,
        source: &str,
        entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<()> {
        let plugin = self
            .plugins
            .iter()
            .find(|p| p.id() == source)
            .ok_or_else(|| anyhow::anyhow!("unknown plugin source: {source}"))?;

        plugin.execute(entry_id, action_id, app)
    }

    /// Return all entries from all plugins with score 0 and no
    /// highlight positions. Used for the empty-query home screen.
    fn all_entries_unscored(&self) -> Vec<ScoredEntry> {
        let mut results = Vec::new();

        for plugin in &self.plugins {
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
