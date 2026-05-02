// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// =========================================================
// Frontend
// =========================================================

/// Frontend component declarations. The host extracts bundled
/// JS files and loads them via dynamic `import()` in the
/// appropriate webview.
///
/// Multi-word field names use dual `#[serde(rename(...))]`
/// attributes for TOML kebab-case ↔ JSON camelCase conversion.
/// See the `Manifest` module comment for the full naming strategy.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrontendDef {
    /// Path to the ES module bundle loaded in the launcher
    /// webview. Contains view and inline-view components as
    /// named exports.
    #[serde(rename(deserialize = "launcher-bundle", serialize = "launcherBundle"))]
    pub launcher_bundle: Option<String>,

    /// Path to the ES module bundle loaded in the settings
    /// webview. Contains the settings component as a named
    /// export.
    #[serde(rename(deserialize = "settings-bundle", serialize = "settingsBundle"))]
    pub settings_bundle: Option<String>,

    /// Maps view names to named exports from `launcher_bundle`.
    /// (e.g., `{ "history" = "ClipboardView" }`).
    #[serde(default)]
    pub views: HashMap<String, String>,

    /// Maps inline view names to named exports from
    /// `launcher_bundle`.
    #[serde(
        default,
        rename(deserialize = "inline-views", serialize = "inlineViews")
    )]
    pub inline_views: HashMap<String, String>,

    /// Path to the CSS file loaded alongside the launcher
    /// bundle. Served via `torchsnap-plugin://` and scoped to
    /// the plugin's container with `@scope`.
    #[serde(
        default,
        rename(deserialize = "launcher-css", serialize = "launcherCss")
    )]
    pub launcher_css: Option<String>,

    /// Path to the CSS file loaded alongside the settings
    /// bundle.
    #[serde(
        default,
        rename(deserialize = "settings-css", serialize = "settingsCss")
    )]
    pub settings_css: Option<String>,

    /// Settings panel component declaration.
    pub settings: Option<FrontendSettingsDef>,
}

/// Settings component reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrontendSettingsDef {
    /// Named export from `settings_bundle` that provides
    /// the settings React component.
    pub component: String,
}
