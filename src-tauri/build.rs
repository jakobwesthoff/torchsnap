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

    // Release builds rerun this script when HEAD moves, so they always
    // carry the current hash. Debug builds do not watch git: every rerun
    // recompiles the app, so their hash can lag behind commits.
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        for path in git_head_paths() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

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

/// The files git changes when HEAD of this checkout moves: `HEAD`, the
/// branch ref it names, `packed-refs`, and this worktree's reflog
/// `logs/HEAD`. Git appends to the reflog on every commit, reset and
/// checkout here, also when a commit turns a ref that lived only in
/// `packed-refs` into a loose file, which was not there to be watched.
///
/// Only existing files are returned, because cargo reruns a build
/// script on every build while one of its `rerun-if-changed` paths is
/// missing. Without a working git the list is empty.
fn git_head_paths() -> Vec<std::path::PathBuf> {
    let git = |args: &[&str]| -> Option<String> {
        let output = std::process::Command::new("git").args(args).output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };

    let mut names = vec![
        "HEAD".to_string(),
        "packed-refs".to_string(),
        "logs/HEAD".to_string(),
    ];
    // A detached HEAD names no branch; HEAD itself then holds the commit.
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        names.push(branch);
    }

    // `--git-path` answers relative to the working directory, which cargo
    // sets to the manifest directory; an absolute answer stays as it is.
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    names
        .iter()
        .filter_map(|name| git(&["rev-parse", "--git-path", name]))
        .map(|path| manifest_dir.join(path))
        .filter(|path| path.exists())
        .collect()
}
