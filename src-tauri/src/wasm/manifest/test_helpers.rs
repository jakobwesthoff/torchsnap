// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared test fixtures for manifest unit tests.

/// Build a minimal valid manifest TOML, optionally appending extra
/// sections. Used by every manifest sub-module's test suite to avoid
/// repeating the full required-field boilerplate.
pub(crate) fn minimal(extra: &str) -> String {
    format!(
        r#"
        [gadget]
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
