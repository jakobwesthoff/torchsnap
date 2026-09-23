# Command rule binary resolved through attacker-controllable PATH at spawn (allowlist bypass → native code exec)

**Kind:** bug (security)
**Severity:** high
**Area:** src-tauri/src/caps/command.rs, src-tauri/src/wasm/argv_matcher.rs

## Problem
A `[[permissions.command]]` rule binary may be a non-absolute,
PATH-resolved name — the manifest doc states so explicitly
(`src-tauri/src/wasm/manifest/permissions/command.rs:20-27,38-40`),
and parse-time validation only checks non-empty / no-NUL / not in
the opener-class denylist. The compiled rule stores the name
verbatim (`argv_matcher.rs:133-134`), matching is exact-string
`rule.binary == binary` (`argv_matcher.rs:229`), and the spawn is
`tokio::process::Command::new(binary)` (`command.rs:285`). For a
non-absolute name that triggers a PATH search in the child's
environment.

The child environment is built by `build_command_env`
(`command.rs:232-259`): the inherited host env is denylist-filtered
and `PATH` is kept (only empty entries stripped), then the
guest-supplied `options.env` overrides are appended **without any
filtering** (`command.rs:252-256`). The child is spawned with
`.env_clear().envs(env)` (`command.rs:289-290`), so the PATH the
guest set is exactly the PATH `execvp` searches (`Command`'s own
docs: the search path for a relative program name is controlled by
the `PATH` set on the command).

Two hijack vectors, both live on macOS/Linux:

- **(a) guest PATH override.** The gadget sets
  `options.env = [("PATH", "/tmp/attacker")]`, drops a malicious
  file named as the permitted binary (`git`, `mdfind`, …) there,
  and the permitted name resolves to attacker code.
- **(b) inherited PATH.** Even with no override, the inherited host
  PATH may contain a user-writable directory ahead of the real
  binary's directory. Weaker precondition, same outcome.

The guest cannot pass `binary = "/tmp/evil"` (no rule matches), so
the redirection must go through resolution of the legitimate name —
which is exactly what makes it a bypass rather than a blocked call.

## Impact
The spawned child is a native OS process running with the full host
user's privileges, **outside** the WASM sandbox. The binary+argv
allowlist is the confinement boundary: a user who grants "run
`mdfind <query>`" reasonably believes the gadget can only issue
Spotlight searches. This defect collapses that narrow grant into
arbitrary native code execution as the user, defeating the explicit
user-facing security control with trivial effort (set `options.env`
PATH, drop a file). It is a capability-confinement bypass that
escapes the sandbox to native exec.

Severity is High rather than Critical because a consent gate
exists: the user must install the gadget and approve a command
permission. It is not lower than High for four reasons: (1) it
yields sandbox-escaping arbitrary native code running as the user;
(2) it fully defeats the explicit user-facing security control (the
binary+argv allowlist); (3) exploit complexity is trivial (set
`options.env` PATH and drop a file); and (4) the precondition is a
normal, expected grant that users will approve for legitimate
gadgets.

## Suggested fix
Resolve each rule's binary to an absolute path exactly once at
load/compile time (`CommandCap::new` / `argv_matcher::compile_rule`)
against a host-controlled, non-guest-influenceable trusted PATH;
store the resolved `PathBuf` on `CompiledCommandRule` and spawn that
absolute path, never the guest string. Keep the guest `binary`
string only for rule *selection* (the existing exact-match; overlap
detection already guarantees a unique matching rule). Fail closed:
if a name does not resolve at load, cap construction errors early,
consistent with the existing fail-on-first-uncompilable-rule
behaviour (`command.rs:137-140`).

Do **not** require authors to hardcode absolute paths in the
manifest — that breaks portability (`/usr/bin/git` vs
`/opt/homebrew/bin/git` vs `/usr/local/bin/git`) and pushes authors
toward whatever path exists on their box, with no security gain.

Two hardening steps that should land regardless of the above, since
the child does its own PATH-based subprocess resolution (`git`
shells out to `git-*` helpers, `core.pager`, `core.editor`, …):

1. Reset the child PATH to a fixed host-controlled safe PATH
   (e.g. `/usr/bin:/bin:/usr/sbin:/sbin`) rather than inheriting it
   verbatim. This closes vector (b) and the one-level-down variant.
2. Refuse `PATH` (and the dynamic-linker vars) as guest overrides,
   and route the whole override loop through the same
   `is_credential_var` + hard-denylist gate the inherited half uses.
   Today that gate protects only the inherited env; the override
   half enforces nothing. The override-injection angle (incl.
   `LD_PRELOAD`/`DYLD_INSERT_LIBRARIES`) is analysed in
   `01kwh4j9bptrayf451yzd2145f-command-env-override-injection.md`.

Caveat to document: the trusted PATH must contain only host-owned
directories. On default single-user macOS the Homebrew prefixes
(`/opt/homebrew`, `/usr/local`) are admin-writable without sudo, so
treating them as trusted is a conscious weakening; a gadget also
holding a filesystem-write cap under such a prefix could plant a
binary there. A TOCTOU between load-time resolution and spawn is
also theoretically present, but far narrower than today's runtime
PATH search. If Windows becomes a target, absolute resolution is
the only portable safe answer (`Command`'s PATH search has
documented Windows limitations plus implicit cwd/`.exe` semantics).

## Related
- Guest-controlled `cwd` is a second, independent way to turn a
  permitted "safe" binary into code execution (e.g. `git` run in an
  attacker-planted directory with a crafted `.git/config`):
  `01kwh4j9bptrayf451yzd2145e-command-cwd-unvalidated.md`.
- Per-rule constraint fields dropped at compile:
  `01kwg1ajrvfsmxyjmm7bvcs04b-command-per-rule-limits-unenforced.md`.

Together, B3 + the env-override injection + the cwd gap mean any
command grant is currently equivalent to arbitrary native code
execution as the user.
