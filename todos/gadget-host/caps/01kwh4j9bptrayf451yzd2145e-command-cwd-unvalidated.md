# Guest-supplied command `cwd` is unvalidated and per-rule `cwd` is never enforced (working-directory → native code exec)

**Kind:** bug (security)
**Severity:** critical
**Area:** src-tauri/src/caps/command.rs, src-tauri/src/wasm/argv_matcher.rs, src-tauri/src/wasm/manifest/permissions/command.rs

This todo covers two queue items that share one fix: the
guest-supplied working directory used verbatim (B4) and the
per-rule manifest `cwd` that is parsed but never enforced or
root-checked (B8).

## Problem
The full parse→compile→spawn path applies no root check to any
working directory:

- **Guest `cwd` is verbatim, no check.** `CommandCap::run` takes
  `options.cwd` — which flows unmodified from the WIT
  `command-options.cwd` (`runtime/host/command.rs:33`) — and does
  `PathBuf::from(explicit)` (`command.rs:177-178`), then
  `.current_dir(cwd)` at spawn (`command.rs:288`). No
  canonicalization, no root check, no consultation of the matched
  rule.
- **Per-rule `cwd` is dead.** `CommandPermissionDef.cwd`
  (`manifest/permissions/command.rs:50`) is validated only for
  variable references at parse time (`:114-118`,
  `ParseTimeResolver.validate_variable_references`) and then
  dropped: `CompiledCommandRule` carries `{ binary, argv }` only
  (`argv_matcher.rs:65-68`), and `compile_rule` (`:121-137`) never
  reads `raw.cwd`. It never reaches runtime.
- **No canonical-under-root on any cwd.** The
  `path_safety::canonical_under_root` helper is wired only for the
  `path-under` *argv* constraint (`argv_matcher.rs:288`), never for
  cwd. `path_safety.rs` was excluded from this review, but its
  module comment claims "per-rule cwd validation in the manifest
  parser uses canonical-under-root"; the parser code above
  contradicts that, since it does variable-reference validation
  only. Treat the comment as stale. The fix below is what would
  make it true.

When the guest omits `cwd`, the host falls back to the per-gadget
scratch directory `${gadget-data}/exec-cwd/` (`command.rs:180-187`).

## Impact
In the untrusted-gadget threat model the outcome is arbitrary
native code execution as the host user, fully outside the WASM
sandbox. The one gating condition is that the gadget already holds
a `[[permissions.command]]` grant for a config-reading binary —
which is not a meaningful mitigation. The entire design premise
(established by the binary-path-resolution finding) is that the
binary+argv allowlist makes a *narrow* command grant safe to
approve. An unconstrained cwd defeats that premise: a grant a
reviewer reads as "may run `git log`" actually means "may run
arbitrary code." The allowlist looks airtight while the working
directory is wide open. Severity is therefore **critical**, gated
only by an existing command grant.

The mechanism is broader than path traversal. Most CLI tools
resolve config, plugins, or targets relative to cwd, and a gadget
controls the contents of any directory it can write (including its
own `${gadget-data}` tree). Concrete cases:

- **`git`** — a permitted `git <subcommand>` run in an
  attacker-planted directory with a crafted `.git/config`
  (`core.pager`, `core.editor`, `core.fsmonitor`) or
  `.gitattributes` filters executes arbitrary commands.
- **`make`** — a permitted `make <anything>` (or bare `make`)
  executes an attacker-planted `Makefile`/`GNUmakefile` in cwd as
  shell. Direct, unconditional RCE: the Makefile *is* the code.
- **Node toolchain (`npm` / `node` / `eslint`)** — `npm` reads a
  cwd-local `.npmrc` and runs `package.json` lifecycle scripts;
  `eslint .` loads config from cwd and `require()`s plugins from a
  cwd-planted `node_modules`, executing attacker JS. A permitted
  `npm run build` or `eslint .` becomes arbitrary code.
- Same family: `python -m` (cwd on `sys.path` → import hijack),
  `direnv`, `cargo` with a planted `.cargo/config.toml` + build
  script.

Beyond config-hijack of the spawned binary, an unvalidated `cwd`
also turns a permitted *file-creating* binary into an arbitrary
filesystem write/symlink primitive: a rule for `ln`, `touch`, `cp`,
`tee`, `mkdir`, etc. run with `cwd` set to any writable directory
creates or overwrites files anywhere the user can write. That
includes planting a symlink inside another gadget's (or its own)
`DirectorySource` root — the concurrent-writer precondition for the
directory-read TOCTOU
(`../host-wasm/01kwfz4kkaq7spwnm2ncket1g1-directory-read-fallback-raw-path.md`)
— and, more broadly, tampering with any user-writable file on disk.

## Suggested fix
Two independent decisions.

**1. Wire the per-rule `cwd` back in (mechanical, low-risk).**
- Add `cwd: Option<PathBuf>` to `CompiledCommandRule`
  (`argv_matcher.rs:65-68`).
- In `compile_rule` (`:121-137`), resolve `raw.cwd` through the
  same `resolver.substitute_variables` path already used for
  `path-under` roots (`:191-197`), storing the resolved root.
  Variables like `${gadget-data}`, `${home}` then work identically
  to argv roots.
- In `CommandCap::run`, capture the matched rule instead of
  discarding it. `argv_matcher::matches` already returns
  `Ok(&CompiledCommandRule)` (`:223-234`); `run` currently throws
  it away with `.is_err()` (`command.rs:171`). Bind the `Ok(rule)`
  and consult `rule.cwd`.

**2. cwd resolution policy (the security decision).** Make the
per-rule `cwd` an **allowlist root**, not a mere default, mirroring
`path-under` exactly:

| Rule `cwd` | Guest supplies `cwd` | Behavior |
|---|---|---|
| present | yes | `path_safety::canonical_under_root(guest_cwd, rule_root)`; `PermissionDenied` on escape (same helper `path-under` uses, `argv_matcher.rs:288`). |
| present | no  | default to the resolved rule root (author's declared default; `create_dir_all` it as today). |
| absent  | yes | **`PermissionDenied`** — fail closed. A guest cannot pick a cwd the manifest never authorized. |
| absent  | no  | existing `${gadget-data}/exec-cwd/` scratch (`command.rs:180-187`), unchanged. |

This gives cwd the same canonical-under-root guarantee argv paths
already have and makes a guest-supplied cwd an explicit, auditable
grant (a broad root like `${home}` becomes a visible manifest
decision, not an implicit default). Keep the guest able to pass a
cwd — running a tool against a user-selected directory (e.g.
`git log` in a repo the user picked) is inherently dynamic and
cannot be pinned in the manifest — but only under an authorized
root.

**Caveat — do not treat this fix as sufficient for config-reading
binaries.** Path-under-root on cwd bounds *where* relative I/O
lands and blocks the "point cwd at an attacker-planted system
directory" case, but it does **not** close the config-hijack class
for the `git`/`make`/`npm`/`eslint` family, because the gadget can
write the malicious config *inside* the permitted root (including
the `exec-cwd` scratch). The only real defense there is
binary-specific config neutralization (for git:
`GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, an empty
`HOME`) or simply not granting config-reading binaries. That is out
of scope for a generic cwd fix but must be captured as a linked
follow-up: the cwd fix alone does not make `git`/`make`/`npm`
grants safe.

## Related
- Command binary PATH-resolution hijack (same confinement boundary,
  different mechanism, disjoint fix):
  `01kwh4j9bptrayf451yzd2145d-command-binary-path-resolution-hijack.md`.
- Per-rule constraint fields (cwd, timeout, output, stdin) parsed
  but dropped at compile:
  `01kwg1ajrvfsmxyjmm7bvcs04b-command-per-rule-limits-unenforced.md`
  (this todo resolves the cwd half of that finding).
- Env-override injection (the other half of "a command grant is
  arbitrary code exec"):
  `01kwh4j9bptrayf451yzd2145f-command-env-override-injection.md`.
