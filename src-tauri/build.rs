// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::Path;
use std::{env, fs};

fn main() {
    // =========================================================
    // Emojibase Data Embedding
    //
    // Copy the emojibase JSON files from node_modules into
    // OUT_DIR so the emoji plugin can embed them via
    // include_str!(). The Tauri build pipeline runs `bun install`
    // before `cargo build`, so these files exist at build time.
    // =========================================================

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    let out_path = Path::new(&out_dir);

    let emoji_files = [
        (
            "../node_modules/emojibase-data/en/data.json",
            "emoji-data.json",
        ),
        (
            "../node_modules/emojibase-data/en/shortcodes/github.json",
            "emoji-shortcodes-github.json",
        ),
        (
            "../node_modules/emojibase-data/en/shortcodes/emojibase.json",
            "emoji-shortcodes-emojibase.json",
        ),
    ];

    for (src, dest) in &emoji_files {
        let src_path = Path::new(src);
        fs::copy(src_path, out_path.join(dest))
            .unwrap_or_else(|e| panic!("copy {src} to OUT_DIR: {e} — run `bun install` first"));
        println!("cargo:rerun-if-changed={src}");
    }

    tauri_build::build()
}
