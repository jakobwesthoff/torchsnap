# 70. Build the host library as rlib only

Date: 2026-10-23

## Status

Accepted

## Context

`src-tauri/Cargo.toml` built the library `torchsnap_lib` as `staticlib`,
`cdylib` and `rlib`. Torchsnap targets macOS; Linux support is in
progress. Its binary `src/main.rs` links the `rlib`. No recipe, release
step, test or CI job uses the static or the dynamic library. Every
compile of the crate still linked the dynamic library as well and wrote
both into the target directory; the static library of a debug build
alone took 1.25 GB.

Tauri's mobile targets use the other two types. On iOS, the Tauri CLI's
Xcode build step needs the static library `lib<name>.a`. On Android,
cargo-mobile2 links the dynamic library `lib<name>.so` into the app's
`jniLibs`. Both stop with an error when the file is missing.

## Decision

The library is built as `rlib` only, for the desktop targets macOS and
Linux (Linux in progress). A build for iOS needs `staticlib` back in
`crate-type`, a build for Android `cdylib`. Such a change of targets is
considered unlikely.

`run()` keeps `#[cfg_attr(mobile, tauri::mobile_entry_point)]`.
tauri-build sets the `mobile` cfg only for iOS and Android, so on macOS
and Linux the attribute does nothing. On iOS it generates the
`start_app` function that the `main` of Tauri's Xcode project calls, on
Android the JNI binding.

The library keeps its name `torchsnap_lib`. On windows-msvc, a lib built
as `cdylib` next to a bin of the same name collides with it in the
output (cargo issue 8519); the suffix avoids that should `cdylib` come
back.

## Consequences

On an Apple silicon Mac, with the median of three runs after a one-line
change, `cargo build` went from 2.95 s to 2.31 s and
`cargo test --no-run` from 3.04 s to 2.54 s, with one link fewer in
each. A clean `cargo build` went from 47.1 s to 44.5 s, and the target
directory after it and a `cargo test --no-run` is 2.2 GB smaller.
