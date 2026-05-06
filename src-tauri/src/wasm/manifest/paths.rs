// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{GadgetIcon, Manifest};
use crate::wasm::source::validate_plugin_path;

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
pub(super) fn validate_manifest_paths(manifest: &Manifest) -> anyhow::Result<()> {
    validate_plugin_path(&manifest.gadget.wasm)
        .map_err(|e| anyhow::anyhow!("invalid `gadget.wasm`: {e}"))?;

    if let GadgetIcon::Asset(ref path) = manifest.gadget.icon {
        validate_plugin_path(path).map_err(|e| anyhow::anyhow!("invalid `gadget.icon`: {e}"))?;
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

#[cfg(test)]
mod tests {
    use crate::wasm::manifest::Manifest;

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
            [gadget]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "../../etc/passwd"
            icon = "heroicons:beaker"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("gadget.wasm"), "{err}");
        assert!(err.to_string().contains("escapes"), "{err}");
    }

    /// Absolute paths in `gadget.wasm` are equally dangerous
    /// and must fail.
    #[test]
    fn reject_plugin_wasm_absolute_path() {
        let toml = r#"
            [gadget]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "/etc/passwd"
            icon = "heroicons:beaker"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("gadget.wasm"), "{err}");
    }

    /// The asset-icon path is also user-controlled and reaches
    /// `read_file` when the frontend renders a sidebar icon.
    #[test]
    fn reject_icon_asset_with_traversal() {
        let toml = r#"
            [gadget]
            id = "bad"
            name = "Bad"
            description = "Bad"
            version = "0.1.0"
            wasm = "ok.wasm"
            icon = "../../.ssh/id_rsa"
        "#;
        let err = Manifest::parse(toml).unwrap_err();
        assert!(err.to_string().contains("gadget.icon"), "{err}");
    }

    /// `heroicons:beaker` is not a path — the guard must
    /// leave HeroIcon references alone. Rejecting them would
    /// break every real-world plugin.
    #[test]
    fn accept_heroicon_icon_reference() {
        let toml = r#"
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
            [gadget]
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
}
