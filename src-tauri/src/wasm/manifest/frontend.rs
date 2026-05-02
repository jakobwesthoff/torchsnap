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

#[cfg(test)]
mod tests {
    use crate::wasm::manifest::Manifest;
    use crate::wasm::manifest::test_helpers::minimal;

    // =====================================================
    // Frontend
    // =====================================================

    #[test]
    fn frontend_omitted_entirely() {
        let m = Manifest::parse(&minimal("")).expect("should parse");
        assert!(m.frontend.is_none());
    }

    #[test]
    fn frontend_launcher_only() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-bundle = "frontend/launcher.js"

            [frontend.views]
            picker = "EmojiGrid"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        let fe = m.frontend.as_ref().expect("frontend");
        assert_eq!(fe.launcher_bundle.as_deref(), Some("frontend/launcher.js"));
        assert!(fe.settings_bundle.is_none());
        assert_eq!(
            fe.views.get("picker").map(String::as_str),
            Some("EmojiGrid")
        );
        assert!(fe.inline_views.is_empty());
        assert!(fe.settings.is_none());
    }

    #[test]
    fn frontend_settings_only() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [frontend]
            settings-bundle = "frontend/settings.js"

            [frontend.settings]
            component = "BangsSettings"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        let fe = m.frontend.as_ref().expect("frontend");
        assert!(fe.launcher_bundle.is_none());
        assert_eq!(fe.settings_bundle.as_deref(), Some("frontend/settings.js"));
        assert!(fe.views.is_empty());
        assert_eq!(
            fe.settings.as_ref().map(|s| s.component.as_str()),
            Some("BangsSettings")
        );
    }

    #[test]
    fn frontend_with_views_and_inline_views() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-bundle = "frontend/launcher.js"

            [frontend.views]
            history = "HistoryView"
            detail = "DetailView"

            [frontend.inline-views]
            result = "InlineResult"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        let fe = m.frontend.as_ref().expect("frontend");
        assert_eq!(fe.views.len(), 2);
        assert_eq!(
            fe.views.get("history").map(String::as_str),
            Some("HistoryView")
        );
        assert_eq!(
            fe.views.get("detail").map(String::as_str),
            Some("DetailView")
        );
        assert_eq!(fe.inline_views.len(), 1);
        assert_eq!(
            fe.inline_views.get("result").map(String::as_str),
            Some("InlineResult")
        );
    }

    // =====================================================
    // Frontend: cross-field validation
    // =====================================================

    #[test]
    fn reject_views_without_launcher_bundle() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Missing launcher bundle"
            version = "0.1.0"
            wasm = "bad.wasm"
            icon = "heroicons:beaker"

            [frontend]
            settings-bundle = "frontend/settings.js"

            [frontend.views]
            picker = "SomeView"
        "#;

        let err = Manifest::parse(toml).unwrap_err();
        assert!(
            err.to_string().contains("launcher-bundle"),
            "error should mention launcher-bundle: {err}"
        );
    }

    #[test]
    fn reject_inline_views_without_launcher_bundle() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Missing launcher bundle"
            version = "0.1.0"
            wasm = "bad.wasm"
            icon = "heroicons:beaker"

            [frontend]

            [frontend.inline-views]
            result = "SomeInline"
        "#;

        let err = Manifest::parse(toml).unwrap_err();
        assert!(
            err.to_string().contains("launcher-bundle"),
            "error should mention launcher-bundle: {err}"
        );
    }

    #[test]
    fn reject_settings_component_without_settings_bundle() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Missing settings bundle"
            version = "0.1.0"
            wasm = "bad.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-bundle = "frontend/launcher.js"

            [frontend.settings]
            component = "SomeSettings"
        "#;

        let err = Manifest::parse(toml).unwrap_err();
        assert!(
            err.to_string().contains("settings-bundle"),
            "error should mention settings-bundle: {err}"
        );
    }

    #[test]
    fn accept_empty_frontend_section() {
        // A [frontend] section with no bundles and no components
        // is valid — it's just a no-op.
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [frontend]
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        let fe = m.frontend.as_ref().expect("frontend exists");
        assert!(fe.launcher_bundle.is_none());
        assert!(fe.settings_bundle.is_none());
        assert!(fe.views.is_empty());
        assert!(fe.inline_views.is_empty());
        assert!(fe.settings.is_none());
    }

    // =====================================================
    // Frontend: CSS fields
    // =====================================================

    #[test]
    fn frontend_with_launcher_css() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-bundle = "frontend/launcher.js"
            launcher-css = "frontend/launcher.css"

            [frontend.views]
            echo = "Echo"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        let fe = m.frontend.as_ref().expect("frontend");
        assert_eq!(fe.launcher_css.as_deref(), Some("frontend/launcher.css"));
    }

    #[test]
    fn frontend_with_settings_css() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [frontend]
            settings-bundle = "frontend/settings.js"
            settings-css = "frontend/settings.css"

            [frontend.settings]
            component = "TestSettings"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        let fe = m.frontend.as_ref().expect("frontend");
        assert_eq!(fe.settings_css.as_deref(), Some("frontend/settings.css"));
    }

    #[test]
    fn frontend_with_both_css_fields() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-bundle = "frontend/launcher.js"
            launcher-css = "frontend/launcher.css"
            settings-bundle = "frontend/settings.js"
            settings-css = "frontend/settings.css"

            [frontend.views]
            echo = "Echo"

            [frontend.settings]
            component = "TestSettings"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        let fe = m.frontend.as_ref().expect("frontend");
        assert_eq!(fe.launcher_css.as_deref(), Some("frontend/launcher.css"));
        assert_eq!(fe.settings_css.as_deref(), Some("frontend/settings.css"));
    }

    #[test]
    fn frontend_css_fields_default_to_none() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-bundle = "frontend/launcher.js"

            [frontend.views]
            echo = "Echo"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        let fe = m.frontend.as_ref().expect("frontend");
        assert!(fe.launcher_css.is_none());
        assert!(fe.settings_css.is_none());
    }

    #[test]
    fn reject_launcher_css_without_launcher_bundle() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "CSS without bundle"
            version = "0.1.0"
            wasm = "bad.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-css = "frontend/launcher.css"
        "#;

        let err = Manifest::parse(toml).unwrap_err();
        assert!(
            err.to_string().contains("launcher-css") && err.to_string().contains("launcher-bundle"),
            "error should mention both fields: {err}"
        );
    }
}
