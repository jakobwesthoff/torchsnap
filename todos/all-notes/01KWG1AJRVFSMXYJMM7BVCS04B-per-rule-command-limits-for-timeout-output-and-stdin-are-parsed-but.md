---
title: "Per-rule command limits for timeout, output and stdin are parsed but never enforced"
kind: bug
component: gadget-host
impact: high
tags: [capabilities]
---
# Per-rule command limits for timeout, output and stdin are parsed but never enforced

Three of the per-rule constraints on `[[permissions.command]]` rules are parsed and documented, but they do not survive rule compilation. The manifest therefore shows reviewers limits with no runtime effect.

## Problem
`[[permissions.command]]` rules carry three per-rule limit
fields, parsed and documented in
`src-tauri/src/wasm/manifest/permissions/command.rs`:

- `timeout-ms-max`: "Requests specifying a longer timeout are
  clamped down."
- `max-output-bytes`, `max-stdin-bytes`: per-rule caps.

None of them survive rule compilation. `CompiledCommandRule`
(`src-tauri/src/wasm/argv_matcher.rs`) holds `binary`, `argv`,
`cwd` and `env` only, and `compile_rule` drops the three limits. At
run time `CommandCap::run` (`src-tauri/src/caps/command.rs`):

- clamps timeout and output only against the global constants
  (`DEFAULT_/MAX_COMMAND_TIMEOUT_MS`,
  `DEFAULT_/MAX_COMMAND_OUTPUT_BYTES`);
- applies no stdin size limit at all (`options.stdin` is written
  in full).

So a manifest saying `timeout-ms-max = 1000` still gets the
60-second global ceiling, and `max-output-bytes = 1024` still gets
64 MiB. `run` already binds the matching rule (it needs it for the
rule's `cwd`), so per-rule enforcement only has to read the fields.

## Impact
The manifest's permission surface lies: manifest reviewers read
constraints that have no runtime effect. The install review leaves
them out until they are enforced (`src-tauri/src/gadget_install/review.rs`).

## Suggested fix
Thread the compiled per-rule limits into `run()`, and clamp or
require: `min(per-rule, global)` for timeout and output, and enforce
`max_stdin_bytes` before writing. Alternatively, delete the three
manifest fields until they are real. Parsing and then ignoring them
is the worst of both.
