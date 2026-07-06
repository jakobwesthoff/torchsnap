// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Interface Gate — WIT import validation
//
// Validates that a compiled WASM component only imports WIT
// interfaces it has permissions for. Called between component
// compilation and instantiation so a gadget with undeclared
// imports is rejected with a clear error before any guest
// code runs.
//
// After the WIT-name-alignment refactor, every gated WIT
// interface name matches its manifest permission key exactly.
// The only knowledge this module carries is *which*
// interfaces are ungated (always available without a
// permission declaration) — everything else must be
// provisioned.
// =========================================================

use wasmtime::Engine;
use wasmtime::component::Component;

use crate::caps::ProvisionedCaps;

// =========================================================
// Constants
// =========================================================

const GADGET_PACKAGE_PREFIX: &str = "torchsnap:gadget/";

/// Core interfaces every gadget imports implicitly — no
/// manifest permission needed. This list is small and stable;
/// new host capabilities are gated by default.
const UNGATED: &[&str] = &["logging", "platform", "types", "assets"];

// =========================================================
// Public API
// =========================================================

/// Validate that `component`'s WIT imports are covered by
/// `caps`. Returns `Ok(())` when every gated import has a
/// corresponding provisioned capability, or a descriptive
/// error listing the missing interfaces.
pub fn validate(
    component: &Component,
    engine: &Engine,
    caps: &ProvisionedCaps,
    gadget_id: &str,
) -> anyhow::Result<()> {
    let component_type = component.component_type();
    let mut missing: Vec<String> = Vec::new();

    for (import_name, _) in component_type.imports(engine) {
        let Some(interface) = extract_interface_name(import_name) else {
            continue;
        };

        if is_ungated(interface) {
            continue;
        }

        if !is_provisioned(interface, caps) {
            missing.push(interface.to_string());
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        missing.sort();
        missing.dedup();
        anyhow::bail!(
            "gadget `{gadget_id}` imports interfaces without matching permissions: [{}]",
            missing.join(", "),
        )
    }
}

// =========================================================
// Internal helpers
// =========================================================

/// Extract the bare interface name from a fully-qualified
/// WIT import name. Returns `None` for non-gadget imports
/// (WASI, other packages).
///
/// Handles both versioned (`torchsnap:gadget/sql-storage@0.1.0`)
/// and unversioned (`torchsnap:gadget/sql-storage`) forms.
fn extract_interface_name(import: &str) -> Option<&str> {
    let rest = import.strip_prefix(GADGET_PACKAGE_PREFIX)?;
    // Strip optional `@<version>` suffix.
    Some(rest.split('@').next().unwrap_or(rest))
}

fn is_ungated(interface: &str) -> bool {
    UNGATED.contains(&interface)
}

/// Check whether a gated interface has a matching provisioned
/// capability. The match arms correspond 1:1 to the gated WIT
/// interfaces in `torchsnap-gadget.wit` — after name
/// alignment, each arm's string is both the WIT interface
/// name and the manifest permission key.
///
/// Unknown interfaces return `false` (fail-safe: reject
/// anything we don't recognize).
fn is_provisioned(interface: &str, caps: &ProvisionedCaps) -> bool {
    match interface {
        "clipboard" => caps.clipboard.is_some(),
        "command" => caps.command.is_some(),
        "filesystem" => caps.filesystem.is_some(),
        "frecency" => caps.frecency.is_some(),
        "http" => caps.http.is_some(),
        "opener" => caps.opener.is_some(),
        "path-resolver" => caps.path_resolver.is_some(),
        "settings" => caps.settings.is_some(),
        "sql-storage" => caps.sql_storage.is_some(),
        "website-metadata" => caps.website_metadata.is_some(),
        _ => false,
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::caps::*;
    use crate::wasm::runtime::WasmRuntime;

    const MINIMAL_GADGET_WASM: &[u8] =
        include_bytes!("../../tests/fixtures/minimal-gadget/minimal_gadget.wasm");

    /// The opener-http fixture imports `opener` and `http`
    /// (gated interfaces), making it suitable for testing
    /// validation rejections.
    const OPENER_HTTP_GADGET_WASM: &[u8] =
        include_bytes!("../../tests/fixtures/opener-http-gadget/opener_http_gadget.wasm");

    fn test_runtime() -> Arc<WasmRuntime> {
        WasmRuntime::new().expect("WasmRuntime::new")
    }

    fn test_paths() -> Arc<crate::paths::GadgetPaths> {
        Arc::new(crate::paths::GadgetPaths {
            platform: Arc::new(crate::paths::PlatformPaths {
                home: std::path::PathBuf::from("/tmp/test"),
                xdg_config: std::path::PathBuf::from("/tmp/test/.config"),
                xdg_data: std::path::PathBuf::from("/tmp/test/.local/share"),
            }),
            gadget_data: std::path::PathBuf::from("/tmp/test-data"),
            gadget_archive: std::path::PathBuf::from("/tmp/test-archive"),
        })
    }

    /// Caps with every capability provisioned. Uses minimal
    /// stub implementations — the validation only checks
    /// `is_some()`, never invokes the caps.
    fn full_caps() -> ProvisionedCaps {
        let paths = test_paths();
        ProvisionedCaps {
            opener: Some(Arc::new(OpenerCap::from_closures(
                OpenerPermissions {
                    schemes: vec![],
                    open_path: false,
                    reveal_path: false,
                },
                Box::new(|_| Ok(())),
                Box::new(|_| Ok(())),
                Box::new(|_| Ok(())),
            ))),
            http: Some(Arc::new(HttpCap::new(vec![]))),
            filesystem: Some(Arc::new(
                FilesystemCap::new(&[], &*paths).expect("test filesystem cap"),
            )),
            command: Some(Arc::new(CommandCap::from_compiled_rules(
                vec![],
                std::path::PathBuf::from("/tmp/test"),
            ))),
            clipboard: Some(Arc::new(ClipboardCap::new(Box::new(|_| Ok(()))))),
            sql_storage: Some(Arc::new(SqlStorageCap::new(Arc::new(
                crate::storage::SqlStorage::open(
                    std::env::temp_dir().join("torchsnap-test-gate.sqlite3"),
                    &["CREATE TABLE IF NOT EXISTS _gate_probe (id INTEGER PRIMARY KEY);"],
                )
                .expect("test db"),
            )))),
            website_metadata: None,
            icon_cache: None,
            settings: None,
            frecency: None,
            path_resolver: Some(Arc::new(PathResolverCap::new(paths))),
        }
    }

    /// Caps with nothing provisioned.
    fn empty_caps() -> ProvisionedCaps {
        ProvisionedCaps {
            opener: None,
            http: None,
            filesystem: None,
            command: None,
            clipboard: None,
            sql_storage: None,
            website_metadata: None,
            icon_cache: None,
            settings: None,
            frecency: None,
            path_resolver: None,
        }
    }

    // ----- extract_interface_name ----------------------------

    #[test]
    fn extract_versioned_name() {
        assert_eq!(
            extract_interface_name("torchsnap:gadget/sql-storage@0.1.0"),
            Some("sql-storage"),
        );
    }

    #[test]
    fn extract_unversioned_name() {
        assert_eq!(
            extract_interface_name("torchsnap:gadget/clipboard"),
            Some("clipboard"),
        );
    }

    #[test]
    fn extract_wasi_import_returns_none() {
        assert_eq!(extract_interface_name("wasi:cli/stdout@0.2.0"), None,);
    }

    #[test]
    fn extract_malformed_returns_none() {
        assert_eq!(extract_interface_name(""), None);
        assert_eq!(extract_interface_name("not-a-package"), None);
    }

    // ----- is_ungated ----------------------------------------

    #[test]
    fn ungated_interfaces_pass() {
        for name in UNGATED {
            assert!(is_ungated(name), "{name} should be ungated");
        }
    }

    #[test]
    fn gated_interfaces_fail_ungated_check() {
        for name in [
            "clipboard",
            "sql-storage",
            "filesystem",
            "http",
            "opener",
            "command",
            "path-resolver",
            "settings",
            "frecency",
            "website-metadata",
        ] {
            assert!(!is_ungated(name), "{name} should NOT be ungated");
        }
    }

    #[test]
    fn unknown_interface_is_not_ungated() {
        assert!(!is_ungated("invented-interface"));
    }

    // ----- is_provisioned ------------------------------------

    #[test]
    fn provisioned_caps_pass_check() {
        let caps = full_caps();
        // full_caps() provisions these — verify they pass.
        for name in [
            "clipboard",
            "sql-storage",
            "filesystem",
            "http",
            "opener",
            "command",
            "path-resolver",
        ] {
            assert!(
                is_provisioned(name, &caps),
                "{name} should be provisioned with full caps",
            );
        }
    }

    #[test]
    fn unprovisioned_caps_fail_check() {
        let caps = full_caps();
        // full_caps() does NOT provision these (they need
        // Tauri Store / FrecencyStore / MetadataService).
        for name in ["settings", "frecency", "website-metadata"] {
            assert!(
                !is_provisioned(name, &caps),
                "{name} should NOT be provisioned in test full_caps",
            );
        }
    }

    #[test]
    fn no_gated_interface_provisioned_with_empty_caps() {
        let caps = empty_caps();
        for name in [
            "clipboard",
            "sql-storage",
            "filesystem",
            "http",
            "opener",
            "command",
            "path-resolver",
            "settings",
            "frecency",
            "website-metadata",
        ] {
            assert!(
                !is_provisioned(name, &caps),
                "{name} should NOT be provisioned with empty caps",
            );
        }
    }

    #[test]
    fn unknown_interface_is_not_provisioned() {
        assert!(!is_provisioned("invented-interface", &full_caps()));
    }

    #[test]
    fn individual_cap_presence_checked_correctly() {
        let mut caps = empty_caps();
        caps.clipboard = Some(Arc::new(ClipboardCap::new(Box::new(|_| Ok(())))));
        assert!(is_provisioned("clipboard", &caps));
        assert!(!is_provisioned("sql-storage", &caps));
    }

    // ----- validate (integration) ----------------------------

    /// The minimal-gadget fixture only imports `types` (the
    /// shared type definitions) — no gated interfaces. It
    /// passes validation even with empty caps.
    #[test]
    fn validate_passes_for_minimal_gadget_with_empty_caps() {
        let runtime = test_runtime();
        let component = runtime.compile(MINIMAL_GADGET_WASM).expect("compile");
        validate(&component, runtime.engine(), &empty_caps(), "minimal")
            .expect("minimal gadget should pass with no caps");
    }

    /// The opener-http fixture imports `opener` and `http`.
    /// With full caps (which includes both), it passes.
    #[test]
    fn validate_passes_when_gated_imports_are_provisioned() {
        let runtime = test_runtime();
        let component = runtime.compile(OPENER_HTTP_GADGET_WASM).expect("compile");
        validate(&component, runtime.engine(), &full_caps(), "opener-http")
            .expect("should pass when opener+http are provisioned");
    }

    /// The opener-http fixture imports `opener` and `http`.
    /// With empty caps, both should be reported as missing.
    #[test]
    fn validate_rejects_missing_gated_imports() {
        let runtime = test_runtime();
        let component = runtime.compile(OPENER_HTTP_GADGET_WASM).expect("compile");
        let err = validate(&component, runtime.engine(), &empty_caps(), "test-gadget")
            .expect_err("should fail with empty caps");
        let msg = format!("{err}");
        assert!(
            msg.contains("test-gadget"),
            "error should name the gadget: {msg}",
        );
        assert!(
            msg.contains("imports interfaces without matching permissions"),
            "error should describe the problem: {msg}",
        );
        assert!(msg.contains("opener"), "should list opener: {msg}");
        assert!(msg.contains("http"), "should list http: {msg}");
    }

    /// Removing only one cap should list only that one.
    #[test]
    fn validate_lists_only_missing_interfaces() {
        let runtime = test_runtime();
        let component = runtime.compile(OPENER_HTTP_GADGET_WASM).expect("compile");
        let mut caps = full_caps();
        caps.opener = None;
        let err = validate(&component, runtime.engine(), &caps, "test")
            .expect_err("should fail for missing opener");
        let msg = format!("{err}");
        assert!(msg.contains("opener"), "should list opener: {msg}");
        assert!(
            !msg.contains("http"),
            "should NOT list provisioned http: {msg}"
        );
    }

    /// The error message lists missing interfaces in sorted order.
    #[test]
    fn validate_error_is_sorted() {
        let runtime = test_runtime();
        let component = runtime.compile(OPENER_HTTP_GADGET_WASM).expect("compile");
        let err =
            validate(&component, runtime.engine(), &empty_caps(), "test").expect_err("should fail");
        let msg = format!("{err}");
        let start = msg.find('[').expect("has [") + 1;
        let end = msg.find(']').expect("has ]");
        let interfaces: Vec<&str> = msg[start..end].split(", ").collect();
        let mut sorted = interfaces.clone();
        sorted.sort();
        assert_eq!(interfaces, sorted, "missing interfaces should be sorted");
    }
}
