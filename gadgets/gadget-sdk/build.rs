// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

fn main() {
    // wit_bindgen::generate! reads the WIT file at compile time
    // but Cargo caches proc macro expansions based on input
    // tokens, not external files. Without this directive, editing
    // the WIT file does not trigger a recompile of the SDK or
    // any gadget that depends on it.
    println!("cargo:rerun-if-changed=wit/torchsnap-gadget.wit");
}
