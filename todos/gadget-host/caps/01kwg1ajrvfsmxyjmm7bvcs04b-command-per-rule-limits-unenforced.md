# Per-rule command limits (cwd, timeout, output, stdin) are parsed but never enforced

**Kind:** bug
**Severity:** high
**Area:** src-tauri/src/caps/command.rs, src-tauri/src/wasm/argv_matcher.rs

## Problem
`[[permissions.command]]` rules carry four per-rule constraint
fields, parsed and documented in
`src-tauri/src/wasm/manifest/permissions/command.rs:47-69`:

- `cwd` — "default working directory for invocations matching
  this rule. Gadget can override per-call; when omitted, the
  host falls back to `${gadget-data}/exec-cwd/`."
- `timeout-ms-max` — "Requests specifying a longer timeout are
  clamped down."
- `max-output-bytes`, `max-stdin-bytes` — per-rule caps.

None of them survive rule compilation. `CompiledCommandRule`
holds only `binary` and `argv`
(`src-tauri/src/wasm/argv_matcher.rs:65-68`); `compile_rule`
drops the other four fields. At run time `CommandCap::run`
(`src-tauri/src/caps/command.rs:166-215`):

- uses `argv_matcher::matches` purely as a yes/no gate — it does
  not even learn *which* rule matched, so per-rule enforcement is
  structurally impossible with the current signature;
- clamps timeout and output only against the global constants
  (`DEFAULT_/MAX_COMMAND_TIMEOUT_MS`,
  `DEFAULT_/MAX_COMMAND_OUTPUT_BYTES`, `command.rs:32-35`);
- applies no stdin size limit at all (`options.stdin` is written
  in full, `command.rs:306-313`);
- when the guest omits `cwd`, always uses the scratch directory
  — a rule-declared `cwd` default is never consulted
  (`command.rs:177-189`).

So a manifest saying `timeout-ms-max = 1000` still gets the
60-second global ceiling, `max-output-bytes = 1024` still gets
64 MiB, and a declared rule `cwd` is silently ignored.

## Impact
The manifest's permission surface lies: reviewers (and the
future permission-consent UI, see
`todos/gadget-host/wasm/01kq7x2ge7d3ykf7vxkz4fvr6a-install-time-permission-consent.md`)
read constraints that have no runtime effect. Gadgets relying on
a rule-declared default cwd run in the wrong directory.
Separately, the guest-supplied `options.cwd` is used verbatim
with no validation at all; see
`01kwh4j9bptrayf451yzd2145e-command-cwd-unvalidated.md` for that
half of the problem.

## Suggested fix
Make `matches` return the matching rule (first match or
best match — needs a decision for overlapping rules), thread the
compiled per-rule constraints into `run()`, and clamp/require:
`min(per-rule, global)` for timeout/output, enforce
`max_stdin_bytes` before writing, and use rule `cwd` when the
call omits one. Alternatively, delete the four manifest fields
until they are real — parsing-but-ignoring is the worst of both.
