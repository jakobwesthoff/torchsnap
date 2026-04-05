// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Open URL Plugin
//
// Detects URLs in the search query (both explicit like
// "https://example.com/path" and bare domains like "example.com")
// and surfaces a single result entry that opens the URL in the
// default browser.
//
// Results are enriched with website metadata (page title, favicon)
// fetched via the shared `WebsiteMetadataService`. Bare domains
// are validated against the Public Suffix List (via the `addr`
// crate) to avoid false positives from random dot-separated words.
//
// Metadata lookups are domain-keyed and blocking — the plugin
// waits for the fetch to complete before showing a result. For
// bare domains, the entry is suppressed entirely if the domain
// is unreachable.
// =========================================================

use std::sync::{Arc, Mutex};

use anyhow::Context;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

use crate::network::website_metadata::{MetadataResult, WebsiteMetadataService};
use crate::search::types::{
    Action, ActionId, ActionKeybinding, EntryIcon, PluginResponse, PostAction,
    ScoredEntry,
};
use crate::unicode::Utf16Positions;

use super::Plugin;

// =========================================================
// Constants
// =========================================================

const PLUGIN_ID: &str = "open-url";

/// Score for URL results — same priority level as bang results.
const URL_SCORE: u32 = 500;

// =========================================================
// URL Detection
// =========================================================

/// A URL parsed from the user's search query.
struct DetectedUrl {
    /// The full URL including scheme (e.g. "https://github.com/user/repo").
    full_url: String,
    /// The domain portion only (e.g. "github.com").
    domain: String,
    /// Whether the user typed an explicit scheme (http:// or https://).
    /// Bare domains like "github.com" have this set to false.
    has_explicit_scheme: bool,
}

/// Attempt to detect a URL in the search query.
///
/// Two detection paths:
/// 1. Explicit scheme — `url::Url::parse` succeeds with http/https
///    and a host. No TLD validation needed (user was explicit).
/// 2. Bare domain — prepend "https://", parse, extract host, then
///    validate the TLD against the Public Suffix List via `addr`.
///    This filters out random words that happen to contain dots
///    (e.g. "hello.world" is not a valid TLD).
fn detect_url(query: &str) -> Option<DetectedUrl> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Path 1: explicit scheme (http:// or https://).
    if let Ok(parsed) = url::Url::parse(trimmed) {
        let scheme = parsed.scheme();
        if (scheme == "http" || scheme == "https") && parsed.host_str().is_some() {
            let domain = parsed.host_str().expect("host presence checked above").to_string();
            return Some(DetectedUrl {
                full_url: parsed.to_string(),
                domain,
                has_explicit_scheme: true,
            });
        }
    }

    // Path 2: bare domain — prepend scheme and validate TLD.
    let candidate = format!("https://{trimmed}");
    let parsed = url::Url::parse(&candidate).ok()?;
    let host = parsed.host_str()?;

    // The `addr` crate checks the host against the Public Suffix List.
    // `has_known_suffix()` returns true only for domains whose TLD
    // is in the PSL (e.g. ".com", ".org", ".co.uk"), filtering out
    // nonsense like "foo.notarealtld".
    let domain_name = addr::parse_domain_name(host).ok()?;
    if !domain_name.has_known_suffix() {
        return None;
    }

    Some(DetectedUrl {
        full_url: candidate,
        domain: host.to_string(),
        has_explicit_scheme: false,
    })
}

// =========================================================
// Plugin Struct
// =========================================================

pub struct OpenUrlPlugin {
    metadata_service: Arc<WebsiteMetadataService>,

    // HACK/FIXME: Stores the resolved URL from the most recent search()
    // so execute() can open it. This is a workaround because execute()
    // only receives the entry_id (which is the domain for frecency
    // tracking) and has no access to the full URL.
    //
    // This MUST be replaced with the arbitrary data parameter mechanism
    // once todo 01kn7v6ynyf580ax9jyyt25jgc is implemented. That
    // refactor should happen shortly after this plugin lands.
    //
    // Safety: The UI can only execute the currently displayed result,
    // and search() runs synchronously per query cycle, so the stored
    // URL always matches what's on screen.
    pending_url: Mutex<Option<String>>,
}

impl OpenUrlPlugin {
    pub fn new(metadata_service: Arc<WebsiteMetadataService>) -> Self {
        Self {
            metadata_service,
            pending_url: Mutex::new(None),
        }
    }
}

// =========================================================
// Plugin Trait Implementation
// =========================================================

impl Plugin for OpenUrlPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn execute(
        &self,
        _entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        // Both actions consume the stashed URL. We take() once and
        // reuse the value for whichever action was triggered.
        let url = self
            .pending_url
            .lock()
            .expect("pending_url lock not poisoned")
            .take()
            .context("no pending URL to open")?;

        match action_id {
            ActionId::Open => {
                app.opener()
                    .open_url(&url, None::<&str>)
                    .context("open URL in browser")?;
                Ok(PostAction::Dismiss)
            }
            ActionId::Copy => {
                app.clipboard()
                    .write_text(&url)
                    .map_err(|e| anyhow::anyhow!("copy URL to clipboard: {e}"))?;
                Ok(PostAction::Dismiss)
            }
            _ => anyhow::bail!("unsupported action: {action_id:?}"),
        }
    }

    // =========================================================
    // Search — URL Detection + Metadata Enrichment
    //
    // Runs inside spawn_blocking, so blocking on the metadata
    // service is fine. The flow is:
    //
    // 1. Detect a URL in the query string
    // 2. Fetch metadata for the domain (blocking)
    // 3. Build a result entry with enriched title/favicon
    // 4. Stash the full URL for execute()
    //
    // Bare domains that are unreachable are silently suppressed.
    // Explicit URLs always produce a result (globe-alt fallback).
    // =========================================================

    fn search(
        &self,
        query: &str,
        _matched_prefix: Option<&str>,
    ) -> Option<PluginResponse> {
        let detected = detect_url(query)?;

        // Blocking metadata lookup — the service handles caching,
        // negative caching, and network fetches internally.
        let metadata_result = self.metadata_service.get(&detected.domain);

        // Decide whether to show a result and how to present it.
        // Title shows the page title when available, domain otherwise.
        // Subtitle always shows "Open {url}" to communicate the action.
        let (title, icon) = match metadata_result {
            MetadataResult::Found(meta) => {
                let title = meta
                    .title
                    .unwrap_or_else(|| detected.domain.clone());
                (title, meta.favicon)
            }
            MetadataResult::ReachableNoData => (
                detected.domain.clone(),
                EntryIcon::HeroIcon("globe-alt".to_string()),
            ),
            MetadataResult::Unreachable => {
                if detected.has_explicit_scheme {
                    // Explicit URL — show a fallback entry even if
                    // the domain is unreachable. The user deliberately
                    // typed a scheme, so they likely want to open it.
                    (
                        detected.domain.clone(),
                        EntryIcon::HeroIcon("globe-alt".to_string()),
                    )
                } else {
                    // Bare domain that's unreachable — suppress the
                    // result to avoid cluttering results with entries
                    // that lead nowhere.
                    return None;
                }
            }
        };

        // Stash the full URL for execute() to retrieve.
        *self
            .pending_url
            .lock()
            .expect("pending_url lock not poisoned") = Some(detected.full_url.clone());

        let entry = ScoredEntry {
            id: detected.domain,
            title,
            subtitle: Some(format!("Open {}", detected.full_url)),
            icon: Some(icon),
            score: URL_SCORE,
            title_positions: Utf16Positions::empty(),
            subtitle_positions: Utf16Positions::empty(),
            actions: vec![
                Action {
                    id: ActionId::Open,
                    label: "Open in Browser".to_string(),
                    keybinding: None,
                },
                Action {
                    id: ActionId::Copy,
                    label: "Copy URL".to_string(),
                    keybinding: Some(ActionKeybinding {
                        modifiers: vec!["Meta".into()],
                        key: "c".into(),
                    }),
                },
            ],
        };

        Some(PluginResponse::Results(vec![entry]))
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_https_url() {
        let result = detect_url("https://github.com/user/repo").unwrap();
        assert_eq!(result.domain, "github.com");
        assert_eq!(result.full_url, "https://github.com/user/repo");
        assert!(result.has_explicit_scheme);
    }

    #[test]
    fn explicit_http_url() {
        let result = detect_url("http://example.com").unwrap();
        assert_eq!(result.domain, "example.com");
        assert!(result.has_explicit_scheme);
    }

    #[test]
    fn bare_domain_with_valid_tld() {
        let result = detect_url("github.com").unwrap();
        assert_eq!(result.domain, "github.com");
        assert_eq!(result.full_url, "https://github.com");
        assert!(!result.has_explicit_scheme);
    }

    #[test]
    fn bare_domain_with_path() {
        let result = detect_url("github.com/user/repo").unwrap();
        assert_eq!(result.domain, "github.com");
        assert_eq!(result.full_url, "https://github.com/user/repo");
        assert!(!result.has_explicit_scheme);
    }

    #[test]
    fn bare_domain_with_invalid_tld() {
        assert!(detect_url("hello.notarealtld").is_none());
    }

    #[test]
    fn plain_word_no_dot() {
        assert!(detect_url("firefox").is_none());
    }

    #[test]
    fn empty_query() {
        assert!(detect_url("").is_none());
    }

    #[test]
    fn whitespace_only() {
        assert!(detect_url("   ").is_none());
    }

    #[test]
    fn query_with_spaces() {
        // "hello world" is not a valid URL
        assert!(detect_url("hello world").is_none());
    }

    #[test]
    fn ftp_scheme_ignored() {
        // Only http/https are supported
        assert!(detect_url("ftp://files.example.com").is_none());
    }

    #[test]
    fn url_with_query_params() {
        let result = detect_url("https://search.example.com?q=test&lang=en").unwrap();
        assert_eq!(result.domain, "search.example.com");
        assert!(result.has_explicit_scheme);
    }

    #[test]
    fn bare_domain_co_uk_tld() {
        let result = detect_url("example.co.uk").unwrap();
        assert_eq!(result.domain, "example.co.uk");
        assert!(!result.has_explicit_scheme);
    }

    #[test]
    fn url_with_port() {
        let result = detect_url("https://localhost:8080/api").unwrap();
        assert_eq!(result.domain, "localhost");
        assert!(result.has_explicit_scheme);
    }
}
