// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Auth-token resolution.
//!
//! The plugin needs the local daemon's auth token to set the
//! `X-ZT1-Auth` header. ZeroTier writes this token at install
//! time and never rotates it, so resolution runs once on
//! `enable()` and is re-run only when the user edits the
//! manual-paste config field.
//!
//! Resolution order:
//!
//! 1. Per-OS canonical paths, first readable wins.
//! 2. The `manualToken` gadget setting.
//!
//! Filesystem reads go through the manifest-allowlisted
//! `fs::read_file` host import; paths the gadget's manifest
//! does not declare return `permission-denied` and the
//! resolver falls through to the next candidate.

use torchsnap_gadget_sdk::platform::{self, Os};
use torchsnap_gadget_sdk::{fs, settings};

/// Source the resolved token came from. Used by the settings
/// UI to decide whether to disable the manual-paste field
/// (auto-detected) or to enable it with an explanatory info
/// box (manual / failed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    /// Found via one of the per-OS canonical filesystem paths.
    AutoDetected,
    /// Picked up from the user's manually-pasted setting.
    ManualPaste,
    /// Neither path nor manual setting yielded a token.
    None,
}

#[derive(Debug, Clone)]
pub struct ResolvedToken {
    pub token: String,
    pub source: TokenSource,
}

/// Per-OS list of candidate auth-token paths. Order matters:
/// the resolver tries them top-to-bottom, first readable wins.
///
/// `user_config_dir` is the resolved absolute path of the
/// host's user-config directory (`${xdg-config}` —
/// `~/Library/Application Support` on macOS, `~/.config` on
/// Linux, `%APPDATA%` on Windows). Passed in rather than
/// looked up here so the function stays pure and unit-testable
/// without invoking the `paths::resolve` host import.
pub fn candidate_paths(os: &Os, user_config_dir: &str) -> Vec<String> {
    match os {
        Os::Macos => vec![
            "/Library/Application Support/ZeroTier/One/authtoken.secret".to_string(),
            // The official UI deposits a 0644 user-readable
            // copy here; far more often available to the
            // unprivileged gadget than the 0600 system path.
            format!("{user_config_dir}/ZeroTier/One/authtoken.secret"),
        ],
        Os::Linux => vec!["/var/lib/zerotier-one/authtoken.secret".to_string()],
        Os::Windows => {
            vec!["C:\\ProgramData\\ZeroTier\\One\\authtoken.secret".to_string()]
        }
        Os::Other(_) => vec![],
    }
}

/// Try every candidate path; return the first token whose
/// file is readable under the plugin's fs allowlist.
pub fn read_first_readable(paths: &[String]) -> Option<String> {
    for path in paths {
        if let Ok(bytes) = fs::read_file(path) {
            if let Some(token) = bytes_to_token(&bytes) {
                return Some(token);
            }
        }
    }
    None
}

/// Trim and validate a token read from disk. Empty content
/// (zero-length file) is treated as "no token" rather than as
/// a valid empty token.
fn bytes_to_token(bytes: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(bytes).ok()?;
    let trimmed = s.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Top-level resolver. Used by `enable()` and after the user
/// edits the manual-paste setting.
pub fn resolve() -> ResolvedToken {
    let os = platform::current_os();
    let user_config = paths_resolve("${xdg-config}");
    let candidates = candidate_paths(&os, &user_config);
    if let Some(token) = read_first_readable(&candidates) {
        return ResolvedToken {
            token,
            source: TokenSource::AutoDetected,
        };
    }

    let manual: String = settings::get_or_else("manualToken", String::new);
    let trimmed = manual.trim();
    if !trimmed.is_empty() {
        return ResolvedToken {
            token: trimmed.to_string(),
            source: TokenSource::ManualPaste,
        };
    }

    ResolvedToken {
        token: String::new(),
        source: TokenSource::None,
    }
}

/// Wrapper around `paths::resolve` with a graceful fallback —
/// an unrecognized variable returns the input unchanged so the
/// caller still has a string to feed into `fs::read_file` (the
/// fs layer will reject the unsubstituted path with
/// `permission-denied` and the resolver tries the next
/// candidate).
fn paths_resolve(template: &str) -> String {
    torchsnap_gadget_sdk::paths::resolve(template).unwrap_or_else(|_| template.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_candidates_include_system_and_user_paths() {
        let paths = candidate_paths(
            &Os::Macos,
            "/Users/test/Library/Application Support",
        );
        assert_eq!(paths.len(), 2);
        assert_eq!(
            paths[0],
            "/Library/Application Support/ZeroTier/One/authtoken.secret"
        );
        assert_eq!(
            paths[1],
            "/Users/test/Library/Application Support/ZeroTier/One/authtoken.secret"
        );
    }

    #[test]
    fn linux_candidate_is_daemon_path_only() {
        let paths = candidate_paths(&Os::Linux, "/home/test/.config");
        assert_eq!(
            paths,
            vec!["/var/lib/zerotier-one/authtoken.secret".to_string()]
        );
    }

    #[test]
    fn windows_candidate_is_programdata_path() {
        let paths = candidate_paths(
            &Os::Windows,
            "C:\\Users\\test\\AppData\\Roaming",
        );
        assert_eq!(paths.len(), 1);
        assert!(paths[0].contains("ProgramData"));
    }

    #[test]
    fn other_os_yields_no_candidates() {
        let paths = candidate_paths(&Os::Other("haiku".into()), "/anywhere");
        assert!(paths.is_empty());
    }

    #[test]
    fn bytes_to_token_strips_surrounding_whitespace() {
        let token = bytes_to_token(b"  abc123\n").expect("non-empty");
        assert_eq!(token, "abc123");
    }

    #[test]
    fn bytes_to_token_rejects_empty_and_whitespace_only_files() {
        assert!(bytes_to_token(b"").is_none());
        assert!(bytes_to_token(b"   \n\t").is_none());
    }

    #[test]
    fn bytes_to_token_rejects_non_utf8() {
        assert!(bytes_to_token(&[0xff, 0xfe, 0x00]).is_none());
    }
}
