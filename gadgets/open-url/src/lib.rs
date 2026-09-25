// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Open URL Gadget (WASM)
//
// Detects URLs in the search query — both explicit
// `http(s)://...` and bare domains like `example.com` — and
// surfaces a single result that opens the URL in the default
// browser. Bare domains are validated against the Public
// Suffix List via the `addr` crate so random dot-separated
// words don't produce false positives.
//
// Metadata enrichment (page title, favicon) goes through the
// host's `website-metadata` interface in `Blocking` mode,
// mirroring the pre-WASM native gadget's behaviour. The
// asymmetric explicit-vs-bare handling on lookup outcomes is
// load-bearing: bare domains that the host can't reach are
// suppressed entirely, while explicit-scheme URLs always
// produce a result.
//
// See `todos/gadgets/open-url/01kqf2tcbz6n2m4kn8299ddfnw-open-url-asymmetric-metadata-mode.md`
// for the deferred follow-up that splits the lookup mode by
// URL kind once we have UX data to motivate it.
// =========================================================

use serde::{Deserialize, Serialize};
use torchsnap_gadget_sdk::prelude::*;
use torchsnap_gadget_sdk::website_metadata::Metadata;

/// Same priority tier as bangs. Lower than a perfect title
/// match plus heavy frecency, high enough to stay near the top
/// when present.
const URL_SCORE: u32 = 500;

struct OpenUrlPlugin;
define_gadget!(OpenUrlPlugin);

// =========================================================
// URL detection
// =========================================================

/// A URL parsed from the user's search query.
struct DetectedUrl {
    /// Full URL including scheme.
    full_url: String,
    /// Bare host (e.g. `github.com`), used for the metadata
    /// lookup and for frecency tracking via the entry id.
    domain: String,
    /// `true` if the user typed an explicit `http(s)://`.
    /// Drives the asymmetric handling of `Unreachable` in
    /// `search()`.
    has_explicit_scheme: bool,
}

/// Detect a URL in the query, returning `None` for inputs
/// that don't look like one.
///
/// Two parse paths:
///
/// 1. **Explicit scheme.** `url::Url::parse` accepts the
///    query as-is. We require `http`/`https` and a host.
///    No PSL check — the user committed to the URL by
///    typing the scheme, even oddities like `localhost:8080`
///    or intranet hostnames are valid.
/// 2. **Bare domain.** Prepend `https://`, parse, extract
///    the host, then validate via
///    `addr::parse_domain_name(host).has_known_suffix()`.
///    The PSL check is what filters typo-words like
///    `hello.notarealtld` while still accepting
///    `example.co.uk` and other multi-label TLDs.
fn detect_url(query: &str) -> Option<DetectedUrl> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(parsed) = url::Url::parse(trimmed) {
        let scheme = parsed.scheme();
        if (scheme == "http" || scheme == "https") && parsed.host_str().is_some() {
            let domain = parsed
                .host_str()
                .expect("host presence checked above")
                .to_string();
            return Some(DetectedUrl {
                full_url: parsed.to_string(),
                domain,
                has_explicit_scheme: true,
            });
        }
    }

    let candidate = format!("https://{trimmed}");
    let parsed = url::Url::parse(&candidate).ok()?;
    let host = parsed.host_str()?;

    let domain_name = addr::parse_domain_name(host).ok()?;
    if !domain_name.has_known_suffix() {
        return None;
    }

    // Reject inputs that *are* a public suffix with nothing
    // below it. The PSL contains single-label gTLDs (`google`,
    // `app`, `dev`, …) and multi-label suffixes (`co.uk`),
    // and `parse_domain_name` happily accepts a query that
    // matches one of them — so `google` and `google.` would
    // both pass `has_known_suffix` despite having no
    // registrable name. `root()` returns `Some` only when
    // there is at least one label below the suffix, which is
    // the actual signal we want for "this is a real bare
    // domain candidate".
    domain_name.root()?;

    Some(DetectedUrl {
        full_url: candidate,
        domain: host.to_string(),
        has_explicit_scheme: false,
    })
}

// =========================================================
// Lifecycle — stateless, query-only gadget
// =========================================================

impl LifecycleGuest for OpenUrlPlugin {
    fn enable() -> Result<(), String> {
        logging::log(logging::LogLevel::Info, "Open URL enabled", &[], None);
        Ok(())
    }

    fn disable() {
        logging::log(logging::LogLevel::Info, "Open URL disabled", &[], None);
    }

    fn on_setting_changed(_key: String, _value: String) {
        // No user-tunable settings.
    }
}

// =========================================================
// Search
// =========================================================

/// What a URL result's actions do. Both carry the full URL.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum Command {
    OpenUrl(String),
    CopyUrl(String),
}

/// Enter opens the URL, Cmd+C copies it.
fn url_actions(url: &str) -> Actions<Command> {
    Actions::new()
        .primary("Open in Browser", Command::OpenUrl(url.to_string()))
        .copy(Action::labeled(
            "Copy URL",
            Command::CopyUrl(url.to_string()),
        ))
}

// Query-only gadget: `entries()` keeps the trait's empty default.
impl Search for OpenUrlPlugin {
    type Command = Command;

    fn search(query: String, _matched_prefix: Option<String>) -> SearchResponse<Command> {
        let Some(detected) = detect_url(&query) else {
            return SearchResponse::Nothing;
        };

        // `Blocking` mode is correct here: for bare domains
        // the entry's existence depends on whether the host
        // can reach the domain, so the lookup is part of
        // result validity, not just decoration. Showing /
        // hiding the entry must not flicker between
        // keystrokes.
        let (title, icon) = match website_metadata::lookup_blocking(&detected.domain) {
            Metadata::Found(entry) => {
                let title = entry.title.unwrap_or_else(|| detected.domain.clone());
                (title, entry.favicon)
            }
            Metadata::NoData => (
                // Reachable but no metadata — show the fallback in
                // both branches.
                detected.domain.clone(),
                EntryIcon::HeroIcon("globe-alt".to_string()),
            ),
            Metadata::Unreachable => {
                if detected.has_explicit_scheme {
                    // User typed a scheme — show fallback even when
                    // unreachable.
                    (
                        detected.domain.clone(),
                        EntryIcon::HeroIcon("globe-alt".to_string()),
                    )
                } else {
                    // Bare domain that's unreachable — suppress so
                    // typo-words ending in `.com` don't litter
                    // results with entries that go nowhere.
                    return SearchResponse::Nothing;
                }
            }
            Metadata::Pending => unreachable!("lookup_blocking never returns Pending"),
        };

        let entry = ScoredEntry {
            id: detected.domain,
            title,
            subtitle: Some(format!("Open {}", detected.full_url)),
            icon: Some(icon),
            score: URL_SCORE,
            title_highlight_positions: Vec::new(),
            subtitle_highlight_positions: Vec::new(),
            actions: url_actions(&detected.full_url),
        };

        SearchResponse::Results(vec![entry])
    }

    fn execute(command: Command) -> Result<PostAction, String> {
        match command {
            Command::OpenUrl(url) => {
                opener::open_url(&url).map_err(|e| format!("open URL: {e:?}"))?;
            }
            Command::CopyUrl(url) => {
                clipboard::write_text(&url).map_err(|e| format!("copy URL to clipboard: {e}"))?;
            }
        }
        Ok(PostAction::Dismiss)
    }
}

impl_noop_messaging!(OpenUrlPlugin);
impl_noop_tasks!(OpenUrlPlugin);

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_actions_open_on_enter_and_copy_on_cmd_c() {
        let actions = url_actions("https://example.com/path");
        assert_eq!(
            actions.primary,
            Some(Action::labeled(
                "Open in Browser",
                Command::OpenUrl("https://example.com/path".into())
            ))
        );
        assert_eq!(
            actions.copy,
            Some(Action::labeled(
                "Copy URL",
                Command::CopyUrl("https://example.com/path".into())
            ))
        );
        assert_eq!(actions.iter().count(), 2);
    }

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
    fn bare_suffix_alone_rejected() {
        // `google` is a single-label gTLD on the PSL — the suffix
        // matches but there is no registrable name below it.
        assert!(detect_url("google").is_none());
    }

    #[test]
    fn bare_suffix_with_trailing_dot_rejected() {
        // `addr` strips the trailing dot before parsing, so this
        // collapses to the same case as `bare_suffix_alone_rejected`.
        assert!(detect_url("google.").is_none());
    }

    #[test]
    fn bare_multilabel_suffix_alone_rejected() {
        // `co.uk` is itself a public suffix; without a registrable
        // label below it there is no real domain to open.
        assert!(detect_url("co.uk").is_none());
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
        assert!(detect_url("hello world").is_none());
    }

    #[test]
    fn ftp_scheme_ignored() {
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
