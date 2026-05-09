// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

fn main() {
    let git_hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={git_hash}");

    // Cargo caches proc macro expansions based on input tokens,
    // not external files the macro reads. Both wasmtime's
    // bindgen! (host side) and wit_bindgen's generate! (gadget
    // SDK) read this WIT file at compile time. Without this
    // directive, editing the WIT file does not trigger a host
    // recompile — leaving the linker with stale interface
    // signatures that reject rebuilt WASM gadgets.
    println!("cargo:rerun-if-changed=../gadgets/gadget-sdk/wit/torchsnap-gadget.wit");

    tauri_build::build()
}
