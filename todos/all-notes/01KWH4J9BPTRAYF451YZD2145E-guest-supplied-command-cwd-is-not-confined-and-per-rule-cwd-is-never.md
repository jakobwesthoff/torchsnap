---
title: "Guest-supplied command cwd is not confined and per-rule cwd is never enforced"
kind: bug
component: gadget-host
impact: critical
tags: [security, capabilities]
---
# Guest-supplied command cwd is not confined and per-rule cwd is never enforced

A command grant lets the guest choose the working directory of the spawned binary, and no root check applies to it. A permitted tool such as `git`, `make` or `npm` then reads config and plugins from a directory the gadget controls, which turns the grant into arbitrary native code execution as the user. The per-rule `cwd` the manifest declares is parsed but never applied. This todo covers both halves, since one fix addresses both. The cwd policy was decided on 2026-10-20 (see Decisions).

## Problem
Read in the code, not reproduced.

The full parse, compile, and spawn path applies no root check to any
working directory:

- **Guest `cwd` is verbatim, with no check.** `CommandCap::run` takes
  `options.cwd`, which flows unmodified from the WIT
  `command-options.cwd` (`src-tauri/src/wasm/runtime/host/command.rs:33`). It does
  `PathBuf::from(explicit)` (`command.rs:177-178`), then
  `.current_dir(cwd)` at spawn (`command.rs:288`). There is no
  canonicalization, no root check, and no consultation of the matched
  rule.
- **Per-rule `cwd` is dead.** `CommandPermissionDef.cwd`
  (`src-tauri/src/wasm/manifest/permissions/command.rs:50`) is validated only for
  variable references at parse time (`:114-118`,
  `ParseTimeResolver.validate_variable_references`), and then
  dropped. `CompiledCommandRule` carries `{ binary, argv }` only
  (`src-tauri/src/wasm/argv_matcher.rs:65-68`), and `compile_rule` (`:121-137`) never
  reads `raw.cwd`. It never reaches runtime.
- **No canonical-under-root check on any cwd.** The
  `path_safety::canonical_under_root` helper (`src-tauri/src/wasm/path_safety.rs`)
  is wired only for the `path-under` *argv* constraint
  (`argv_matcher.rs:288`), never for cwd. `path_safety.rs` was excluded
  from this review, but its module comment claims "per-rule cwd validation
  in the manifest parser uses canonical-under-root". The parser code above
  contradicts that, since it does variable-reference validation
  only. Treat the comment as stale. The fix below is what would
  make it true.

When the guest omits `cwd`, the host falls back to the per-gadget
scratch directory `${gadget-data}/exec-cwd/` (`command.rs:180-187`).

## Impact
In the untrusted-gadget threat model the outcome is arbitrary
native code execution as the host user, fully outside the WASM
sandbox. The one gating condition is that the gadget already holds
a `[[permissions.command]]` grant for a config-reading binary.
That is not a meaningful mitigation. The entire design premise
(established by the binary-path-resolution finding) is that the
binary and argv allowlist makes a *narrow* command grant safe to
approve. An unconstrained cwd defeats that premise: a grant a
reviewer reads as "may run `git log`" actually means "may run
arbitrary code." The allowlist looks airtight while the working
directory is wide open. Severity is therefore **critical**, gated
only by an existing command grant.

The mechanism is broader than path traversal. Most CLI tools
resolve config, plugins, or targets relative to cwd, and a gadget
controls the contents of any directory it can write, including its
own `${gadget-data}` tree. Concrete cases:

- **`git`**: a permitted `git <subcommand>` run in an
  attacker-planted directory with a crafted `.git/config`
  (`core.pager`, `core.editor`, `core.fsmonitor`) or
  `.gitattributes` filters executes arbitrary commands.
- **`make`**: a permitted `make <anything>` (or bare `make`)
  executes an attacker-planted `Makefile`/`GNUmakefile` in cwd as
  shell. This is direct, unconditional RCE, since the Makefile is the code.
- **Node toolchain (`npm` / `node` / `eslint`)**: `npm` reads a
  cwd-local `.npmrc` and runs `package.json` lifecycle scripts.
  `eslint .` loads config from cwd and `require()`s plugins from a
  cwd-planted `node_modules`, executing attacker JS. A permitted
  `npm run build` or `eslint .` becomes arbitrary code.
- Same family: `python -m` (cwd on `sys.path` gives an import hijack),
  `direnv`, and `cargo` with a planted `.cargo/config.toml` and build
  script.

Beyond config hijack of the spawned binary, an unvalidated `cwd`
also turns a permitted *file-creating* binary into an arbitrary
filesystem write and symlink primitive. A rule for `ln`, `touch`, `cp`,
`tee`, `mkdir`, etc. run with `cwd` set to any writable directory
creates or overwrites files anywhere the user can write. That
includes planting a symlink inside another gadget's (or its own)
`DirectorySource` root. That is the concurrent-writer precondition for the
directory-read race covered by its own todo, and more broadly it allows
tampering with any user-writable file on disk.

## Decisions

Decided by Jakob on 2026-10-20:

- **Threat model: permissions are binding.** What the install review
  shows bounds what a gadget can do. A grant read as "may run
  `git log`" must not mean more. This overrides the trusted-author
  reasoning of ADR 0036 and ADR 0040 for command grants; the
  implementation records it in a new ADR that amends both.
- **cwd policy: only under the rule's `cwd` root**, as in the table
  under Suggested fix, point 2. A guest-supplied cwd is checked with
  `canonical_under_root` against the rule's root; without a rule `cwd`,
  a guest-supplied cwd is `PermissionDenied`.
- **Config-reading binaries are handled by structural rules, not by a
  list of binaries** (a list is always incomplete). Nothing the gadget
  can write may be visible to a spawned program as configuration:
  - The manifest parser rejects a rule `cwd` root that lies in, or has
    an ancestor in, an area the gadget may write. The same holds for
    `path-under` argv roots.
  - Without a rule `cwd`, and for `HOME` and `TMPDIR`, the host creates
    a fresh empty directory per spawn that the gadget cannot reach, and
    removes it afterwards. This replaces the `${gadget-data}/exec-cwd/`
    scratch in the table below.
  - The install review describes every command grant generically (a
    native program runs with the user's rights) and marks
    unconstrained argv; it names no binaries.
  - The env half (declared env values must not point into
    gadget-writable areas) is in the env todo.
  The follow-up todo "Neutralize config loading for config-reading
  binaries in command grants" carries these rules.

## Suggested fix
Two parts; the policy in part 2 is decided (see Decisions), except
that the absent/absent row now uses the fresh per-spawn directory.

**1. Wire the per-rule `cwd` back in (mechanical, low risk).**
- Add `cwd: Option<PathBuf>` to `CompiledCommandRule`
  (`argv_matcher.rs:65-68`).
- In `compile_rule` (`:121-137`), resolve `raw.cwd` through the
  same `resolver.substitute_variables` path already used for
  `path-under` roots (`:191-197`), and store the resolved root.
  Variables like `${gadget-data}` and `${home}` then work identically
  to argv roots.
- In `CommandCap::run`, capture the matched rule instead of
  discarding it. `argv_matcher::matches` already returns
  `Ok(&CompiledCommandRule)` (`:223-234`), but `run` currently throws
  it away with `.is_err()` (`command.rs:171`). Bind the `Ok(rule)`
  and consult `rule.cwd`.

**2. cwd resolution policy (the security decision).** Make the
per-rule `cwd` an **allowlist root**, not a mere default, mirroring
`path-under` exactly:

| Rule `cwd` | Guest supplies `cwd` | Behavior |
|---|---|---|
| present | yes | `path_safety::canonical_under_root(guest_cwd, rule_root)`; `PermissionDenied` on escape (same helper `path-under` uses, `argv_matcher.rs:288`). |
| present | no  | default to the resolved rule root (author's declared default; `create_dir_all` it as today). |
| absent  | yes | **`PermissionDenied`**: fail closed. A guest cannot pick a cwd the manifest never authorized. |
| absent  | no  | existing `${gadget-data}/exec-cwd/` scratch (`command.rs:180-187`), unchanged. |

This gives cwd the same canonical-under-root guarantee argv paths
already have, and makes a guest-supplied cwd an explicit, auditable
grant. A broad root like `${home}` becomes a visible manifest
decision, not an implicit default. Keep the guest able to pass a
cwd: running a tool against a user-selected directory (e.g.
`git log` in a repo the user picked) is inherently dynamic and
cannot be pinned in the manifest. It must stay under an authorized
root.

**Caveat: do not treat this fix as sufficient for config-reading
binaries.** Path-under-root on cwd bounds *where* relative I/O
lands and blocks the "point cwd at an attacker-planted system
directory" case. It does **not** close the config-hijack class
for the `git`/`make`/`npm`/`eslint` family, because the gadget can
write the malicious config *inside* the permitted root, including
the `exec-cwd` scratch. The only real defense there is
binary-specific config neutralization (for git:
`GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, an empty
`HOME`), or simply not granting config-reading binaries. That is out
of scope for a generic cwd fix, but it must be captured as a linked
follow-up. The cwd fix alone does not make `git`/`make`/`npm`
grants safe.

## Relations
- Relates to: [Command rule binary resolves through the attacker-controllable PATH at spawn](01KWH4J9BPTRAYF451YZD2145D-command-rule-binary-resolves-through-the-attacker-controllable-path-at.md), a different mechanism at the same confinement boundary, with a disjoint fix
- Relates to: [Per-rule command limits for cwd, timeout, output and stdin are parsed but never enforced](01KWG1AJRVFSMXYJMM7BVCS04B-per-rule-command-limits-for-cwd-timeout-output-and-stdin-are-parsed-but.md), this todo resolves the cwd half of that finding
- Relates to: [DirectorySource reads: residual check-then-read race and per-read root canonicalization](01KWFZ4KKAQ7SPWNM2NCKET1G1-directorysource-reads-residual-check-then-read-race-and-per-read-root.md), a planted symlink is the concurrent-writer precondition for that race
