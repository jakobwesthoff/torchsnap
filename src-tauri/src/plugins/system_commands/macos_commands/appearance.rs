// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Appearance Commands (macOS)
//
// Switch between dark and light mode. Only the relevant
// command appears based on the current system appearance.
//
// The current dark-mode state is cached with a TTL to avoid
// an osascript roundtrip on every keystroke (since
// `is_available` is called during each search). The cache
// is invalidated after toggling so the next search reflects
// the new state immediately.
// =========================================================

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::platform::macos::osascript;
use crate::plugins::system_commands::SystemCommand;
use crate::search::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction};

// =========================================================
// Dark Mode State Cache
// =========================================================

const CACHE_TTL: Duration = Duration::from_secs(180);

struct DarkModeState {
    is_dark: bool,
    fetched_at: Instant,
}

/// Global cache shared between `SwitchToDarkMode` and `SwitchToLightMode`.
fn cache() -> &'static Mutex<Option<DarkModeState>> {
    static CACHE: OnceLock<Mutex<Option<DarkModeState>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Query the current dark mode state, returning a cached value if fresh.
fn is_dark_mode() -> bool {
    let mut guard = cache().lock().expect("dark mode cache lock poisoned");

    if let Some(ref state) = *guard
        && state.fetched_at.elapsed() < CACHE_TTL
    {
        return state.is_dark;
    }

    // Cache miss or expired — query the system.
    let is_dark = osascript::eval(
        r#"tell application "System Events" to tell appearance preferences to get dark mode"#,
    )
    .map(|s| s == "true")
    .unwrap_or(false);

    *guard = Some(DarkModeState {
        is_dark,
        fetched_at: Instant::now(),
    });

    is_dark
}

/// Invalidate the cache so the next `is_dark_mode()` call re-queries.
fn invalidate_cache() {
    *cache().lock().expect("dark mode cache lock poisoned") = None;
}

// =========================================================
// Switch to Dark Mode
// =========================================================

pub struct SwitchToDarkMode;

impl SystemCommand for SwitchToDarkMode {
    fn id(&self) -> &str {
        "switch-to-dark-mode"
    }

    fn is_available(&self) -> bool {
        !is_dark_mode()
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Switch to Dark Mode".into(),
            subtitle: Some("Change system appearance to dark".into()),
            icon: Some(EntryIcon::HeroIcon("moon".into())),
            keywords: vec![
                "dark mode".into(),
                "dark".into(),
                "appearance".into(),
            ],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Switch".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        // Always toggle rather than set, so even a stale cache
        // produces correct behavior — the system flips to the
        // opposite of whatever it currently is.
        osascript::run(
            r#"tell application "System Events" to tell appearance preferences to set dark mode to not dark mode"#,
        )?;
        invalidate_cache();
        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Switch to Light Mode
// =========================================================

pub struct SwitchToLightMode;

impl SystemCommand for SwitchToLightMode {
    fn id(&self) -> &str {
        "switch-to-light-mode"
    }

    fn is_available(&self) -> bool {
        is_dark_mode()
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Switch to Light Mode".into(),
            subtitle: Some("Change system appearance to light".into()),
            icon: Some(EntryIcon::HeroIcon("sun".into())),
            keywords: vec![
                "light mode".into(),
                "light".into(),
                "appearance".into(),
            ],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Switch".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        osascript::run(
            r#"tell application "System Events" to tell appearance preferences to set dark mode to not dark mode"#,
        )?;
        invalidate_cache();
        Ok(PostAction::Dismiss)
    }
}
