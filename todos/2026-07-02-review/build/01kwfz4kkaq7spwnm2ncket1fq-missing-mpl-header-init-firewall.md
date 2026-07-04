# Missing MPL-2.0 header in `.devcontainer/init-firewall.sh`

**Kind:** bug
**Severity:** low
**Area:** .devcontainer/init-firewall.sh

## Problem
Project rule (CLAUDE.md, "MPL-2.0 Source File Headers"): every source
file, including shell scripts, must carry the MPL-2.0 header comment.
A repo-wide sweep on 2026-07-02 over `.rs`, `.ts`, `.tsx`, `.js`,
`.css`, `.html`, `.sh`, `.just`, and `Justfile` files (excluding
`node_modules/`, `target/`, `dist/`) found exactly one file missing
the header:

- `.devcontainer/init-firewall.sh`

All other source files in the repository contain the header.

## Impact
License-header policy violation only; no runtime effect.

## Suggested fix
Add the shell-style header after the shebang line:

```
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
```
