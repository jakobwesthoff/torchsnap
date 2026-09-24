---
kind: improvement
severity: low
status: open
area: [just/gadgets.just]
---

# `package-gadget` exclusion list misses `build.rs` (ships in .torchsnap archives)

## Problem

`package-gadget` documents its blacklist as excluding "build
inputs and developer-only artifacts" (`just/gadgets.just:70-100`)
and excludes `Cargo.toml`, `Cargo.lock`, and `src/*` — but not
`build.rs`, which is equally a Rust build input living at the
gadget root. The emoji-picker gadget has one
(`gadgets/emoji-picker/build.rs`), so its `.torchsnap` archive
ships the build script alongside the compiled `.wasm`.

Harmless at runtime (the loader ignores unknown files), but it
contradicts the recipe's own contract ("only the compiled binary
ships"), leaks build-machine details (the script embeds local path
conventions), and slightly bloats archives. Any future gadget
adding `.cargo/` or `rust-toolchain.toml` at its root would ship
those too.

## Suggested fix

Add to the `zip` exclusion list in `package-gadget`
(`gadgets.just:117-131`):

```
-x 'build.rs' \
-x '.cargo/*' \
-x 'rust-toolchain.toml' \
```

Re-run `just build-gadgets` and spot-check
`unzip -l gadgets/emoji-picker.torchsnap` afterwards.
