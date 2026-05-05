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

use serde::{Deserialize, Serialize};

pub(crate) mod permissions;
pub use permissions::{ArgvConstraint, CommandPermissionDef, PermissionsDef};

pub(crate) mod tasks;
pub use tasks::TaskDef;

pub(crate) mod frontend;
pub use frontend::FrontendDef;

pub(crate) mod storage;
pub use storage::StorageDef;

mod paths;

#[cfg(test)]
pub(crate) mod test_helpers;

// =========================================================
// Top-Level Manifest
//
// Serde naming conventions:
//
// This struct tree serves two formats:
// - `deserialize` = TOML input (manifest.toml, kebab-case keys)
// - `serialize`   = JSON output (Tauri commands → frontend, camelCase keys)
//
// Example: `launcher-bundle` in TOML ↔ `launcherBundle` in JSON.
//
// Fields that need different names in each direction use dual
// rename attributes:
//   `#[serde(rename(deserialize = "kebab-case", serialize = "camelCase"))]`
//
// Types with custom serde impls (`GadgetId`, `GadgetIcon`) handle
// their own format: `GadgetId` serializes as a plain string,
// `GadgetIcon` serializes back to the `"heroicons:<name>"` or
// bare path format matching the TOML input.
// =========================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub gadget: GadgetMeta,

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

    /// Per-plugin storage configuration. Currently only the
    /// SQL sub-table is supported, but the wrapping
    /// `[storage]` namespace leaves room for future
    /// `[storage.kv]` / `[storage.files]` blocks without
    /// breaking existing manifests.
    pub storage: Option<StorageDef>,

    /// Scheduled background tasks. Each `[[tasks]]` entry
    /// declares a unique `id` and a 5-field POSIX cron
    /// expression. The host's per-plugin scheduler walks
    /// the list, sleeps until the earliest next fire, and
    /// invokes the WIT `tasks::run-task` guest export.
    #[serde(default, rename = "tasks")]
    pub tasks: Vec<TaskDef>,

    /// Host capability permissions. Plugins opt into `opener`,
    /// `http`, and `command` by declaring the relevant sub-tables
    /// or `[[permissions.command]]` arrays. Omitting `[permissions]`
    /// entirely means no capability is available (deny by default).
    pub permissions: Option<PermissionsDef>,
}

// =========================================================
// Plugin Identity & Core Properties
// =========================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GadgetMeta {
    /// Stable identifier. Lowercase alphanumeric and hyphens
    /// only (e.g., `"clipboard-manager"`). Used as the key
    /// for settings namespaces, frecency storage, data
    /// directories, and frontend registry lookups.
    pub id: GadgetId,

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
    pub icon: GadgetIcon,

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
pub struct GadgetId(String);

impl GadgetId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GadgetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for GadgetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for GadgetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        validate_plugin_id(&raw).map_err(serde::de::Error::custom)?;
        Ok(GadgetId(raw))
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
    if let Some(ch) = id
        .chars()
        .find(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit() && *c != '-')
    {
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
/// reads the image bytes via `GadgetSource::read_file`.
#[derive(Debug, Clone)]
pub enum GadgetIcon {
    /// A HeroIcon name (e.g., `"clipboard-document-list"`).
    HeroIcon(String),

    /// A relative path to a WebP image within the plugin
    /// source (e.g., `"assets/icon.webp"`). MUST be read via
    /// `GadgetSource::read_file` — this is never a filesystem
    /// path.
    Asset(String),
}

impl Serialize for GadgetIcon {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            GadgetIcon::HeroIcon(name) => serializer.serialize_str(&format!("heroicons:{name}")),
            GadgetIcon::Asset(path) => serializer.serialize_str(path),
        }
    }
}

impl<'de> Deserialize<'de> for GadgetIcon {
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
            Ok(GadgetIcon::HeroIcon(name.to_string()))
        } else {
            Ok(GadgetIcon::Asset(raw))
        }
    }
}

// =========================================================
// Shortcuts
// =========================================================

/// Declaration of a single global keyboard shortcut.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShortcutDef {
    /// Human-readable label (e.g., "Open Clipboard History").
    pub label: String,

    /// Default key combination (e.g., "CmdOrCtrl+Shift+V").
    /// The host stores the actual binding — this is only used
    /// as the initial default.
    pub default: String,
}

// =========================================================
// Parsing
// =========================================================

impl Manifest {
    /// Parse a manifest from TOML source text.
    pub fn parse(toml_source: &str) -> anyhow::Result<Self> {
        let manifest: Manifest =
            toml::from_str(toml_source).map_err(|e| anyhow::anyhow!("invalid manifest: {e}"))?;

        // Validate `[[tasks]]` entries early so a malformed
        // cron expression or a duplicate id fails plugin
        // load instead of waiting for the scheduler to
        // crash at runtime.
        tasks::validate_task_definitions(&manifest.tasks)?;

        // Validate and normalize `[permissions]` entries.
        // `validate_permissions` consumes the raw value and returns a
        // normalized copy — origins are stored as `ascii_serialization()`
        // so runtime checks can use plain string equality.
        let manifest = Manifest {
            permissions: manifest
                .permissions
                .map(permissions::validate_permissions)
                .transpose()?,
            ..manifest
        };

        // Every user-supplied path must be plugin-relative and
        // free of traversal. Rejecting at parse time keeps the
        // guarantee load-bearing: no downstream code ever sees
        // an unvalidated path. See the "Plugin Path Guard"
        // comment in `source.rs` for the rationale.
        paths::validate_manifest_paths(&manifest)?;

        // Validate that views/inline-views reference a launcher bundle.
        if let Some(ref frontend) = manifest.frontend {
            let has_launcher_components =
                !frontend.views.is_empty() || !frontend.inline_views.is_empty();
            if has_launcher_components && frontend.launcher_bundle.is_none() {
                anyhow::bail!("manifest declares views or inline-views but no launcher-bundle");
            }

            if frontend.launcher_css.is_some() && frontend.launcher_bundle.is_none() {
                anyhow::bail!("manifest declares launcher-css but no launcher-bundle");
            }

            if frontend.settings.is_some() && frontend.settings_bundle.is_none() {
                anyhow::bail!("manifest declares a settings component but no settings-bundle");
            }

            if frontend.settings_css.is_some() && frontend.settings_bundle.is_none() {
                anyhow::bail!("manifest declares settings-css but no settings-bundle");
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
    use super::test_helpers::minimal;
    use super::*;

    // =====================================================
    // Manifest: happy paths
    // =====================================================

    #[test]
    fn parse_minimal_manifest() {
        let m = Manifest::parse(&minimal("")).expect("should parse");
        assert_eq!(m.gadget.id.as_str(), "test-plugin");
        assert_eq!(m.gadget.name, "Test Plugin");
        assert_eq!(m.gadget.description, "A test plugin");
        assert_eq!(m.gadget.version, "0.1.0");
        assert_eq!(m.gadget.wasm, "test.wasm");
        assert!(matches!(m.gadget.icon, GadgetIcon::HeroIcon(ref n) if n == "beaker"));
        assert!(m.gadget.prefixes.is_empty());
        assert!(m.settings.is_empty());
        assert!(m.shortcuts.is_empty());
        assert!(m.frontend.is_none());
    }

    #[test]
    fn parse_full_manifest() {
        let toml = r#"
            [gadget]
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
        assert_eq!(m.gadget.id.as_str(), "clipboard-manager");
        assert_eq!(m.gadget.version, "2.3.1");
        assert_eq!(m.gadget.prefixes, vec![":"]);

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
            assert!(validate_plugin_id(id).is_ok(), "should accept `{id}`");
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
        assert_eq!(m.gadget.id.as_str(), "test-plugin");
        assert_eq!(m.gadget.id.to_string(), "test-plugin");
    }

    #[test]
    fn plugin_id_equality() {
        let toml = minimal("");
        let m1 = Manifest::parse(&toml).expect("parse 1");
        let m2 = Manifest::parse(&toml).expect("parse 2");
        assert_eq!(m1.gadget.id, m2.gadget.id);
    }

    // =====================================================
    // Icon parsing
    // =====================================================

    #[test]
    fn parse_heroicon() {
        let toml = minimal("");
        let m = Manifest::parse(&toml).expect("should parse");
        match &m.gadget.icon {
            GadgetIcon::HeroIcon(name) => assert_eq!(name, "beaker"),
            GadgetIcon::Asset(p) => panic!("expected HeroIcon, got Asset({p})"),
        }
    }

    #[test]
    fn parse_asset_icon_simple_path() {
        let toml = r#"
            [gadget]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "assets/icon.webp"
        "#;
        let m = Manifest::parse(toml).expect("should parse");
        match &m.gadget.icon {
            GadgetIcon::Asset(path) => assert_eq!(path, "assets/icon.webp"),
            GadgetIcon::HeroIcon(n) => panic!("expected Asset, got HeroIcon({n})"),
        }
    }

    #[test]
    fn parse_asset_icon_bare_filename() {
        let toml = r#"
            [gadget]
            id = "test"
            name = "Test"
            description = "Test"
            version = "0.1.0"
            wasm = "test.wasm"
            icon = "icon.webp"
        "#;
        let m = Manifest::parse(toml).expect("should parse");
        assert!(matches!(&m.gadget.icon, GadgetIcon::Asset(p) if p == "icon.webp"));
    }

    #[test]
    fn reject_heroicons_prefix_without_name() {
        let toml = r#"
            [gadget]
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
        assert!(m.gadget.prefixes.is_empty());
    }

    #[test]
    fn parse_single_prefix() {
        let m = Manifest::parse(&minimal(r#"prefixes = [":"]"#)).expect("should parse");
        assert_eq!(m.gadget.prefixes, vec![":"]);
    }

    #[test]
    fn parse_multiple_prefixes() {
        let m = Manifest::parse(&minimal(r#"prefixes = [":", "=", "!"]"#)).expect("should parse");
        assert_eq!(m.gadget.prefixes, vec![":", "=", "!"]);
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
            [gadget]
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
        assert_eq!(
            m.settings.get("enabled").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            m.settings.get("maxItems").and_then(|v| v.as_integer()),
            Some(100)
        );
        assert_eq!(
            m.settings.get("ratio").and_then(|v| v.as_float()),
            Some(0.5)
        );
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
            [gadget]
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
    // Required field enforcement
    // =====================================================

    #[test]
    fn reject_missing_id() {
        let toml = r#"
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
}
