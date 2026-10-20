---
title: "Command rule binary resolves through the attacker-controllable PATH at spawn"
kind: bug
component: gadget-host
impact: high
tags: [security, capabilities]
---
# Command rule binary resolves through the attacker-controllable PATH at spawn

A `[[permissions.command]]` rule can name its binary without a path, and the spawn then resolves that name through the child's PATH. A gadget that overrides PATH, or a user-writable directory early on the inherited PATH, can point the permitted name at attacker code. The result is native code execution as the user, outside the WASM sandbox, from a grant the user believed was narrow. This is an allowlist bypass, not a blocked call.

## Problem
A `[[permissions.command]]` rule binary may be a non-absolute,
PATH-resolved name. The manifest doc states so explicitly
(`src-tauri/src/wasm/manifest/permissions/command.rs:20-27,38-40`),
and parse-time validation only checks non-empty, no-NUL, and not in
the opener-class denylist. The compiled rule stores the name
verbatim (`argv_matcher.rs:133-134`). Matching is exact-string
`rule.binary == binary` (`argv_matcher.rs:229`), and the spawn is
`tokio::process::Command::new(binary)` (`command.rs:285`). For a
non-absolute name, that triggers a PATH search in the child's
environment.

The child environment is built by `build_command_env`
(`command.rs:232-259`). The inherited host env is denylist-filtered
and `PATH` is kept (only empty entries stripped). The guest-supplied
`options.env` overrides are then appended **without any
filtering** (`command.rs:252-256`). The child is spawned with
`.env_clear().envs(env)` (`command.rs:289-290`), so the PATH the
guest set is exactly the PATH `execvp` searches. `Command`'s own
docs say the search path for a relative program name is controlled by
the `PATH` set on the command.

Two hijack vectors, both live on macOS and Linux:

- **(a) guest PATH override.** The gadget sets
  `options.env = [("PATH", "/tmp/attacker")]`, drops a malicious
  file named as the permitted binary (`git`, `mdfind`, ...) there,
  and the permitted name resolves to attacker code.
- **(b) inherited PATH.** Even with no override, the inherited host
  PATH may contain a user-writable directory ahead of the real
  binary's directory. This is a weaker precondition with the same outcome.

The guest cannot pass `binary = "/tmp/evil"` (no rule matches), so
the redirection must go through resolution of the legitimate name.
That is what makes it a bypass rather than a blocked call.

## Impact
The spawned child is a native OS process running with the full host
user's privileges, **outside** the WASM sandbox. The binary and argv
allowlist is the confinement boundary: a user who grants "run
`mdfind <query>`" reasonably believes the gadget can only issue
Spotlight searches. This defect collapses that narrow grant into
arbitrary native code execution as the user, defeating the explicit
user-facing security control with trivial effort (set the PATH in
`options.env`, drop a file). It is a capability-confinement bypass that
escapes the sandbox to native exec.

Severity is High rather than Critical because a consent gate
exists: the user must install the gadget and approve a command
permission. It is not lower than High for four reasons. (1) It
yields sandbox-escaping arbitrary native code running as the user.
(2) It fully defeats the explicit user-facing security control (the
binary and argv allowlist). (3) Exploit complexity is trivial: set
`options.env` PATH and drop a file. (4) The precondition is a
normal, expected grant that users will approve for legitimate
gadgets.

## Suggested fix
Resolve each rule's binary to an absolute path exactly once at
load or compile time (`CommandCap::new` / `argv_matcher::compile_rule`),
against a host-controlled, non-guest-influenceable trusted PATH.
Store the resolved `PathBuf` on `CompiledCommandRule`, and spawn that
absolute path, never the guest string. Keep the guest `binary`
string only for rule *selection* (the existing exact match; overlap
detection already guarantees a unique matching rule). Fail closed:
if a name does not resolve at load, cap construction errors early,
consistent with the existing fail-on-first-uncompilable-rule
behaviour (`command.rs:137-140`).

Do **not** require authors to hardcode absolute paths in the
manifest. That breaks portability (`/usr/bin/git` vs
`/opt/homebrew/bin/git` vs `/usr/local/bin/git`) and pushes authors
toward whatever path exists on their box, with no security gain.

Two hardening steps should land regardless of the above. The child
does its own PATH-based subprocess resolution (`git` shells out to
`git-*` helpers, `core.pager`, `core.editor`, ...):

1. Reset the child PATH to a fixed host-controlled safe PATH
   (e.g. `/usr/bin:/bin:/usr/sbin:/sbin`) rather than inheriting it
   verbatim. This closes vector (b) and the one-level-down variant.
2. Refuse `PATH` (and the dynamic-linker vars) as guest overrides,
   and route the whole override loop through the same
   `is_credential_var` and hard-denylist gate the inherited half uses.
   Today that gate protects only the inherited env. The override
   half enforces nothing. The override-injection angle, including
   `LD_PRELOAD` and `DYLD_INSERT_LIBRARIES`, is analysed in the separate
   todo "Guest env overrides bypass the credential filter, and the env denylist leaks credentials".

Caveat to document: the trusted PATH must contain only host-owned
directories. On default single-user macOS the Homebrew prefixes
(`/opt/homebrew`, `/usr/local`) are admin-writable without sudo, so
treating them as trusted is a conscious weakening. A gadget also
holding a filesystem-write cap under such a prefix could plant a
binary there. A TOCTOU between load-time resolution and spawn is
also theoretically present, but far narrower than today's runtime
PATH search. If Windows becomes a target, absolute resolution is
the only portable safe answer, since `Command`'s PATH search has
documented Windows limitations plus implicit cwd and `.exe` semantics.

## Relations
- Relates to: [Per-rule command limits for timeout, output and stdin are parsed but never enforced](01KWG1AJRVFSMXYJMM7BVCS04B-per-rule-command-limits-for-timeout-output-and-stdin-are-parsed-but.md), per-rule constraint fields dropped at compile
