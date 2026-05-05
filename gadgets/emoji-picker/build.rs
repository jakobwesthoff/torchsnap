// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Emojibase Data Embedding
//
// Copies the `emojibase-data` JSON files from the sibling
// `frontend/node_modules/` tree into `OUT_DIR` so `src/lib.rs`
// can embed them into the wasm binary via `include_str!`.
//
// The `.torchsnap` archive intentionally strips
// `frontend/node_modules/*` (see `just/plugins.just` —
// `package-plugin` exclusion list), which means the guest
// cannot read these files at runtime. Embedding at compile
// time is the simplest workaround for the v1 plugin ABI. A
// future `plugin-assets` WIT interface
// (`todos/wasm/…plugin-assets-wit-interface.md`) would let
// the guest load them from the archive and shrink the binary.
//
// `just build-plugin emoji-picker` runs `bun install` in
// `frontend/` before invoking Cargo, so `node_modules/` is
// guaranteed to exist by the time this build script runs.
// =========================================================

use std::path::Path;
use std::{env, fs};

fn main() {
    let out_dir = env::var("OUT_DIR").expect("cargo provides OUT_DIR");
    let out_path = Path::new(&out_dir);

    // (source path relative to the crate root, destination filename in OUT_DIR).
    let emoji_files = [
        (
            "frontend/node_modules/emojibase-data/en/data.json",
            "emoji-data.json",
        ),
        (
            "frontend/node_modules/emojibase-data/en/shortcodes/github.json",
            "emoji-shortcodes-github.json",
        ),
        (
            "frontend/node_modules/emojibase-data/en/shortcodes/emojibase.json",
            "emoji-shortcodes-emojibase.json",
        ),
    ];

    for (src, dest) in &emoji_files {
        let src_path = Path::new(src);
        fs::copy(src_path, out_path.join(dest)).unwrap_or_else(|e| {
            panic!(
                "copy `{src}` into OUT_DIR: {e} \
                 — run `bun install` in `plugins/emoji-picker/frontend/` \
                 (or build via `just build-plugin emoji-picker` which does this automatically)"
            )
        });
        // Re-run when the source JSON changes (e.g. after
        // `emojibase-data` is upgraded in package.json).
        println!("cargo:rerun-if-changed={src}");
    }
}
