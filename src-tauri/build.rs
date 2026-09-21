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

    // `tauri.conf.json` bundles the whole staging directory that
    // `just stage-bundled-gadgets` fills. tauri-build rejects a
    // resource path that does not exist but accepts an empty
    // directory, so creating it here lets `cargo check`, clippy and
    // the tests run on a checkout where no gadget was staged yet.
    let bundled_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/bundled-gadgets");
    std::fs::create_dir_all(&bundled_dir).expect("the repository root is writable");
    println!("cargo:rerun-if-changed=../target/bundled-gadgets");

    // An empty `gadgets/bundled.toml` stages nothing on purpose, so an
    // empty directory is not an error. It is flagged for release
    // builds because the usual cause is running `tauri build` without
    // `just build`, which would ship an app with no bundled gadgets.
    let has_staged_gadget = std::fs::read_dir(&bundled_dir)
        .expect("the directory was created above")
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "torchsnap")
        });
    if !has_staged_gadget && std::env::var("PROFILE").as_deref() == Ok("release") {
        println!(
            "cargo:warning=target/bundled-gadgets/ holds no .torchsnap archive; \
             this release bundles no gadgets. Build with `just build --release`."
        );
    }

    tauri_build::build()
}
