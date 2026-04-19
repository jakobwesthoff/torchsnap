# Torchsnap Development Guidelines

## License

This project is licensed under the Mozilla Public License Version 2.0 (MPL-2.0).

### MPL-2.0 Source File Headers

**Every source file** in this project MUST include the MPL-2.0 header comment
at the top. This is a strict requirement — not optional.

When creating a new source file, add the appropriate header as the first line
(after shebang lines in scripts). When encountering an existing source file
that is missing the header, add it immediately regardless of the current task.

#### Header formats by file type

**Rust (.rs), TypeScript (.ts, .tsx), JavaScript (.js):**
```
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
```

**CSS (.css):**
```
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
```

**HTML (.html):**
```
<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
```

**Shell scripts (.sh), Justfiles (.just), Makefiles:**
```
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
```

Files that do NOT need headers: `.json`, `.toml`, `.lock`, `.md`, images,
and other non-source configuration files.

## Tooling notes

### `cargo-component` is NOT used

This project does **not** use `cargo-component`. WASM plugins are built
with plain `cargo build --release` from the `plugins/` virtual
workspace (the `wasm32-wasip2` target is set as the workspace default
via `plugins/.cargo/config.toml`). The WIT `wit_bindgen::generate!`
invocation lives in the `torchsnap-plugin-sdk` crate
(`plugins/plugin-sdk/`); plugin crates consume the generated bindings
through `use torchsnap_plugin_sdk::prelude::*;` and register themselves
via `define_plugin!(MyPlugin)`. Do not install `cargo-component` and do
not add recipes that depend on it.

WIT inspection / formatting uses `wasm-tools` (`just check-wit`,
`just fmt-wit`), which is a separate tool.

## Plugin build pipeline

Release bundles ship only the plugins whitelisted in
`plugins/bundled.toml`. The `stage-bundled-plugins` Just recipe
reads the whitelist, rebuilds each listed plugin, and copies the
resulting `.torchsnap` archives into `target/bundled-plugins/`,
which Tauri picks up via the `resources` entry in
`tauri.conf.json`. `just build` runs this staging step before
`tauri build` automatically; no manual orchestration needed.

The repo-root `target/` directory is gitignored — it is owned
entirely by this staging flow. Cargo itself uses
`src-tauri/target/` for the host and `plugins/target/` for the
plugin virtual workspace.

To add a plugin to release bundles, edit `plugins/bundled.toml`
and rebuild. To develop a plugin without adding it to release
bundles, just keep its source under `plugins/<id>/` — the debug
loader scans that directory automatically (ADR 0035).

## Plugin storage layout

Host-managed per-plugin state (SQLite databases, future blob /
cache sibling directories) lives under
`<app_data_dir>/plugin-home/<plugin-id>/`, separate from plugin
code which lives under `<app_data_dir>/plugins/`. SQLite files
use the `.sqlite3` extension project-wide (not `.db`). See
ADR 0035 (distribution) and ADR 0018 (SQL storage) for details.
