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

use super::source::validate_plugin_path;

pub(crate) mod permissions;
pub use permissions::PermissionsDef;

#[cfg(test)]
mod test_helpers;

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
// Types with custom serde impls (`PluginId`, `PluginIcon`) handle
// their own format: `PluginId` serializes as a plain string,
// `PluginIcon` serializes back to the `"heroicons:<name>"` or
// bare path format matching the TOML input.
// =========================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
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

impl Serialize for PluginId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
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

impl Serialize for PluginIcon {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PluginIcon::HeroIcon(name) => serializer.serialize_str(&format!("heroicons:{name}")),
            PluginIcon::Asset(path) => serializer.serialize_str(path),
        }
    }
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

// =========================================================
// Storage configuration
//
// Plugins opt into per-plugin storage by declaring a
// `[storage]` table in their manifest. The host materializes
// the requested backends on first use — plugins that never
// touch their storage never get a database file on disk.
// =========================================================

/// `[storage]` block. Future expansion can add `[storage.kv]`,
/// `[storage.files]`, etc. without breaking existing manifests.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageDef {
    /// `[storage.sql]` — per-plugin SQLite database.
    pub sql: Option<SqlStorageDef>,
}

/// `[storage.sql]` block.
///
/// Migrations are declared as a list of file paths relative
/// to the plugin root. The host reads the file contents via
/// `PluginSource::read_file` at plugin load time and applies
/// them during `enable()` before the guest runs.
///
/// Single source of truth: the `.sql` files. Plugin tests can
/// `include_str!` the same files the manifest references —
/// no duplication, no drift.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SqlStorageDef {
    /// Ordered list of migration file paths. Each path is
    /// relative to the plugin root and should resolve to a
    /// `.sql` text file inside the plugin's directory or
    /// archive.
    #[serde(default)]
    pub migrations: Vec<String>,
}

/// `[permissions.opener]` — declares the plugin's
/// `opener` capabilities: which URL schemes it may pass to
/// `open-url`, and whether it may invoke `open-path` /
/// `reveal-path`.
///
/// ```toml
/// [permissions.opener]
/// schemes      = ["https", "http"]
/// open-path    = true
/// reveal-path  = true
/// ```
///
/// At least one capability must be granted: an
/// `[permissions.opener]` block with an empty `schemes`
/// list and both booleans `false` is a manifest authoring
/// error (the section is declared without granting
/// anything).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenerPermissionsDef {
    /// URL schemes accepted by `opener::open-url`.
    #[serde(default)]
    pub schemes: Vec<String>,

    /// Whether the plugin may invoke `opener::open-path`
    /// (open a filesystem path with the registered
    /// application).
    #[serde(default, rename = "open-path")]
    pub open_path: bool,

    /// Whether the plugin may invoke `opener::reveal-path`
    /// (reveal a filesystem path in the OS file manager).
    #[serde(default, rename = "reveal-path")]
    pub reveal_path: bool,
}

/// `[permissions.http]` — declares which origins the plugin
/// is allowed to reach via `http::fetch`.
///
/// ```toml
/// [permissions.http]
/// origins = ["https://api.example.com"]
///
/// # or trust-all:
/// origins = ["*"]
/// ```
///
/// Origins must be valid `scheme + host` pairs
/// (e.g. `"https://api.example.com"`). They are normalized
/// to `ascii_serialization()` form at parse time. The
/// special value `"*"` opts the plugin into trust-all mode.
///
/// An empty `origins` list is a manifest authoring error.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpPermissionsDef {
    /// Stored as normalized `ascii_serialization()` origins,
    /// except for the literal `"*"` which is preserved as-is.
    pub origins: Vec<String>,
}

/// `[permissions.fs]` — declares read-only filesystem paths
/// the plugin may access via the `fs` host import.
///
/// ```toml
/// [permissions.fs]
/// read = [
///     "${xdg-config}/myapp/config.toml",
///     "/var/lib/myapp/data/*.json",
/// ]
/// ```
///
/// Patterns accept `${...}` substitution tokens from
/// [`RECOGNIZED_PERMISSION_VARIABLES`] and the glob
/// metacharacters `*` (single segment) and `**` (multi-segment).
///
/// An empty `read` list is a manifest authoring error.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FsPermissionsDef {
    /// Patterns the plugin may read from. `${...}` tokens are
    /// preserved verbatim — substitution and glob compilation
    /// happen at bridge construction, when the per-instance
    /// `PathContext` is available.
    pub read: Vec<String>,
}

// =========================================================
// Command (process exec) permissions
// =========================================================

/// `[[permissions.command]]` rule — a single binary +
/// argv-shape pattern the plugin is permitted to invoke
/// via `command::run`.
///
/// ```toml
/// [[permissions.command]]
/// binary = "mdfind"
/// argv = [
///     { kind = "literal", value = "kMDItemContentType == 'com.apple.application-bundle'" },
/// ]
/// ```
///
/// The `binary` field is either an absolute path
/// (`"/usr/bin/mdfind"`) or a `PATH`-resolved name
/// (`"mdfind"`). Opener-class binaries (`open`, `xdg-open`,
/// `start`, etc.) are rejected at manifest parse time —
/// plugins wanting "open with the registered application"
/// use `[permissions.opener] open-path = true` instead.
///
/// `argv` is a per-position constraint list. Each element
/// declares what kind of argv value is accepted at that
/// position. An empty `argv` list means the binary is
/// invoked with no arguments. The `rest` constraint kind
/// covers all remaining positions and may only appear at
/// the trailing position.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandPermissionDef {
    /// The binary the rule grants. Absolute path or
    /// PATH-resolved name. Validated at manifest parse
    /// time (no NUL bytes, not in opener-class denylist).
    pub binary: String,

    /// Per-position argv constraints. Empty means the
    /// rule grants `binary` with zero arguments.
    #[serde(default)]
    pub argv: Vec<ArgvConstraint>,

    /// Optional default working directory for invocations
    /// matching this rule. Plugin can override per-call;
    /// when omitted, the host falls back to the per-plugin
    /// scratch directory at `${plugin-data}/exec-cwd/`.
    pub cwd: Option<String>,

    /// Hard ceiling on `command-options.timeout-ms` for
    /// invocations matching this rule. Calls that request
    /// a longer timeout are clamped down. `None` defers to
    /// the host default.
    #[serde(default, rename = "timeout-ms-max")]
    pub timeout_ms_max: Option<u32>,

    /// Hard ceiling on `command-options.max-output-bytes`
    /// for invocations matching this rule. `None` defers
    /// to the host default.
    #[serde(default, rename = "max-output-bytes")]
    pub max_output_bytes: Option<u64>,

    /// Hard ceiling on `command-options.stdin` byte length
    /// for invocations matching this rule. `None` defers
    /// to the host default.
    #[serde(default, rename = "max-stdin-bytes")]
    pub max_stdin_bytes: Option<u64>,
}

/// One per-position argv constraint. Internally tagged via
/// the `kind` discriminator.
///
/// Constraint kinds:
///
/// - `literal`     — exact byte match against `value`.
/// - `enum`        — argv element must equal one of `values`.
/// - `glob`        — argv element must match `pattern` as a glob.
/// - `regex`       — argv element must match `pattern` (anchored).
/// - `path-under`  — argv element parses as an absolute path that
///                   canonicalizes under `root`. `root` may use the
///                   substitution variables `${plugin-data}`,
///                   `${plugin-archive}`, `${home}`, `${xdg-config}`,
///                   `${xdg-data}`.
/// - `any-string`  — argv element accepted unconditionally.
/// - `rest`        — applies `constraint` to every remaining argv
///                   element. May only appear at the trailing position.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ArgvConstraint {
    Literal {
        value: String,
    },
    Enum {
        values: Vec<String>,
    },
    Glob {
        pattern: String,
    },
    Regex {
        pattern: String,
    },
    #[serde(rename = "path-under")]
    PathUnder {
        root: String,
    },
    AnyString,
    Rest {
        constraint: Box<ArgvConstraint>,
    },
}

// Substitution variables recognized in `literal`, `enum`,
// `path-under`, and per-rule `cwd` fields are defined in
// `super::permission_vars` and shared with the runtime
// `paths::resolve` host import. See that module for the
// list and the parser/substituter implementations.

// =========================================================
// Scheduled tasks
// =========================================================

/// `[[tasks]]` entry — a single scheduled background task.
///
/// `schedule` is a 5-field POSIX cron expression
/// (`minute hour day month weekday`). The host parses and
/// validates it at manifest load time and stores the parsed
/// `cron::Schedule` separately in the bridge — this struct
/// only carries the raw user-facing fields so that
/// (de)serialization stays straightforward.
///
/// Sub-minute scheduling is rejected — `cron`'s
/// underlying syntax is 6/7-field, but plugins use the
/// stricter 5-field POSIX form so the schedule space is
/// predictable and there's no chance of accidentally
/// scheduling a task at the second-resolution.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskDef {
    /// Unique task identifier within the plugin. Passed
    /// back to the guest via `tasks::run-task(task-id)`
    /// when the cron schedule fires.
    pub id: String,

    /// 5-field POSIX cron expression
    /// (`minute hour day month weekday`).
    pub schedule: String,
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
        validate_task_definitions(&manifest.tasks)?;

        // Validate and normalize `[permissions]` entries.
        // `validate_permissions` consumes the raw value and returns a
        // normalized copy — origins are stored as `ascii_serialization()`
        // so runtime checks can use plain string equality.
        let manifest = Manifest {
            permissions: manifest.permissions.map(permissions::validate_permissions).transpose()?,
            ..manifest
        };

        // Every user-supplied path must be plugin-relative and
        // free of traversal. Rejecting at parse time keeps the
        // guarantee load-bearing: no downstream code ever sees
        // an unvalidated path. See the "Plugin Path Guard"
        // comment in `source.rs` for the rationale.
        validate_manifest_paths(&manifest)?;

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

/// Walk every path-like field of a parsed manifest and run
/// it through [`validate_plugin_path`]. Rejects any manifest
/// that references an absolute path, a traversal, a Windows-
/// style prefix, a backslash, or a NUL byte. Called from
/// [`Manifest::parse`] — no caller needs to invoke it
/// directly.
///
/// Fields covered:
///
/// - `plugin.wasm`
/// - `plugin.icon` (Asset variant only — `heroicons:…` is
///   not a path and is skipped).
/// - `frontend.launcher_bundle`, `frontend.settings_bundle`,
///   `frontend.launcher_css`, `frontend.settings_css`.
/// - Every entry in `storage.sql.migrations`.
///
/// Not covered: `frontend.views` and `frontend.inline_views`
/// values — those are JavaScript export names, not paths.
fn validate_manifest_paths(manifest: &Manifest) -> anyhow::Result<()> {
    validate_plugin_path(&manifest.plugin.wasm)
        .map_err(|e| anyhow::anyhow!("invalid `plugin.wasm`: {e}"))?;

    if let PluginIcon::Asset(ref path) = manifest.plugin.icon {
        validate_plugin_path(path).map_err(|e| anyhow::anyhow!("invalid `plugin.icon`: {e}"))?;
    }

    if let Some(ref frontend) = manifest.frontend {
        if let Some(ref path) = frontend.launcher_bundle {
            validate_plugin_path(path)
                .map_err(|e| anyhow::anyhow!("invalid `frontend.launcher-bundle`: {e}"))?;
        }
        if let Some(ref path) = frontend.settings_bundle {
            validate_plugin_path(path)
                .map_err(|e| anyhow::anyhow!("invalid `frontend.settings-bundle`: {e}"))?;
        }
        if let Some(ref path) = frontend.launcher_css {
            validate_plugin_path(path)
                .map_err(|e| anyhow::anyhow!("invalid `frontend.launcher-css`: {e}"))?;
        }
        if let Some(ref path) = frontend.settings_css {
            validate_plugin_path(path)
                .map_err(|e| anyhow::anyhow!("invalid `frontend.settings-css`: {e}"))?;
        }
    }

    if let Some(ref storage) = manifest.storage
        && let Some(ref sql) = storage.sql
    {
        for path in &sql.migrations {
            validate_plugin_path(path).map_err(|e| {
                anyhow::anyhow!("invalid `storage.sql.migrations` entry `{path}`: {e}")
            })?;
        }
    }

    Ok(())
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_helpers::minimal;

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
        let m = Manifest::parse(&minimal(r#"prefixes = [":", "=", "!"]"#)).expect("should parse");
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
            err.to_string().contains("launcher-css") && err.to_string().contains("launcher-bundle"),
            "error should mention both fields: {err}"
        );
    }

    // =====================================================
    // Manifest path guard
    //
    // Every path-like field routes through
    // `validate_plugin_path`. These tests cover both the
    // per-field plumbing (did the parser call the guard on
    // THIS field?) and one happy-path full-manifest case so
    // regressions that skip the validator wholesale are
    // caught.
    // =====================================================

    /// Regression guard: a manifest that uses traversal in
    /// `plugin.wasm` must fail parsing. Failing silently
    /// would let a crafted plugin have the host `read_file`
    /// an arbitrary file as "the WASM component".
    #[test]
    fn reject_plugin_wasm_with_traversal() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "../../etc/passwd"
            icon = "heroicons:beaker"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("plugin.wasm"), "{err}");
        assert!(err.to_string().contains("escapes"), "{err}");
    }

    /// Absolute paths in `plugin.wasm` are equally dangerous
    /// and must fail.
    #[test]
    fn reject_plugin_wasm_absolute_path() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "/etc/passwd"
            icon = "heroicons:beaker"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("plugin.wasm"), "{err}");
    }

    /// The asset-icon path is also user-controlled and reaches
    /// `read_file` when the frontend renders a sidebar icon.
    #[test]
    fn reject_icon_asset_with_traversal() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "../../.ssh/id_rsa"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("plugin.icon"), "{err}");
    }

    /// `heroicons:beaker` is not a path — the guard must
    /// leave HeroIcon references alone. Rejecting them would
    /// break every real-world plugin.
    #[test]
    fn accept_heroicon_icon_reference() {
        let toml = r#"
            [plugin]
            id = "ok"
            name = "Ok"
            description = "Ok"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "heroicons:beaker"
        "#;
        Manifest::parse(toml).expect("HeroIcon references must pass the path guard");
    }

    #[test]
    fn reject_launcher_bundle_with_traversal() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-bundle = "../outside.js"

            [frontend.views]
            echo = "Echo"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("launcher-bundle"), "{err}");
    }

    #[test]
    fn reject_settings_bundle_with_backslash() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "heroicons:beaker"

            [frontend]
            settings-bundle = "frontend\\settings.js"

            [frontend.settings]
            component = "X"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("settings-bundle"), "{err}");
    }

    #[test]
    fn reject_launcher_css_absolute_path() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-bundle = "frontend/launcher.js"
            launcher-css = "/etc/steal.css"

            [frontend.views]
            echo = "Echo"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("launcher-css"), "{err}");
    }

    #[test]
    fn reject_settings_css_windows_drive_letter() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "heroicons:beaker"

            [frontend]
            settings-bundle = "frontend/settings.js"
            settings-css = "C:/Windows/System32/secret.css"

            [frontend.settings]
            component = "X"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("settings-css"), "{err}");
    }

    #[test]
    fn reject_sql_migration_with_traversal() {
        let toml = r#"
            [plugin]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "heroicons:beaker"

            [storage.sql]
            migrations = ["migrations/001_init.sql", "../secret.sql"]
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("migrations"), "{err}");
        assert!(err.to_string().contains("secret.sql"), "{err}");
    }

    /// Views and inline-views values are JavaScript export
    /// names, not file paths. Symbols with `..` or `/` in
    /// them are still invalid JS identifiers but the path
    /// guard specifically must not reject them — that would
    /// incorrectly conflate two concerns.
    #[test]
    fn accept_views_with_dot_characters_in_export_names() {
        let toml = r#"
            [plugin]
            id = "ok"
            name = "Ok"
            description = "Ok"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "heroicons:beaker"

            [frontend]
            launcher-bundle = "frontend/launcher.js"

            [frontend.views]
            weirdname = "Exports.With.Dots"
        "#;
        Manifest::parse(toml).expect("view export names are not paths and must pass");
    }

    /// Happy-path sanity: every path-looking field at once
    /// passes when all values are legitimate.
    #[test]
    fn accept_full_manifest_with_every_path_field_valid() {
        let toml = r#"
            [plugin]
            id = "ok"
            name = "Ok"
            description = "Ok"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "assets/icon.webp"

            [frontend]
            launcher-bundle = "frontend/dist/launcher.js"
            launcher-css = "frontend/dist/launcher.css"
            settings-bundle = "frontend/dist/settings.js"
            settings-css = "frontend/dist/settings.css"

            [frontend.views]
            one = "OneView"

            [frontend.settings]
            component = "MySettings"

            [storage.sql]
            migrations = ["migrations/001_init.sql", "migrations/002_tweak.sql"]
        "#;
        Manifest::parse(toml).expect("fully valid manifest must parse");
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
            err.to_string().contains("settings-css") && err.to_string().contains("settings-bundle"),
            "error should mention both fields: {err}"
        );
    }

    // =====================================================
    // Permissions: happy paths
    // =====================================================

    #[test]
    fn accept_manifest_without_permissions_section() {
        let m = Manifest::parse(&minimal("")).expect("should parse");
        assert!(m.permissions.is_none());
    }

    #[test]
    fn accept_permissions_section_with_no_sub_tables() {
        let m = Manifest::parse(&minimal("[permissions]")).expect("should parse");
        let p = m.permissions.expect("permissions present");
        assert!(p.opener.is_none());
        assert!(p.http.is_none());
    }

    #[test]
    fn accept_opener_permission_with_schemes() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.opener]
               schemes = ["https", "http"]"#,
        ))
        .expect("should parse");
        let schemes = m
            .permissions
            .expect("permissions")
            .opener
            .expect("opener")
            .schemes;
        assert_eq!(schemes, vec!["https", "http"]);
    }

    #[test]
    fn accept_http_permission_with_specific_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["https://api.example.com"]"#,
        ))
        .expect("should parse");
        let origins = m
            .permissions
            .expect("permissions")
            .http
            .expect("http")
            .origins;
        assert_eq!(origins, vec!["https://api.example.com"]);
    }

    #[test]
    fn accept_http_permission_with_wildcard() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["*"]"#,
        ))
        .expect("should parse");
        let origins = m
            .permissions
            .expect("permissions")
            .http
            .expect("http")
            .origins;
        assert_eq!(origins, vec!["*"]);
    }

    #[test]
    fn accept_opener_and_http_permissions_together() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.opener]
               schemes = ["https"]
               [permissions.http]
               origins = ["https://api.example.com"]"#,
        ))
        .expect("should parse");
        let p = m.permissions.expect("permissions");
        assert!(p.opener.is_some());
        assert!(p.http.is_some());
    }

    // =====================================================
    // Permissions: origin normalization
    // =====================================================

    #[test]
    fn normalize_trailing_slash_in_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["https://example.com/"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "https://example.com");
    }

    #[test]
    fn normalize_uppercase_scheme_in_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["HTTPS://example.com"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "https://example.com");
    }

    #[test]
    fn normalize_default_port_in_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["https://example.com:443"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "https://example.com");
    }

    #[test]
    fn preserve_non_default_port_in_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["https://example.com:8443"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "https://example.com:8443");
    }

    #[test]
    fn preserve_wildcard_as_is() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["*"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "*");
    }

    // =====================================================
    // Permissions: validation errors
    // =====================================================

    #[test]
    fn reject_opener_permission_with_no_capabilities_granted() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.opener]
               schemes = []"#,
        ))
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("permissions.opener") && msg.contains("capability"),
            "error should mention permissions.opener and capability granting: {msg}"
        );
    }

    #[test]
    fn accept_opener_with_only_open_path() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.opener]
               open-path = true"#,
        ))
        .expect("should parse");
        let opener = m.permissions.unwrap().opener.unwrap();
        assert!(opener.schemes.is_empty());
        assert!(opener.open_path);
        assert!(!opener.reveal_path);
    }

    #[test]
    fn accept_opener_with_only_reveal_path() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.opener]
               reveal-path = true"#,
        ))
        .expect("should parse");
        let opener = m.permissions.unwrap().opener.unwrap();
        assert!(opener.reveal_path);
    }

    // =====================================================
    // Permissions: fs
    // =====================================================

    #[test]
    fn fs_pattern_with_substitution_token_passes_metachar_check() {
        // Regression: the `${xdg-config}` token's literal `{`
        // and `}` must not trip the unsupported-glob-metachar
        // scan. The check should ignore characters inside
        // `${...}` substitutions.
        let m = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = ["${xdg-config}/ZeroTier/One/authtoken.secret"]"#,
        ))
        .expect("should parse");
        let fs = m.permissions.unwrap().fs.unwrap();
        assert_eq!(fs.read.len(), 1);
    }

    #[test]
    fn fs_pattern_rejects_empty_read_list() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = []"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `read` list"));
    }

    #[test]
    fn fs_pattern_rejects_traversal() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = ["/etc/../etc/hosts"]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("traversal segment"));
    }

    #[test]
    fn fs_pattern_rejects_unknown_substitution_variable() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = ["${nope}/foo"]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("unknown variable"));
    }

    #[test]
    fn fs_pattern_accepts_glob_metachars_star_and_doublestar() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = [
                   "${xdg-config}/myapp/*.toml",
                   "${xdg-data}/myapp/**/*.json",
               ]"#,
        ))
        .expect("should parse");
        let fs = m.permissions.unwrap().fs.unwrap();
        assert_eq!(fs.read.len(), 2);
    }

    // =====================================================
    // Permissions: command rules
    // =====================================================

    #[test]
    fn accept_minimal_command_rule() {
        let m = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "mdfind""#,
        ))
        .expect("should parse");
        let command = m.permissions.unwrap().command;
        assert_eq!(command.len(), 1);
        assert_eq!(command[0].binary, "mdfind");
        assert!(command[0].argv.is_empty());
    }

    #[test]
    fn accept_command_rule_with_full_argv_vocabulary() {
        let m = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "enum", values = ["HEAD", "main"] },
                   { kind = "glob", pattern = "refs/heads/*" },
                   { kind = "regex", pattern = "[0-9a-f]{40}" },
                   { kind = "path-under", root = "${plugin-data}/repos" },
                   { kind = "any-string" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]"#,
        ))
        .expect("should parse");
        let command = m.permissions.unwrap().command;
        assert_eq!(command[0].argv.len(), 7);
    }

    #[test]
    fn reject_command_rule_with_empty_binary() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = """#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `binary`"));
    }

    #[test]
    fn reject_command_rule_with_nul_binary() {
        let err = Manifest::parse(&minimal(
            "[[permissions.command]]\nbinary = \"foo\\u0000bar\"\n",
        ))
        .unwrap_err();
        assert!(err.to_string().contains("NUL byte"));
    }

    #[test]
    fn reject_command_rule_with_bad_regex() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "grep"
               argv = [{ kind = "regex", pattern = "[unclosed" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("does not compile"));
    }

    #[test]
    fn reject_command_rule_with_empty_enum() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = [] }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `values`"));
    }

    #[test]
    fn reject_command_rule_with_empty_glob() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "glob", pattern = "" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `pattern`"));
    }

    #[test]
    fn reject_command_rule_with_empty_path_under_root() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "path-under", root = "" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `root`"));
    }

    #[test]
    fn reject_command_rule_with_unknown_variable() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "${plugin-typo}" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("unknown variable"));
    }

    #[test]
    fn reject_command_rule_with_argv_after_rest() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "rest", constraint = { kind = "any-string" } },
                   { kind = "literal", value = "trailing" },
               ]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("after a `rest` constraint"));
    }

    #[test]
    fn reject_command_rule_with_nested_rest() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "rest", constraint = { kind = "rest", constraint = { kind = "any-string" } } },
               ]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("nested inside another `rest`"));
    }

    #[test]
    fn accept_recognized_variables_in_literal() {
        for var in &[
            "plugin-data",
            "plugin-archive",
            "home",
            "xdg-config",
            "xdg-data",
        ] {
            let toml_text = format!(
                r#"[[permissions.command]]
                   binary = "echo"
                   argv = [{{ kind = "literal", value = "${{{var}}}/foo" }}]"#
            );
            Manifest::parse(&minimal(&toml_text))
                .unwrap_or_else(|e| panic!("variable `{var}` should be accepted: {e}"));
        }
    }

    #[test]
    fn accept_multiple_command_rules() {
        let m = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "mdfind"

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "rev-parse" }]"#,
        ))
        .expect("should parse");
        let command = m.permissions.unwrap().command;
        assert_eq!(command.len(), 2);
        assert_eq!(command[0].binary, "mdfind");
        assert_eq!(command[1].binary, "git");
    }

    #[test]
    fn manifest_without_command_section_has_empty_command_vec() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["*"]"#,
        ))
        .expect("should parse");
        assert!(m.permissions.unwrap().command.is_empty());
    }

    // =====================================================
    // Permissions: command rule overlap
    // =====================================================

    #[test]
    fn accept_rules_with_different_binaries() {
        // Even though argv shapes are identical, distinct
        // binaries means the rules cannot match the same call.
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "mdfind"

               [[permissions.command]]
               binary = "git""#,
        ))
        .expect("different binaries do not overlap");
    }

    #[test]
    fn accept_rules_distinguished_by_literal_position() {
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "rev-parse" }]"#,
        ))
        .expect("different first-position literals do not overlap");
    }

    #[test]
    fn reject_identical_rules() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn reject_literal_subsumed_by_enum() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["log", "show"] }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn reject_enums_with_intersection() {
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["log", "show"] }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["show", "diff"] }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn accept_enums_without_intersection() {
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["log", "show"] }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "enum", values = ["push", "pull"] }]"#,
        ))
        .expect("disjoint enums do not overlap");
    }

    #[test]
    fn accept_rules_of_different_length_without_rest() {
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "literal", value = "HEAD" },
               ]"#,
        ))
        .expect("different fixed lengths cannot match the same argv");
    }

    #[test]
    fn reject_pattern_constraint_against_literal_at_same_length() {
        // `glob` / `regex` / `path-under` are conservatively
        // treated as overlapping with anything at the same
        // position — authors must differentiate elsewhere.
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "regex", pattern = "[a-z]+" }]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn reject_rest_swallowing_fixed_rule() {
        // Rule 1 accepts ["log", X*]; rule 2 accepts ["log", "HEAD"].
        // Rule 1's rest-of-any-string trivially overlaps rule 2.
        let err = Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]

               [[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "literal", value = "HEAD" },
               ]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("potentially-overlapping"));
    }

    #[test]
    fn accept_rest_with_disjoint_prefix() {
        // Rule 1's prefix is `log`; rule 2 starts with `show`.
        // The prefixes don't overlap, so no argv tuple matches both.
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]

               [[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "show" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]"#,
        ))
        .expect("disjoint prefixes do not overlap even with rest on both");
    }

    #[test]
    fn accept_fixed_rule_shorter_than_rest_prefix() {
        // Rule 1 needs 2 fixed args + rest; rule 2 has only 1 fixed arg.
        // Rule 2's call cannot satisfy rule 1's required-prefix length,
        // so they do not overlap.
        Manifest::parse(&minimal(
            r#"[[permissions.command]]
               binary = "git"
               argv = [
                   { kind = "literal", value = "log" },
                   { kind = "literal", value = "HEAD" },
                   { kind = "rest", constraint = { kind = "any-string" } },
               ]

               [[permissions.command]]
               binary = "git"
               argv = [{ kind = "literal", value = "log" }]"#,
        ))
        .expect("fixed rule shorter than rest-rule prefix does not overlap");
    }

    #[test]
    fn reject_http_permission_with_empty_origins() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = []"#,
        ))
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("permissions.http") && msg.contains("origins"),
            "error should mention both fields: {msg}"
        );
    }

    #[test]
    fn reject_malformed_origin() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["not-a-url"]"#,
        ))
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not-a-url"),
            "error should mention the offending value: {msg}"
        );
    }

    // =====================================================
    // Permissions: website-metadata flag
    // =====================================================

    #[test]
    fn website_metadata_perm_absent_when_no_permissions_section() {
        let m = Manifest::parse(&minimal("")).expect("parses");
        // No `[permissions]` section → `permissions` is `None`.
        assert!(m.permissions.is_none());
    }

    #[test]
    fn website_metadata_perm_defaults_to_false_when_other_perms_present() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["*"]"#,
        ))
        .expect("parses");
        let perms = m.permissions.expect("permissions section present");
        assert!(!perms.website_metadata);
    }

    #[test]
    fn website_metadata_perm_true_when_set() {
        let m = Manifest::parse(&minimal(
            r#"[permissions]
               website-metadata = true"#,
        ))
        .expect("parses");
        let perms = m.permissions.expect("permissions section present");
        assert!(perms.website_metadata);
    }

    #[test]
    fn website_metadata_perm_explicit_false() {
        let m = Manifest::parse(&minimal(
            r#"[permissions]
               website-metadata = false"#,
        ))
        .expect("parses");
        let perms = m.permissions.expect("permissions section present");
        assert!(!perms.website_metadata);
    }

    #[test]
    fn website_metadata_perm_rejects_non_bool() {
        let err = Manifest::parse(&minimal(
            r#"[permissions]
               website-metadata = "yes""#,
        ))
        .unwrap_err();
        // Confirm parsing fails on a type mismatch rather than
        // silently coercing the string to a bool.
        let msg = err.to_string();
        assert!(
            msg.contains("website-metadata") || msg.contains("bool") || msg.contains("type"),
            "expected type error for non-bool flag, got: {msg}"
        );
    }

    #[test]
    fn website_metadata_perm_coexists_with_http_and_command() {
        let m = Manifest::parse(&minimal(
            r#"[permissions]
               website-metadata = true

               [permissions.http]
               origins = ["https://api.example.com"]

               [[permissions.command]]
               binary = "git""#,
        ))
        .expect("parses");
        let perms = m.permissions.expect("permissions section present");
        assert!(perms.website_metadata);
        assert!(perms.http.is_some());
        assert_eq!(perms.command.len(), 1);
    }
}

// =========================================================
// Task definition validation
// =========================================================

/// Verify that every `[[tasks]]` entry parses as a valid
/// 5-field POSIX cron expression and that no two tasks
/// share the same id.
///
/// Both checks happen at manifest load time so that broken
/// schedules surface as clean plugin-load errors instead of
/// crashing the scheduler later.
pub(crate) fn validate_task_definitions(tasks: &[TaskDef]) -> anyhow::Result<()> {
    let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for task in tasks {
        if !seen_ids.insert(task.id.as_str()) {
            anyhow::bail!("duplicate scheduled task id `{}`", task.id);
        }
        parse_cron_schedule(&task.schedule)
            .map_err(|e| anyhow::anyhow!("invalid schedule for task `{}`: {e}", task.id))?;
    }
    Ok(())
}

/// Parse a 5-field POSIX cron expression
/// (`minute hour day month weekday`) into a
/// `cron::Schedule`.
///
/// The `cron` crate uses Quartz-style 6/7-field syntax
/// (`sec min hour day month dow [year]`), so we wrap the
/// user's 5 fields with `0` for seconds and `*` for year.
/// A pre-check on the field count catches the most common
/// authoring errors — wrong number of fields, Quartz macros
/// like `@daily` — with a friendly message before delegating
/// to `cron` for full validation.
pub(crate) fn parse_cron_schedule(schedule: &str) -> anyhow::Result<cron::Schedule> {
    use std::str::FromStr;

    // Pre-check the field count so plugin authors who pass
    // a 4/6/7-field expression get a clear "expected
    // 5-field POSIX cron" message instead of an opaque
    // Quartz-internal error from the `cron` crate. The
    // wrapping below is still the actual validation
    // mechanism — this check just catches the common
    // failure modes early with a friendlier explanation.
    let field_count = schedule.split_whitespace().count();
    if field_count != 5 {
        anyhow::bail!(
            "expected 5-field POSIX cron `minute hour day month weekday`, got {field_count} field(s) — sub-minute scheduling and 6/7-field Quartz syntax are not supported"
        );
    }

    // The `cron` crate uses Quartz-style 6/7-field syntax
    // (`sec min hour day month dow [year]`), so we wrap the
    // user's 5 fields with `0` for seconds and `*` for year.
    // This wrapping is also a defensive validation: a valid
    // 5-field POSIX expression becomes a valid 7-field
    // Quartz expression that `cron::Schedule::from_str`
    // accepts; a malformed expression that somehow has 5
    // tokens but isn't valid POSIX cron becomes a malformed
    // 7-field Quartz expression that the parser rejects
    // with its own error.
    let normalized = format!("0 {schedule} *");
    cron::Schedule::from_str(&normalized).map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
mod task_validation_tests {
    use super::*;

    fn task(id: &str, schedule: &str) -> TaskDef {
        TaskDef {
            id: id.to_string(),
            schedule: schedule.to_string(),
        }
    }

    #[test]
    fn empty_task_list_is_ok() {
        validate_task_definitions(&[]).unwrap();
    }

    #[test]
    fn valid_5_field_cron_accepted() {
        validate_task_definitions(&[task("cleanup", "*/30 * * * *")]).unwrap();
        validate_task_definitions(&[task("daily", "0 4 * * *")]).unwrap();
    }

    #[test]
    fn six_field_cron_rejected() {
        // Quartz-style 6-field input — explicitly out of
        // scope so plugins don't accidentally schedule at
        // second resolution. The cron crate's parser
        // rejects the resulting 8-field intermediate.
        let err = validate_task_definitions(&[task("bad", "0 */30 * * * *")]).unwrap_err();
        assert!(err.to_string().contains("schedule"), "{err}");
    }

    #[test]
    fn malformed_cron_rejected() {
        let err = validate_task_definitions(&[task("bad", "not a cron")]).unwrap_err();
        assert!(err.to_string().contains("schedule"), "{err}");
    }

    #[test]
    fn duplicate_task_ids_rejected() {
        let err = validate_task_definitions(&[
            task("cleanup", "*/30 * * * *"),
            task("cleanup", "0 0 * * *"),
        ])
        .unwrap_err();
        assert!(err.to_string().contains("duplicate"), "{err}");
    }

    #[test]
    fn empty_schedule_rejected() {
        let err = validate_task_definitions(&[task("bad", "")]).unwrap_err();
        assert!(err.to_string().contains("5-field"), "{err}");
    }

    #[test]
    fn four_field_schedule_rejected() {
        let err = validate_task_definitions(&[task("bad", "* * * *")]).unwrap_err();
        assert!(err.to_string().contains("5-field"), "{err}");
    }

    #[test]
    fn seven_field_schedule_rejected() {
        let err = validate_task_definitions(&[task("bad", "0 */30 * * * * *")]).unwrap_err();
        assert!(err.to_string().contains("5-field"), "{err}");
    }

    #[test]
    fn all_wildcards_5_field_accepted() {
        // The simplest legal POSIX cron expression — fires
        // every minute. Confirms the base case parses.
        validate_task_definitions(&[task("ok", "* * * * *")]).unwrap();
    }

    #[test]
    fn quartz_macro_at_daily_rejected() {
        // `@daily` is a Quartz alias for `0 0 * * *`, but
        // it's a single-token whole-expression macro. After
        // wrapping it becomes `0 @daily *` which the cron
        // crate rejects. The friendlier error wrapper catches
        // it as a 1-field input first.
        let err = validate_task_definitions(&[task("bad", "@daily")]).unwrap_err();
        assert!(err.to_string().contains("5-field"), "{err}");
    }
}
