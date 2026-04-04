// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Manifest
//
// Typed representation of the `manifest.toml` that every
// `.torchsnap` plugin archive (or development directory)
// must contain. The manifest carries all metadata the host
// needs to display, configure, and load the plugin without
// instantiating the WASM module.
// =========================================================

use std::collections::HashMap;

use serde::Deserialize;

// =========================================================
// Top-Level Manifest
// =========================================================

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub plugin: PluginMeta,

    /// Plugin-specific settings defaults. Each key-value pair
    /// is applied to the settings store on first load (existing
    /// user values are never overwritten). The values are
    /// arbitrary JSON-compatible types.
    #[serde(default)]
    pub settings: HashMap<String, toml::Value>,

    /// Global keyboard shortcuts the plugin wants to register.
    /// Keys are stable shortcut IDs (e.g., `"open-clipboard"`),
    /// values describe the shortcut.
    #[serde(default)]
    pub shortcuts: HashMap<String, ShortcutDef>,

    /// Frontend component declarations. Omitted when the
    /// plugin has no UI.
    pub frontend: Option<FrontendDef>,
}

// =========================================================
// Plugin Identity & Core Properties
// =========================================================

#[derive(Debug, Clone, Deserialize)]
pub struct PluginMeta {
    /// Stable identifier. Lowercase alphanumeric and hyphens
    /// only (e.g., `"clipboard-manager"`). Used as the key
    /// for settings namespaces, frecency storage, data
    /// directories, and frontend registry lookups.
    pub id: PluginId,

    /// Human-readable display name shown in the settings
    /// sidebar and section header.
    pub name: String,

    /// Short description shown in the settings section header
    /// below the plugin name.
    pub description: String,

    /// Plugin version (e.g., `"0.1.0"`). For display and
    /// future update checking.
    pub version: String,

    /// Path to the WASM component binary within the archive
    /// or directory (e.g., `"hello_world.wasm"`).
    pub wasm: String,

    /// Plugin icon for the settings sidebar and section header.
    ///
    /// Two formats are supported:
    /// - `"heroicons:<name>"` — resolved to a HeroIcon component
    /// - Any other string — treated as a path to a WebP image
    ///   within the plugin archive/directory
    pub icon: PluginIcon,

    /// Optional search prefixes for exclusive query routing
    /// (e.g., `[":"]` for the emoji picker). Omit for catalog
    /// or always-on query plugins.
    #[serde(default)]
    pub prefixes: Vec<String>,
}

// =========================================================
// Plugin ID (validated newtype)
// =========================================================

/// A validated plugin identifier. Lowercase ASCII alphanumeric
/// characters and hyphens only, must not be empty, must not
/// start or end with a hyphen.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginId(String);

impl PluginId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PluginId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PluginId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        validate_plugin_id(&raw).map_err(serde::de::Error::custom)?;
        Ok(PluginId(raw))
    }
}

fn validate_plugin_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("plugin id must not be empty".into());
    }
    if id.starts_with('-') || id.ends_with('-') {
        return Err(format!(
            "plugin id `{id}` must not start or end with a hyphen"
        ));
    }
    if let Some(ch) = id.chars().find(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit() && *c != '-') {
        return Err(format!(
            "plugin id `{id}` contains invalid character `{ch}` — \
             only lowercase alphanumeric and hyphens are allowed"
        ));
    }
    Ok(())
}

// =========================================================
// Plugin Icon
// =========================================================

/// Either a HeroIcon reference or a path to an image file
/// within the plugin source (archive or directory).
///
/// The image variant stores a path relative to the plugin
/// root — it is not a filesystem path. At load time the host
/// reads the image bytes via `PluginSource::read_file`.
#[derive(Debug, Clone)]
pub enum PluginIcon {
    /// A HeroIcon name (e.g., `"clipboard-document-list"`).
    HeroIcon(String),

    /// A relative path to a WebP image within the plugin
    /// source (e.g., `"assets/icon.webp"`). MUST be read via
    /// `PluginSource::read_file` — this is never a filesystem
    /// path.
    Asset(String),
}

impl<'de> Deserialize<'de> for PluginIcon {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        if let Some(name) = raw.strip_prefix("heroicons:") {
            if name.is_empty() {
                return Err(serde::de::Error::custom(
                    "heroicons: prefix requires an icon name",
                ));
            }
            Ok(PluginIcon::HeroIcon(name.to_string()))
        } else {
            Ok(PluginIcon::Asset(raw))
        }
    }
}

// =========================================================
// Shortcuts
// =========================================================

/// Declaration of a single global keyboard shortcut.
#[derive(Debug, Clone, Deserialize)]
pub struct ShortcutDef {
    /// Human-readable label (e.g., "Open Clipboard History").
    pub label: String,

    /// Default key combination (e.g., "CmdOrCtrl+Shift+V").
    /// The host stores the actual binding — this is only used
    /// as the initial default.
    pub default: String,
}

// =========================================================
// Frontend
// =========================================================

/// Frontend component declarations. The host extracts bundled
/// JS files and loads them via dynamic `import()` in the
/// appropriate webview.
#[derive(Debug, Clone, Deserialize)]
pub struct FrontendDef {
    /// Path to the ES module bundle loaded in the launcher
    /// webview. Contains view and inline-view components as
    /// named exports.
    #[serde(rename = "launcher-bundle")]
    pub launcher_bundle: Option<String>,

    /// Path to the ES module bundle loaded in the settings
    /// webview. Contains the settings component as a named
    /// export.
    #[serde(rename = "settings-bundle")]
    pub settings_bundle: Option<String>,

    /// Maps view names to named exports from `launcher_bundle`.
    /// (e.g., `{ "history" = "ClipboardView" }`).
    #[serde(default)]
    pub views: HashMap<String, String>,

    /// Maps inline view names to named exports from
    /// `launcher_bundle`.
    #[serde(default, rename = "inline-views")]
    pub inline_views: HashMap<String, String>,

    /// Path to the CSS file loaded alongside the launcher
    /// bundle. Served via `torchsnap-plugin://` and scoped to
    /// the plugin's container with `@scope`.
    #[serde(default, rename = "launcher-css")]
    pub launcher_css: Option<String>,

    /// Path to the CSS file loaded alongside the settings
    /// bundle.
    #[serde(default, rename = "settings-css")]
    pub settings_css: Option<String>,

    /// Settings panel component declaration.
    pub settings: Option<FrontendSettingsDef>,
}

/// Settings component reference.
#[derive(Debug, Clone, Deserialize)]
pub struct FrontendSettingsDef {
    /// Named export from `settings_bundle` that provides
    /// the settings React component.
    pub component: String,
}

// =========================================================
// Parsing
// =========================================================

impl Manifest {
    /// Parse a manifest from TOML source text.
    pub fn parse(toml_source: &str) -> anyhow::Result<Self> {
        let manifest: Manifest =
            toml::from_str(toml_source).map_err(|e| anyhow::anyhow!("invalid manifest: {e}"))?;

        // Validate that views/inline-views reference a launcher bundle.
        if let Some(ref frontend) = manifest.frontend {
            let has_launcher_components =
                !frontend.views.is_empty() || !frontend.inline_views.is_empty();
            if has_launcher_components && frontend.launcher_bundle.is_none() {
                anyhow::bail!(
                    "manifest declares views or inline-views but no launcher-bundle"
                );
            }

            if frontend.launcher_css.is_some() && frontend.launcher_bundle.is_none() {
                anyhow::bail!(
                    "manifest declares launcher-css but no launcher-bundle"
                );
            }

            if frontend.settings.is_some() && frontend.settings_bundle.is_none() {
                anyhow::bail!(
                    "manifest declares a settings component but no settings-bundle"
                );
            }

            if frontend.settings_css.is_some() && frontend.settings_bundle.is_none() {
                anyhow::bail!(
                    "manifest declares settings-css but no settings-bundle"
                );
            }
        }

        Ok(manifest)
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================
    // Helpers
    // =====================================================

    /// Build a minimal valid manifest TOML, optionally
    /// appending extra sections.
    fn minimal(extra: &str) -> String {
        format!(
            r#"
            [plugin]
            id = "test-plugin"
            name = "Test Plugin"
            description = "A test plugin"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"
            {extra}
            "#
        )
    }

    // =====================================================
    // Manifest: happy paths
    // =====================================================

    #[test]
    fn parse_minimal_manifest() {
        let m = Manifest::parse(&minimal("")).expect("should parse");
        assert_eq!(m.plugin.id.as_str(), "test-plugin");
        assert_eq!(m.plugin.name, "Test Plugin");
        assert_eq!(m.plugin.description, "A test plugin");
        assert_eq!(m.plugin.version, "0.1.0");
        assert_eq!(m.plugin.wasm, "test.wasm");
        assert!(matches!(m.plugin.icon, PluginIcon::HeroIcon(ref n) if n == "beaker"));
        assert!(m.plugin.prefixes.is_empty());
        assert!(m.settings.is_empty());
        assert!(m.shortcuts.is_empty());
        assert!(m.frontend.is_none());
    }

    #[test]
    fn parse_full_manifest() {
        let toml = r#"
            [plugin]
            id = "clipboard-manager"
            name = "Clipboard Manager"
            description = "Clipboard history with search and paste"
            version = "2.3.1"
            wasm = "clipboard_manager.wasm"
            icon = "heroicons:clipboard-document-list"
            prefixes = [":"]

            [settings]
            retentionDays = 90
            bringToFrontOnPaste = true

            [shortcuts]
            open-clipboard = { label = "Open Clipboard History", default = "CmdOrCtrl+Shift+V" }

            [frontend]
            launcher-bundle = "frontend/launcher.js"
            settings-bundle = "frontend/settings.js"

            [frontend.views]
            history = "ClipboardView"

            [frontend.inline-views]
            result = "ClipboardInline"

            [frontend.settings]
            component = "ClipboardSettings"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        assert_eq!(m.plugin.id.as_str(), "clipboard-manager");
        assert_eq!(m.plugin.version, "2.3.1");
        assert_eq!(m.plugin.prefixes, vec![":"]);

        assert_eq!(
            m.settings.get("retentionDays").and_then(|v| v.as_integer()),
            Some(90)
        );
        assert_eq!(
            m.settings
                .get("bringToFrontOnPaste")
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        let shortcut = m.shortcuts.get("open-clipboard").expect("shortcut exists");
        assert_eq!(shortcut.label, "Open Clipboard History");
        assert_eq!(shortcut.default, "CmdOrCtrl+Shift+V");

        let fe = m.frontend.as_ref().expect("frontend exists");
        assert_eq!(fe.launcher_bundle.as_deref(), Some("frontend/launcher.js"));
        assert_eq!(fe.settings_bundle.as_deref(), Some("frontend/settings.js"));
        assert_eq!(
            fe.views.get("history").map(String::as_str),
            Some("ClipboardView")
        );
        assert_eq!(
            fe.inline_views.get("result").map(String::as_str),
            Some("ClipboardInline")
        );
        assert_eq!(
            fe.settings.as_ref().map(|s| s.component.as_str()),
            Some("ClipboardSettings")
        );
    }

    // =====================================================
    // Plugin ID validation
    // =====================================================

    #[test]
    fn accept_valid_plugin_ids() {
        let valid = [
            "a",
            "hello-world",
            "my-plugin-2",
            "x1",
            "abc",
            "a-b-c-d",
            "plugin123",
        ];
        for id in valid {
            assert!(
                validate_plugin_id(id).is_ok(),
                "should accept `{id}`"
            );
        }
    }

    #[test]
    fn reject_empty_plugin_id() {
        let err = validate_plugin_id("").unwrap_err();
        assert!(err.contains("empty"), "error: {err}");
    }

    #[test]
    fn reject_leading_hyphen() {
        let err = validate_plugin_id("-leading").unwrap_err();
        assert!(err.contains("start or end with a hyphen"), "error: {err}");
    }

    #[test]
    fn reject_trailing_hyphen() {
        let err = validate_plugin_id("trailing-").unwrap_err();
        assert!(err.contains("start or end with a hyphen"), "error: {err}");
    }

    #[test]
    fn reject_uppercase_in_plugin_id() {
        let err = validate_plugin_id("Upper").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn reject_underscore_in_plugin_id() {
        let err = validate_plugin_id("under_score").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn reject_spaces_in_plugin_id() {
        let err = validate_plugin_id("has space").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn reject_dots_in_plugin_id() {
        let err = validate_plugin_id("my.plugin").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn reject_unicode_in_plugin_id() {
        let err = validate_plugin_id("plügin").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn plugin_id_display_and_as_str() {
        let toml = minimal("");
        let m = Manifest::parse(&toml).expect("should parse");
        assert_eq!(m.plugin.id.as_str(), "test-plugin");
        assert_eq!(m.plugin.id.to_string(), "test-plugin");
    }

    #[test]
    fn plugin_id_equality() {
        let toml = minimal("");
        let m1 = Manifest::parse(&toml).expect("parse 1");
        let m2 = Manifest::parse(&toml).expect("parse 2");
        assert_eq!(m1.plugin.id, m2.plugin.id);
    }

    // =====================================================
    // Icon parsing
    // =====================================================

    #[test]
    fn parse_heroicon() {
        let toml = minimal("");
        let m = Manifest::parse(&toml).expect("should parse");
        match &m.plugin.icon {
            PluginIcon::HeroIcon(name) => assert_eq!(name, "beaker"),
            PluginIcon::Asset(p) => panic!("expected HeroIcon, got Asset({p})"),
        }
    }

    #[test]
    fn parse_asset_icon_simple_path() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "assets/icon.webp"
        "#;
        let m = Manifest::parse(toml).expect("should parse");
        match &m.plugin.icon {
            PluginIcon::Asset(path) => assert_eq!(path, "assets/icon.webp"),
            PluginIcon::HeroIcon(n) => panic!("expected Asset, got HeroIcon({n})"),
        }
    }

    #[test]
    fn parse_asset_icon_bare_filename() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "icon.webp"
        "#;
        let m = Manifest::parse(toml).expect("should parse");
        assert!(matches!(&m.plugin.icon, PluginIcon::Asset(p) if p == "icon.webp"));
    }

    #[test]
    fn reject_heroicons_prefix_without_name() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(
            err.to_string().contains("icon name"),
            "error should mention missing icon name: {err}"
        );
    }

    // =====================================================
    // Prefixes
    // =====================================================

    #[test]
    fn prefixes_default_to_empty() {
        let m = Manifest::parse(&minimal("")).expect("should parse");
        assert!(m.plugin.prefixes.is_empty());
    }

    #[test]
    fn parse_single_prefix() {
        let m = Manifest::parse(&minimal(r#"prefixes = [":"]"#)).expect("should parse");
        assert_eq!(m.plugin.prefixes, vec![":"]);
    }

    #[test]
    fn parse_multiple_prefixes() {
        let m = Manifest::parse(&minimal(r#"prefixes = [":", "=", "!"]"#))
            .expect("should parse");
        assert_eq!(m.plugin.prefixes, vec![":", "=", "!"]);
    }

    // =====================================================
    // Settings defaults
    // =====================================================

    #[test]
    fn settings_default_to_empty() {
        let m = Manifest::parse(&minimal("")).expect("should parse");
        assert!(m.settings.is_empty());
    }

    #[test]
    fn parse_settings_with_mixed_types() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [settings]
            enabled = true
            maxItems = 100
            ratio = 0.5
            label = "hello"
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        assert_eq!(m.settings.get("enabled").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(m.settings.get("maxItems").and_then(|v| v.as_integer()), Some(100));
        assert_eq!(m.settings.get("ratio").and_then(|v| v.as_float()), Some(0.5));
        assert_eq!(
            m.settings.get("label").and_then(|v| v.as_str()),
            Some("hello")
        );
    }

    // =====================================================
    // Shortcuts
    // =====================================================

    #[test]
    fn shortcuts_default_to_empty() {
        let m = Manifest::parse(&minimal("")).expect("should parse");
        assert!(m.shortcuts.is_empty());
    }

    #[test]
    fn parse_multiple_shortcuts() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"

            [shortcuts]
            open-history = { label = "Open History", default = "CmdOrCtrl+Shift+V" }
            toggle-mode = { label = "Toggle Mode", default = "CmdOrCtrl+Shift+M" }
        "#;

        let m = Manifest::parse(toml).expect("should parse");
        assert_eq!(m.shortcuts.len(), 2);

        let open = m.shortcuts.get("open-history").expect("open-history");
        assert_eq!(open.label, "Open History");
        assert_eq!(open.default, "CmdOrCtrl+Shift+V");

        let toggle = m.shortcuts.get("toggle-mode").expect("toggle-mode");
        assert_eq!(toggle.label, "Toggle Mode");
        assert_eq!(toggle.default, "CmdOrCtrl+Shift+M");
    }

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
        assert_eq!(fe.views.get("picker").map(String::as_str), Some("EmojiGrid"));
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
        assert_eq!(fe.views.get("history").map(String::as_str), Some("HistoryView"));
        assert_eq!(fe.views.get("detail").map(String::as_str), Some("DetailView"));
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
    // Required field enforcement
    // =====================================================

    #[test]
    fn reject_missing_id() {
        let toml = r#"
            [plugin]
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"
        "#;
        assert!(Manifest::parse(toml).is_err());
    }

    #[test]
    fn reject_missing_name() {
        let toml = r#"
            [plugin]
            id = "test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"
        "#;
        assert!(Manifest::parse(toml).is_err());
    }

    #[test]
    fn reject_missing_description() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"
        "#;
        assert!(Manifest::parse(toml).is_err());
    }

    #[test]
    fn reject_missing_version() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            wasm = "test.wasm"
            icon = "heroicons:beaker"
        "#;
        assert!(Manifest::parse(toml).is_err());
    }

    #[test]
    fn reject_missing_wasm() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            icon = "heroicons:beaker"
        "#;
        assert!(Manifest::parse(toml).is_err());
    }

    #[test]
    fn reject_missing_icon() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
        "#;
        assert!(Manifest::parse(toml).is_err());
    }

    #[test]
    fn reject_missing_plugin_section() {
        let toml = r#"
            [settings]
            key = "value"
        "#;
        assert!(Manifest::parse(toml).is_err());
    }

    // =====================================================
    // Malformed TOML
    // =====================================================

    #[test]
    fn reject_completely_invalid_toml() {
        assert!(Manifest::parse("not valid toml [[[").is_err());
    }

    #[test]
    fn reject_empty_input() {
        assert!(Manifest::parse("").is_err());
    }

    // =====================================================
    // Unknown fields are ignored (forward compatibility)
    // =====================================================

    #[test]
    fn ignore_unknown_fields_in_plugin_section() {
        let toml = r#"
            [plugin]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "heroicons:beaker"
            author = "Unknown Author"
            license = "MIT"
        "#;
        // Unknown fields should not cause a parse error —
        // this ensures forward compatibility when new manifest
        // fields are added in later versions.
        Manifest::parse(toml).expect("should parse despite unknown fields");
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
            err.to_string().contains("launcher-css")
                && err.to_string().contains("launcher-bundle"),
            "error should mention both fields: {err}"
        );
    }

    #[test]
    fn reject_settings_css_without_settings_bundle() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "CSS without bundle"
            version = "0.1.0"
            wasm = "bad.wasm"
            icon = "heroicons:beaker"

            [frontend]
            settings-css = "frontend/settings.css"
        "#;

        let err = Manifest::parse(toml).unwrap_err();
        assert!(
            err.to_string().contains("settings-css")
                && err.to_string().contains("settings-bundle"),
            "error should mention both fields: {err}"
        );
    }
}
