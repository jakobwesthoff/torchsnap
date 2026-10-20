---
title: "Guest env overrides bypass the credential filter, and the env denylist leaks credentials"
kind: bug
component: gadget-host
impact: critical
tags: [security, capabilities]
---
# Guest env overrides bypass the credential filter, and the env denylist leaks credentials

The spawned command's environment is built by filtering the inherited host env through a denylist, but the guest's own `options.env` overrides skip that filter. A gadget can set `LD_PRELOAD`, `DYLD_INSERT_LIBRARIES` or an interpreter hijack variable and run attacker code inside a permitted binary. The denylist also misses many real credentials in the inherited half. The threat model and the env policy were decided on 2026-10-20 (see Decisions).

## Decisions

Decided by Jakob on 2026-10-20:

- **Threat model: permissions are binding.** What the install review
  shows bounds what a gadget can do, so sub-issue 2 is a real escape
  and both sub-issues get fixed. This overrides ADR 0040's env handling
  and its rejection of a manifest env allowlist, and the trusted-author
  reasoning of ADR 0036 for command grants; the implementation records
  it in a new ADR that amends both.
- **Env is declared in the manifest now** (option (c) under Suggested
  fix): a command rule declares the env keys it may set, the install
  review shows them, and runtime overrides are limited to the declared
  keys. An undeclared override is rejected.
- **Hard rejection even when declared:** `LD_*`, `DYLD_*`, `PATH` and
  the interpreter and tool-hijack set listed under Suggested fix,
  point 2, cannot be declared; the manifest parser rejects them.
- **Inherited base is an allowlist** (Suggested fix, point 1): locale
  and `TZ`, a fixed `PATH`, and `HOME` and `TMPDIR` pointing at a fresh
  per-spawn directory the host creates and the gadget cannot reach (the
  structural rule from the cwd todo).
- **Declared env values must not point into areas the gadget may
  write**, or a config pointer such as `GIT_CONFIG_GLOBAL` would plant
  configuration. How this is enforced for guest-supplied values (fixed
  values in the manifest, value patterns, or a path check) is for the
  implementation plan.

## Problem
Read in the code, not reproduced.

`build_command_env` (`src-tauri/src/caps/command.rs:232-259`) builds the child env by
filtering the inherited host env through `is_credential_var`
(`command.rs:222-230`). That is a hard denylist plus an uppercase-suffix
list. It keeps `PATH` (empty entries stripped), then appends the
guest-supplied `options.env` overrides. The child is spawned with
`.env_clear().envs(env)` (`command.rs:289-290`).

```rust
const ENV_HARD_DENYLIST: &[&str] = &[
    "LD_PRELOAD","LD_LIBRARY_PATH","DYLD_INSERT_LIBRARIES","DYLD_LIBRARY_PATH",
    "SSH_AUTH_SOCK","GPG_AGENT_INFO",
];
const ENV_CREDENTIAL_SUFFIXES: &[&str] =
    &["_TOKEN","_KEY","_PASSWORD","_SECRET","_PASS","_PASSWD","_APIKEY"];
```

Two confirmed sub-issues:

1. **The denylist leaks real credential vars (a bug regardless of the threat model).**
   The filter only strips inherited vars whose name follows
   the `WORD_<SUFFIX>` convention with a suffix in the list, or is on
   the hard list. Everything else passes through. This misses
   high-value secrets (`PGPASSWORD`, `MYSQL_PWD`, `DATABASE_URL`, and
   others, full list below) and the entire remaining host env. It is a bug
   against ADR 0040's *own* stated goal of stripping accidental
   credential inheritance, and it stands regardless of the trust model.

2. **Guest overrides bypass the filter entirely (the sharp one).**
   The override loop (`command.rs:252-256`) appends guest-supplied
   keys with **no** `is_credential_var` check. A gadget sets
   `options.env = [("LD_PRELOAD","/tmp/evil.so")]` (or
   `DYLD_INSERT_LIBRARIES` on macOS, or a family of interpreter or tool
   hijack vars), and it lands in the child env. The denylist stripped
   it from the inherited half, but the gadget re-injects it, running
   attacker code inside the permitted binary at spawn.

**Critical caveat: this is documented, intentional design, not an
oversight.** ADR 0040 ("WASM plugin command API",
`docs/adr/0040-wasm-plugin-command-api.md:196-209`) specifies exactly
this: "Base env = host process env minus a denylist, then plugin
overrides layered on top ... A plugin can re-set any stripped variable
explicitly via `command-options.env` if it genuinely needs to." Its
"Rejected alternatives" section (`:335-340`) rejects a manifest env
allowlist (`env-allowed = [...]`) on the reasoning that a malicious
manifest *author* is out of scope (ADR 0036 trust model): "no
security distinction between a manifest allowlist and free per-call
env overrides, since both are equally controlled by the plugin
author. Either both are dangerous or neither is." So this is not
"the code diverges from intent". The documented intent is wrong
under the untrusted-gadget threat model this security review assumes.

**The todo's first required decision is therefore the threat model.**
Sub-issue 2's severity depends entirely on which model wins (argued
below). Sub-issue 1 is a bug either way.

## Impact

### Severity
- **Sub-issue 2: critical** under the untrusted-gadget model:
  a deterministic sandbox escape from a bounded
  `[[permissions.command]]` grant to arbitrary native code as the
  user. It **downgrades to low or informational** only if gadget authors
  are considered fully trusted. Then env injection adds nothing an
  author couldn't get by declaring `binary = "/bin/sh"`.
- **Sub-issue 1: high and independent of the threat model.** It is a bug against
  ADR 0040's own credential-stripping goal, and it stands even if the ADR's
  trust model is kept.
- Record the todo overall as **critical, gated on a threat-model decision**,
  with sub-issue 1 as an independently valid high.

### Code-execution chain (sub-issue 2), verified end to end
There is zero validation at any hop:
- WIT: `command-options.env: list<tuple<string,string>>`
  (`gadgets/gadget-sdk/wit/torchsnap-gadget.wit:522`). The doc comment
  says the host env is stripped. It says nothing about filtering the
  gadget's own overrides.
- The bridge `From` impl is a straight move, `env: o.env`, with no filter
  (`src-tauri/src/wasm/runtime/host/command.rs:29-40`).
- The host `command::run` checks only `(binary, argv)` against the rules.
  It never inspects `options.env` (`src-tauri/src/wasm/runtime/host/command.rs:75-107`).
- `build_command_env` filters the inherited half but pushes guest
  keys verbatim (`command.rs:252-256`), then `.env_clear().envs(env)`
  spawns (`command.rs:289-290`).
- The manifest cannot constrain env. `CommandPermissionDef` has
  `binary`, `argv`, `cwd`, `timeout_ms_max`, `max_output_bytes`, and
  `max_stdin_bytes`, and **no env field** (`src-tauri/src/wasm/manifest/permissions/command.rs:34-70`).

**LD_PRELOAD (Linux, universal):** glibc's loader loads the named
`.so` and runs its ELF constructors before `main`, for any
dynamically-linked child. This is ungated for normal (non-setuid, non-AT_SECURE)
binaries running under the user's uid. The payload prerequisite is low:
a gadget with any filesystem-write cap writes the `.so` into its
`gadget-home/<id>/`, and the command cap itself auto-creates a
writable `exec-cwd/` scratch dir that is the child's cwd
(`command.rs:180-187`). LD_PRELOAD can also target any pre-existing
readable `.so`.

**No-payload-file variants (these widen the finding well beyond
LD_PRELOAD).** These need no attacker file, only that the permitted
binary is or invokes the interpreter:
- `NODE_OPTIONS=--require /path`: any `node` or node tool.
- `GIT_SSH_COMMAND="sh -c '...'"` or `GIT_SSH`: a permitted `git` doing a
  remote op.
- `BASH_ENV=/path`: sourced by non-interactive `bash`.
- `PERL5OPT=-Mevil`, `PYTHONSTARTUP`, `PYTHONPATH`, `RUBYOPT`,
  `PERL5LIB`: per interpreter.
- `PAGER`, `GIT_PAGER`, `LESSOPEN=|cmd %s`, `EDITOR`, `VISUAL`,
  `GIT_EXTERNAL_DIFF`: commands the tool shells out to.
- `PATH` re-point: the permitted binary's own subprocess lookups
  resolve attacker binaries (the binary-path resolution todo).

So "run one whitelisted binary with constrained argv" becomes "run
arbitrary native code as the user."

**macOS DYLD nuance:** `DYLD_INSERT_LIBRARIES` and `DYLD_LIBRARY_PATH` are
stripped by dyld for SIP and system binaries (`/usr/bin/*`, `/bin/*`), for
hardened-runtime-signed binaries lacking the
`com.apple.security.cs.allow-dyld-environment-variables` entitlement,
and for restricted or setuid binaries. This does **not** save Torchsnap.
The permitted binaries are typically Homebrew, cargo, go, or npm CLI tools,
which are routinely ad-hoc-signed or signed without the hardened
runtime and therefore honor `DYLD_INSERT_LIBRARIES`. The attack lands
against exactly the class of binary this feature exists to run. It is
blocked only when the permitted binary happens to be an Apple
system (SIP) or hardened binary. The `NODE_OPTIONS`, `GIT_SSH_COMMAND`,
`BASH_ENV`, and `PATH` variants have no dyld mitigation at all. Linux has
no equivalent mitigation.

**Exfiltration:** if the permitted binary is `env`, `printenv`, or a shell,
or the LD_PRELOAD payload reads `environ`, the gadget reads the entire
leaked child env back over stdout. That turns sub-issue 1 into active
credential theft rather than passive spill.

## Why the ADR's rejection rationale is flawed (the argument this todo needs)
ADR 0040 rejects a manifest env allowlist by conflating the **trust
model** (is the author malicious?) with the **least-privilege and consent
boundary** (does the granted capability match what the user
believed they granted?). There are two concrete flaws:

1. **The manifest is the consent surface, and per-call env is invisible.**
   `[[permissions.command]]` is a graduated, reviewable grant. A user
   granting `binary = "/usr/bin/pandoc"` concludes "worst case, it
   converts documents." Per-call `command-options.env` is declared
   nowhere and shown to no one. `LD_PRELOAD` or `NODE_OPTIONS` silently
   promote that narrow grant to "arbitrary code as the user." A
   manifest allowlist omitting `LD_*`, `DYLD_*`, and `PATH` keeps pandoc as
   pandoc. That is the security distinction the ADR says doesn't
   exist. The ADR's claimed benefit, "manifest review sees the explicit
   set", is false for the injection case, because the reviewer never sees an
   `LD_PRELOAD` that wasn't in the manifest.
2. **"The author could just declare `/bin/sh`" only holds if the user
   consents equally to every binary.** They don't. The command system
   is graduated-trust, and env injection collapses the graduation. Users
   are far likelier to grant `/usr/bin/pandoc` than `/bin/sh`.

Under the review's untrusted-gadget premise, the ADR's out-of-scope
author assumption does not hold, and sub-issue 2 is a real escape. If
Jakob decides gadgets are genuinely fully trusted (curated store and
code review), sub-issue 2 is informational and only sub-issue 1
remains. This is the decision the todo must force first.

## Concrete leaking-var list (self-contained; today's denylist misses all of these)
Correctly caught, for contrast: `AWS_SECRET_ACCESS_KEY` (`_KEY`),
`AWS_SESSION_TOKEN` (`_TOKEN`), `GITHUB_TOKEN` and `GH_TOKEN` (`_TOKEN`),
`OPENAI_API_KEY` and `ANTHROPIC_API_KEY` (`_KEY`), `DB_PASSWORD`
(`_PASSWORD`). Missed:

**Category A: real secrets whose name has an unlisted suffix (high).**
- `PGPASSWORD`: libpq password. `"PGPASSWORD".ends_with("_PASSWORD")`
  is false (no underscore before `PASSWORD`). Very common. Leaks.
- `MYSQL_PWD`: MySQL password. `_PWD` is unlisted. Leaks.
- Bare names `PASSWORD`, `TOKEN`, `SECRET`, `APIKEY`, `PASSPHRASE`:
  no leading underscore, no suffix match.
- Connection strings embedding `user:password@host`: `DATABASE_URL`,
  `REDIS_URL`, `POSTGRES_URL`, `MONGODB_URI`, `CLICKHOUSE_URL`
  (`_URL` and `_URI` are unlisted).
- `SENTRY_DSN` (`_DSN`, embeds an auth key), `GOOGLE_APPLICATION_CREDENTIALS`
  (`_CREDENTIALS`, a path to a service-account key), `AWS_WEB_IDENTITY_TOKEN_FILE`
  (`_FILE`, a path to an OIDC token), `KUBECONFIG` (cluster certs and tokens).
- `NPM_AUTH`, `REDISCLI_AUTH`, bare `AUTHORIZATION` and `AUTH` (`_AUTH`
  is unlisted); `SLACK_WEBHOOK_URL`, `DISCORD_WEBHOOK`, `TEAMS_WEBHOOK`.

**Category B: credential-adjacent identifiers (medium disclosure).**
`AWS_ACCESS_KEY_ID` (`_ID`, the access-key half), `AZURE_CLIENT_ID`,
`AZURE_TENANT_ID`, `AWS_ACCOUNT_ID`, `TWILIO_ACCOUNT_SID`.

**Category C: the entire remaining host env (why to flip to an
allowlist).** `HOME`, `USER`, `LOGNAME`, `SHELL`, `PWD`, `PATH`,
`TMPDIR`, `XDG_*`, `SSH_CLIENT` and `SSH_CONNECTION`, toolchain shims
(`NVM_*`, `ASDF_*`, `PYENV_*`, `RBENV_*`, `CARGO_*`, `GOPATH`), CI
context (`GITHUB_*`, `CI_*`, `GITLAB_*`), and every bespoke var the
user exported. On a developer's own machine (this is a dev tool) that
env is saturated with secrets. The credential-var namespace is
unbounded, so a denylist is the wrong shape.

## Suggested fix
Two independent changes, and both should land. The first is
threat-model-independent, and the second is the escape fix.

**1. Flip the inherited base from denylist to allowlist (fixes
sub-issue 1 and achieves ADR 0040's stated goal properly).** Inherit
nothing. Build the child env from a fixed, minimal, auditable set:
- Locale and encoding: `LANG`, `LANGUAGE`, `LC_ALL`, `LC_CTYPE`, all
  `LC_*`, `TZ`. `TERM` only if a target tool needs it (child stdio is
  piped, `command.rs:291-297`).
- `PATH`: do **not** inherit. Supply a fixed constant (e.g.
  `/usr/bin:/bin:/usr/local/bin`, plus `/opt/homebrew/bin` on Apple
  Silicon). The manifest `binary` is already concrete, so PATH is only
  needed for the child's own subprocess lookups. A constant avoids
  leaking the developer's PATH and closes the PATH-repoint vector on
  the inherited half. It subsumes the existing `strip_empty_path_entries`.
- `HOME` and `TMPDIR`: point at isolated per-gadget dirs (under
  `gadget-home/<id>/`), not host values. This stops the child reaching
  `~/.aws/credentials`, `~/.ssh`, `~/.netrc`, and `~/.gitconfig`, which are exactly
  the file-based secrets the denylist can't see. The trade-off: tools
  expecting user config won't find it, so provision explicitly where a
  gadget legitimately needs it.
- `USER` and `LOGNAME`: omit by default (identity disclosure). Synthesize
  a placeholder only if a tool requires it.

The allowlist fails **closed**, so some tool will want a var you
didn't allow. Start minimal, expand deliberately per concrete need,
and in debug builds log any dropped override or inherited var for easy
diagnosis. That is the correct trade: bounded and auditable, versus a
denylist that fails open and is provably incomplete.

**2. Apply one shared policy to overrides too, and hard-reject
exec-enabling keys (fixes sub-issue 2).** The root cause is an
asymmetry: the filter runs on the inherited half and not on the override
half. There must be exactly one env policy applied to both branches.
Even keeping today's denylist, running it over the overrides would
kill the `LD_PRELOAD` re-injection. Pair the symmetry fix with a hard
override-rejection set:
- Reject any override key with prefix `LD_` or `DYLD_` outright
  (covers `LD_PRELOAD`, `LD_LIBRARY_PATH`, `LD_AUDIT`,
  `DYLD_INSERT_LIBRARIES`, `DYLD_LIBRARY_PATH`,
  `DYLD_FALLBACK_LIBRARY_PATH`, and future variants). Prefix rejection
  beats enumeration.
- Reject `PATH` as an override (this ties to the binary-path resolution todo).
- Reject the interpreter and tool-hijack set: `NODE_OPTIONS`, `PYTHONPATH`,
  `PYTHONSTARTUP`, `PERL5LIB`, `PERL5OPT`, `RUBYOPT`, `RUBYLIB`,
  `BASH_ENV`, `ENV`, `IFS`, `GIT_SSH`, `GIT_SSH_COMMAND`,
  `GIT_EXTERNAL_DIFF`, `GIT_PAGER`, `PAGER`, `LESSOPEN`, `EDITOR`,
  `VISUAL`, `BROWSER`, `SHELL`.
- Secondary hardening: reject keys that are empty, contain `=` or NUL,
  or values containing NUL. The bridge does no such check today.

Override *policy* overall has three options, of increasing restriction:
- **(a) No guest env overrides at all.** Drop or ignore
  `command-options.env`, or error on non-empty. This is the simplest and safest
  option, but it loses cases like `GIT_AUTHOR_NAME`. No evidence in the repo that any
  gadget needs overrides, so this is a defensible immediate stopgap.
- **(b) Runtime allowlist of override keys.** The guest may set only keys
  on a safe allowlist (values arbitrary), never the rejected set.
  This is bounded, but values remain author-controlled.
- **(c) Manifest-declared env (revisit ADR 0040's rejection).** The
  gadget declares permitted env keys (optionally fixed values or value
  patterns) in its manifest, consented at install like other
  permissions. Runtime overrides are intersected with the declaration.
  This is most consistent with Torchsnap's model, since command rules are already
  manifest-declared and consented, and it is exactly what ADR 0040 rejected.
  The rejection holds only under the trusted-author assumption.
  The trade-off is schema and consent-UI work.

Recommendation: ship **(2)**'s shared policy and hard-reject set
immediately. This alone converts the critical escape into "gadget
can set a few benign vars". Adopt **(c)** as the north star, which requires
re-opening ADR 0040. Run with **(a)** in the interim if (c) is
deferred, since (b)'s marginal benefit over (a) is small until real
gadget needs appear.

**Design principle to record verbatim:** one env policy, applied
identically to the inherited base and to guest overrides. The base is
an allowlist (a fixed minimal set, a controlled PATH, and isolated
HOME and TMPDIR). Overrides are filtered through the same allowlist plus a
hard rejection of `LD_*`, `DYLD_*`, `PATH`, and interpreter-hijack keys. The
current split, which filters one branch and trusts the other, is the exact bug.
A denylist on either branch is the wrong shape, because the
credential and exec-enabling namespaces are unbounded.

## Files
- `src-tauri/src/caps/command.rs:37-54` (denylist and suffix constants), `:222-259`
  (`is_credential_var`, `build_command_env`, unfiltered override loop
  `:252-256`), `:180-187` (auto-created writable `exec-cwd`),
  `:289-290` (`.env_clear().envs(env)`).
- `src-tauri/src/wasm/runtime/host/command.rs:29-40` (unfiltered `From`), `:75-107`
  (host `run`).
- `gadgets/gadget-sdk/wit/torchsnap-gadget.wit:513-533`
  (`command-options`, `env` at `:522`).
- `src-tauri/src/wasm/manifest/permissions/command.rs:34-70` (`CommandPermissionDef`, no
  env field).
- `docs/adr/0040-wasm-plugin-command-api.md:196-209` (env handling),
  `:335-340` (rejected manifest allowlist). ADR 0036 (trust model).
  Both must be revisited as part of resolving this todo.

## Relations
- Relates to: [Command rule binary resolves through the attacker-controllable PATH at spawn](01KWH4J9BPTRAYF451YZD2145D-command-rule-binary-resolves-through-the-attacker-controllable-path-at.md), the PATH-override half of this escape
- Relates to: [Guest-supplied command cwd is not confined and per-rule cwd is never enforced](01KWH4J9BPTRAYF451YZD2145E-guest-supplied-command-cwd-is-not-confined-and-per-rule-cwd-is-never.md), the other half of "a command grant is arbitrary code exec"
