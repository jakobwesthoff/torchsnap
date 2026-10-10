---
title: "Per-rule command limits for cwd, timeout, output and stdin are parsed but never enforced"
kind: bug
component: gadget-host
impact: high
tags: [capabilities]
---
# Per-rule command limits for cwd, timeout, output and stdin are parsed but never enforced

The four per-rule constraints on `[[permissions.command]]` rules are parsed and documented, but they do not survive rule compilation. The manifest therefore shows reviewers limits with no runtime effect, and a rule's declared default cwd is ignored.

## Problem
`[[permissions.command]]` rules carry four per-rule constraint
fields, parsed and documented in
`src-tauri/src/wasm/manifest/permissions/command.rs:47-69`:

- `cwd`: "default working directory for invocations matching
  this rule. Gadget can override per-call; when omitted, the
  host falls back to `${gadget-data}/exec-cwd/`."
- `timeout-ms-max`: "Requests specifying a longer timeout are
  clamped down."
- `max-output-bytes`, `max-stdin-bytes`: per-rule caps.

None of them survive rule compilation. `CompiledCommandRule`
holds only `binary` and `argv`
(`src-tauri/src/wasm/argv_matcher.rs:65-68`), and `compile_rule`
drops the other four fields. At run time `CommandCap::run`
(`src-tauri/src/caps/command.rs:166-215`):

- uses `argv_matcher::matches` purely as a yes/no gate. It does
  not even learn *which* rule matched, so per-rule enforcement is
  structurally impossible with the current signature;
- clamps timeout and output only against the global constants
  (`DEFAULT_/MAX_COMMAND_TIMEOUT_MS`,
  `DEFAULT_/MAX_COMMAND_OUTPUT_BYTES`, `command.rs:32-35`);
- applies no stdin size limit at all (`options.stdin` is written
  in full, `command.rs:306-313`);
- when the guest omits `cwd`, always uses the scratch directory.
  A rule-declared `cwd` default is never consulted
  (`command.rs:177-189`).

So a manifest saying `timeout-ms-max = 1000` still gets the
60-second global ceiling, `max-output-bytes = 1024` still gets
64 MiB, and a declared rule `cwd` is silently ignored.

## Impact
The manifest's permission surface lies: manifest reviewers read
constraints that have no runtime effect. The install review leaves
them out until they are enforced (`src-tauri/src/gadget_install/review.rs`). Gadgets relying on
a rule-declared default cwd run in the wrong directory.
Separately, the guest-supplied `options.cwd` is used verbatim
with no validation at all. That half is tracked in its own todo,
"Guest-supplied command cwd is unvalidated and per-rule cwd is never enforced".

## Suggested fix
Make `matches` return the matching rule (first match or
best match; overlapping rules need a decision). Thread the
compiled per-rule constraints into `run()`, and clamp or require:
`min(per-rule, global)` for timeout and output, enforce
`max_stdin_bytes` before writing, and use the rule `cwd` when the
call omits one. Alternatively, delete the four manifest fields
until they are real. Parsing and then ignoring them is the worst of both.
