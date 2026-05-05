// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Platform host import
//
// Stateless. Returns the host's OS and CPU architecture via
// `cfg!()` macros at compile time. No permission gate, no
// setters — the values are informational and don't widen
// the sandbox.
// =========================================================

use crate::wasm::bindings;

use super::super::GadgetState;

impl bindings::torchsnap::plugin::platform::Host for GadgetState {
    fn current_os(&mut self) -> bindings::torchsnap::plugin::platform::Os {
        use bindings::torchsnap::plugin::platform::Os;

        if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Other(std::env::consts::OS.to_string())
        }
    }

    fn current_arch(&mut self) -> bindings::torchsnap::plugin::platform::Arch {
        use bindings::torchsnap::plugin::platform::Arch;

        if cfg!(target_arch = "x86_64") {
            Arch::X8664
        } else if cfg!(target_arch = "aarch64") {
            Arch::Aarch64
        } else {
            Arch::Other(std::env::consts::ARCH.to_string())
        }
    }
}
