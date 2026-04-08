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
with plain `cargo build --target wasm32-wasip2 --release` and the WIT
bindings are generated via the `wit-bindgen` macro inside each plugin
crate. Do not install `cargo-component` and do not add recipes that
depend on it.

WIT inspection / formatting uses `wasm-tools` (`just check-wit`,
`just fmt-wit`), which is a separate tool.
