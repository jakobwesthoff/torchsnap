// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{Manifest, PluginIcon};
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
