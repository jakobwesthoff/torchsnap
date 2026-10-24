// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget Manifest
//
// Typed representation of the `manifest.toml` that every
// `.torchsnap` gadget archive (or development directory)
// must contain. The manifest carries all metadata the host
// needs to display, configure, and load the gadget without
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

    /// Gadget-specific settings defaults. Each key-value pair
    /// is applied to the settings store on first load (existing
    /// user values are never overwritten). The values are
    /// arbitrary JSON-compatible types.
    #[serde(default)]
    pub settings: HashMap<String, toml::Value>,

    /// Global keyboard shortcuts the gadget wants to register.
    /// Keys are stable shortcut IDs (e.g., `"open-clipboard"`),
    /// values describe the shortcut.
    #[serde(default)]
    pub shortcuts: HashMap<String, ShortcutDef>,

    /// Frontend component declarations. Omitted when the
    /// gadget has no UI.
    pub frontend: Option<FrontendDef>,

    /// Per-gadget storage configuration. Currently only the
    /// SQL sub-table is supported, but the wrapping
    /// `[storage]` namespace leaves room for future
    /// `[storage.kv]` / `[storage.files]` blocks without
    /// breaking existing manifests.
    pub storage: Option<StorageDef>,

    /// Scheduled background tasks. Each `[[tasks]]` entry
    /// declares a unique `id` and a 5-field POSIX cron
    /// expression. The host's per-gadget scheduler walks
    /// the list, sleeps until the earliest next fire, and
    /// invokes the WIT `tasks::run-task` guest export.
    #[serde(default, rename = "tasks")]
    pub tasks: Vec<TaskDef>,

    /// Host capability permissions. Gadgets opt into `opener`,
    /// `http`, and `command` by declaring the relevant sub-tables
    /// or `[[permissions.command]]` arrays. Omitting `[permissions]`
    /// entirely means no capability is available (deny by default).
    pub permissions: Option<PermissionsDef>,
}

// =========================================================
// Gadget Identity & Core Properties
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
    /// below the gadget name.
    pub description: String,

    /// Gadget version (e.g., `"0.1.0"`). For display and
    /// future update checking.
    pub version: String,

    /// Path to the WASM component binary within the archive
    /// or directory (e.g., `"hello_world.wasm"`).
    pub wasm: String,

    /// Gadget icon for the settings sidebar and section header.
    ///
    /// Two formats are supported:
    /// - `"heroicons:<name>"` — resolved to a HeroIcon component
    /// - Any other string — treated as a path to a WebP image
    ///   within the gadget archive/directory
    pub icon: GadgetIcon,

    /// Optional search prefixes for exclusive query routing
    /// (e.g., `[":"]` for the emoji picker). Omit for catalog
    /// or always-on query gadgets.
    #[serde(default)]
    pub prefixes: Vec<String>,
}

// =========================================================
// Gadget ID (validated newtype)
// =========================================================

/// A validated gadget identifier. Lowercase ASCII alphanumeric
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
        validate_gadget_id(&raw).map_err(serde::de::Error::custom)?;
        Ok(GadgetId(raw))
    }
}

pub(crate) fn validate_gadget_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("gadget id must not be empty".into());
    }
    if id.starts_with('-') || id.ends_with('-') {
        return Err(format!(
            "gadget id `{id}` must not start or end with a hyphen"
        ));
    }
    if let Some(ch) = id
        .chars()
        .find(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit() && *c != '-')
    {
        return Err(format!(
            "gadget id `{id}` contains invalid character `{ch}` — \
             only lowercase alphanumeric and hyphens are allowed"
        ));
    }
    Ok(())
}

// =========================================================
// Gadget Icon
// =========================================================

/// Either a HeroIcon reference or a path to an image file
/// within the gadget source (archive or directory).
///
/// The image variant stores a path relative to the gadget
/// root — it is not a filesystem path. At load time the host
/// reads the image bytes via `GadgetSource::read_file`.
#[derive(Debug, Clone)]
pub enum GadgetIcon {
    /// A HeroIcon name (e.g., `"clipboard-document-list"`).
    HeroIcon(String),

    /// A relative path to a WebP image within the gadget
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
        // cron expression or a duplicate id fails gadget
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

        // Every user-supplied path must be gadget-relative and
        // free of traversal. Rejecting at parse time keeps the
        // guarantee load-bearing: no downstream code ever sees
        // an unvalidated path. See the "Gadget Path Guard"
        // comment in `source.rs` for the rationale.
        paths::validate_manifest_paths(&manifest)?;

        // The host provisions SQL storage only when both halves are
        // present: the permission grants the database that
        // `[storage.sql]` declares. Either half alone is an authoring
        // mistake, so both must appear together.
        let wants_sql_storage = manifest.permissions.as_ref().is_some_and(|p| p.sql_storage);
        let has_storage_sql = manifest.storage.as_ref().is_some_and(|s| s.sql.is_some());
        if wants_sql_storage && !has_storage_sql {
            anyhow::bail!(
                "manifest sets `permissions.sql-storage = true` but declares no `[storage.sql]` \
                 table; add `[storage.sql]` with its migrations or remove the permission"
            );
        }
        if has_storage_sql && !wants_sql_storage {
            anyhow::bail!(
                "manifest declares a `[storage.sql]` table but does not set \
                 `permissions.sql-storage = true`; add the permission or remove the table"
            );
        }

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
    // Permissions: sql-storage needs [storage.sql]
    // =====================================================

    #[test]
    fn reject_sql_storage_permission_without_storage_sql() {
        for extra in [
            "[permissions]\nsql-storage = true",
            "[permissions]\nsql-storage = true\n[storage]",
        ] {
            let err = Manifest::parse(&minimal(extra)).expect_err(extra);
            let msg = err.to_string();
            assert!(msg.contains("sql-storage"), "{extra}: {msg}");
            assert!(msg.contains("[storage.sql]"), "{extra}: {msg}");
        }
    }

    // Without the permission the host provisions no database, so the
    // table would declare storage the gadget never gets.
    #[test]
    fn reject_storage_sql_without_sql_storage_permission() {
        for extra in [
            "[storage.sql]\nmigrations = [\"migrations/001.sql\"]",
            "[permissions]\nsql-storage = false\n[storage.sql]\nmigrations = []",
            "[permissions]\nsettings = true\n[storage.sql]\nmigrations = []",
        ] {
            let err = Manifest::parse(&minimal(extra)).expect_err(extra);
            let msg = err.to_string();
            assert!(msg.contains("sql-storage"), "{extra}: {msg}");
            assert!(msg.contains("[storage.sql]"), "{extra}: {msg}");
        }
    }

    #[test]
    fn accept_empty_storage_table_without_sql_storage_permission() {
        let m = Manifest::parse(&minimal("[storage]")).expect("should parse");
        assert!(m.storage.expect("storage").sql.is_none());
    }

    #[test]
    fn accept_sql_storage_permission_with_storage_sql() {
        let m = Manifest::parse(&minimal(
            "[permissions]\nsql-storage = true\n[storage.sql]\nmigrations = [\"migrations/001.sql\"]",
        ))
        .expect("should parse");
        assert!(m.permissions.expect("permissions").sql_storage);
        assert!(m.storage.and_then(|s| s.sql).is_some());
    }

    // =====================================================
    // Manifest: happy paths
    // =====================================================

    #[test]
    fn parse_minimal_manifest() {
        let m = Manifest::parse(&minimal("")).expect("should parse");
        assert_eq!(m.gadget.id.as_str(), "test-gadget");
        assert_eq!(m.gadget.name, "Test Gadget");
        assert_eq!(m.gadget.description, "A test gadget");
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
    // Gadget ID validation
    // =====================================================

    #[test]
    fn accept_valid_gadget_ids() {
        let valid = [
            "a",
            "hello-world",
            "my-gadget-2",
            "x1",
            "abc",
            "a-b-c-d",
            "gadget123",
        ];
        for id in valid {
            assert!(validate_gadget_id(id).is_ok(), "should accept `{id}`");
        }
    }

    #[test]
    fn reject_empty_gadget_id() {
        let err = validate_gadget_id("").unwrap_err();
        assert!(err.contains("empty"), "error: {err}");
    }

    #[test]
    fn reject_leading_hyphen() {
        let err = validate_gadget_id("-leading").unwrap_err();
        assert!(err.contains("start or end with a hyphen"), "error: {err}");
    }

    #[test]
    fn reject_trailing_hyphen() {
        let err = validate_gadget_id("trailing-").unwrap_err();
        assert!(err.contains("start or end with a hyphen"), "error: {err}");
    }

    #[test]
    fn reject_uppercase_in_gadget_id() {
        let err = validate_gadget_id("Upper").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn reject_underscore_in_gadget_id() {
        let err = validate_gadget_id("under_score").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn reject_spaces_in_gadget_id() {
        let err = validate_gadget_id("has space").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn reject_dots_in_gadget_id() {
        let err = validate_gadget_id("my.gadget").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn reject_unicode_in_gadget_id() {
        let err = validate_gadget_id("plügin").unwrap_err();
        assert!(err.contains("invalid character"), "error: {err}");
    }

    #[test]
    fn gadget_id_display_and_as_str() {
        let toml = minimal("");
        let m = Manifest::parse(&toml).expect("should parse");
        assert_eq!(m.gadget.id.as_str(), "test-gadget");
        assert_eq!(m.gadget.id.to_string(), "test-gadget");
    }

    #[test]
    fn gadget_id_equality() {
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
    fn reject_missing_gadget_section() {
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
    fn ignore_unknown_fields_in_gadget_section() {
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

    // =====================================================
    // Every gadget in the repository parses
    // =====================================================

    /// The gadgets under `gadgets/` and the command test fixture are
    /// what real manifests look like, so a parser rule that rejects one
    /// of them breaks a shipped or a template gadget.
    #[test]
    fn every_gadget_manifest_parses() {
        let manifests = [
            (
                "awake",
                include_str!("../../../../gadgets/awake/manifest.toml"),
            ),
            (
                "bangs",
                include_str!("../../../../gadgets/bangs/manifest.toml"),
            ),
            (
                "calculator",
                include_str!("../../../../gadgets/calculator/manifest.toml"),
            ),
            (
                "emoji-picker",
                include_str!("../../../../gadgets/emoji-picker/manifest.toml"),
            ),
            (
                "hello-world",
                include_str!("../../../../gadgets/hello-world/manifest.toml"),
            ),
            (
                "open-url",
                include_str!("../../../../gadgets/open-url/manifest.toml"),
            ),
            (
                "template",
                include_str!("../../../../gadgets/template/manifest.toml"),
            ),
            (
                "zerotier",
                include_str!("../../../../gadgets/zerotier/manifest.toml"),
            ),
            (
                "command-gadget fixture",
                include_str!("../../../tests/fixtures/command-gadget/manifest.toml"),
            ),
        ];
        for (name, source) in manifests {
            if let Err(e) = Manifest::parse(source) {
                panic!("manifest of `{name}` no longer parses: {e:#}");
            }
        }
    }

    // =====================================================
    // Every gadget declares the permissions its code uses
    // =====================================================

    /// Concatenate the Rust source of a gadget, leaving out comment
    /// lines, so a snippet shown only in a comment does not count as
    /// use.
    fn gadget_code(dir: &std::path::Path) -> String {
        let mut code = String::new();
        for entry in std::fs::read_dir(dir).expect("gadget source dir is readable") {
            let path = entry.expect("dir entry is readable").path();
            if path.is_dir() {
                code.push_str(&gadget_code(&path));
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).expect("source is readable");
                for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
                    code.push_str(line);
                    code.push('\n');
                }
            }
        }
        code
    }

    /// The host refuses to enable a gadget whose component imports an
    /// interface its manifest does not declare. The template is what
    /// authors copy, so a mismatch there ships to every new gadget.
    /// This checks the interfaces whose SDK module name appears in a
    /// gadget's code against the manifest, without needing a built
    /// component. `website_metadata` is left out because the template
    /// carries it as unused example code.
    #[test]
    fn every_gadget_declares_the_permissions_its_code_uses() {
        let gadgets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../gadgets");
        let mut checked = 0;
        for entry in std::fs::read_dir(&gadgets_dir).expect("gadgets dir is readable") {
            let dir = entry.expect("dir entry is readable").path();
            let manifest_path = dir.join("manifest.toml");
            if !manifest_path.is_file() {
                continue;
            }
            let id = dir.file_name().expect("dir has a name").to_string_lossy();
            let source = std::fs::read_to_string(&manifest_path).expect("manifest is readable");
            let manifest = Manifest::parse(&source).expect("manifest parses");
            let granted =
                |f: fn(&PermissionsDef) -> bool| manifest.permissions.as_ref().is_some_and(f);

            let code = gadget_code(&dir.join("src"));
            let uses = |module: &str| {
                let needle = format!("{module}::");
                code.match_indices(&needle).any(|(at, _)| {
                    !code[..at]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                })
            };

            let declared: [(&str, bool); 9] = [
                ("opener", granted(|p| p.opener.is_some())),
                ("http", granted(|p| p.http.is_some())),
                ("filesystem", granted(|p| p.filesystem.is_some())),
                ("command", granted(|p| !p.command.is_empty())),
                ("clipboard", granted(|p| p.clipboard)),
                ("frecency", granted(|p| p.frecency)),
                ("settings", granted(|p| p.settings)),
                ("sql_storage", granted(|p| p.sql_storage)),
                ("path_resolver", granted(|p| p.path_resolver)),
            ];
            for (module, has_permission) in declared {
                assert!(
                    has_permission || !uses(module),
                    "gadget `{id}` calls `{module}::` but its manifest does not declare the permission",
                );
            }
            checked += 1;
        }
        assert!(checked >= 8, "found only {checked} gadget manifests");
    }
}
