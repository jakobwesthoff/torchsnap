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
