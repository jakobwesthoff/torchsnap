# 40. WASM plugin command API

Date: 2026-04-27

## Status

Accepted

## Context

ADR 0036 set the trust-model invariant: the WIT capability surface is
the primary sandbox, and any new capability — particularly anything
that reaches outside the wasmtime guest — requires a deliberate WIT
addition reasoned about in its own ADR. ADR 0036 explicitly listed
"no process spawning" among the things WASM plugins cannot do today.

A meaningful class of legitimate plugins is blocked by that gap.
The standing example is the `app-launcher` plugin, which today is a
native Rust plugin because it must run `mdfind` on macOS, enumerate
`.desktop` files on Linux, and launch applications via the OS
handler. More generally: any plugin that wraps a CLI (git, kubectl,
package managers, system utilities, custom helper binaries) cannot
be written as a WASM plugin. The launcher genre attracts these
plugins; not having a path for them keeps useful functionality
trapped in the native host.

Raycast solves the same problem with a fundamentally different
trust model — extensions are full Node.js code with unrestricted
`child_process` access, kept honest by mandatory open-source review
and a centralized store. Torchsnap's distribution model (per ADR
0035) has none of those compensating controls: archives are
user-installable from arbitrary sources, no central review, no
signing today. Copying Raycast's runtime posture would invalidate
the WIT-sandbox premise the rest of the system rests on.

The design question this ADR settles: how do we let plugins spawn
system processes while keeping the WIT shape — not a permissive
runtime — as the security boundary? The answer this ADR commits to
is a manifest-declared capability with per-rule argv constraints,
matching the pattern already established by `[permissions.http]`
and `[permissions.opener]` and extending it with the additional
verification a heavier capability needs.

## Decision

### A new `command` host interface, synchronous and bounded

Add a `command` interface to the WIT world with one function:

```wit
interface command {
    record command-options {
        args:             list<string>,
        cwd:              option<string>,
        env:              list<tuple<string, string>>,
        stdin:            option<list<u8>>,
        timeout-ms:       option<u32>,
        max-output-bytes: option<u64>,
    }

    record command-result {
        exit-code:  option<s32>,            // none = killed by signal
        signal:     option<string>,
        timed-out:  bool,
        stdout:     list<u8>,
        stderr:     list<u8>,
    }

    variant command-error {
        permission-denied(string),
        spawn-failed(string),
        timeout,
        output-too-large(tuple<list<u8>, list<u8>>),    // (stdout, stderr)
    }

    run: func(binary: string, options: command-options)
         -> result<command-result, command-error>;
}
```

The function is synchronous from the guest's perspective. The host
runs the spawn on tokio via `tokio::task::block_in_place`, mirroring
the bridge pattern established by `http::fetch` (ADR 0038). No
shell is invoked: argv goes directly to `Command::new(binary).args
(args)`. No process handles are returned; no streaming; no PTY.
The `Mutex<Store<PluginState>>` already serializes guest calls per
plugin, so a plugin has at most one in-flight `run` at a time —
the cap is 1 by architecture, not by manifest.

### Permission model: rules with per-position argv constraints

Plugins declare repeated `[[permissions.command]]` rules in
`manifest.toml`:

```toml
[[permissions.command]]
binary = "mdfind"
argv = [
    { kind = "literal", literal = "kMDItemContentType == 'com.apple.application-bundle'" },
]

[[permissions.command]]
binary = "git"
argv = [
    { kind = "literal", literal = "rev-parse" },
    { kind = "literal", literal = "HEAD" },
]
```

Each rule names a binary (PATH-resolved name or absolute path) and
a sequence of argv constraints. Constraint kinds:

- `literal` — exact byte match
- `enum` — one of N exact strings
- `glob` — shape match against a glob pattern
- `regex` — anchored regex (compiled at parse time)
- `path-under` — canonicalized path lies under a declared root,
  no symlink escape, supports not-yet-existing files via the
  shared `path_safety::canonical_under_root` helper
- `any-string` — explicit acknowledgement of free-form input
- `rest` — applies a constraint to all remaining argv positions

Variables `${plugin-data}`, `${plugin-archive}`, `${home}`,
`${xdg-config}`, `${xdg-data}` are resolved once at plugin enable
time (not per call) and used in `path-under` and `literal`
constraints.

At call time the host validates the observed `(binary, argv)`
against the rule set via the pure-function matcher in
`argv_matcher.rs`. Mismatches return `permission-denied`. No
binary, no rule, no execution — deny-by-default, identical in
spirit to existing capabilities.

### Rule overlap is rejected at manifest parse time

If any two rules accept the same `(binary, argv)`, manifest load
fails with both rule indices and an example overlap. This forces
unambiguous manifests and keeps "first-match-wins" out of the
trust model: a permission decision can never silently depend on
rule ordering.

### Opener-class binaries are not denied in command rules

`open`, `xdg-open`, `start` and similar registered-handler launchers
are *not* hard-rejected as `binary` in command rules. The extended
`opener` interface (see below) is the preferred path for "open this
with the registered application", but plugin authors may still
declare the launcher binaries as command rules if they have a
specific reason. The trust decision belongs to the plugin author and
the manifest reviewer, not to the host enforcing a paternalistic
denylist.

### Argv constraints are defense-in-depth, not soundness

Argv constraints restrict the *shape* of invocations, not the
*consequences*. A plugin granted `git` with argv constraints
inherits whatever `git` reads from `~/.gitconfig` and
`/etc/gitconfig` — including `core.pager`, `core.editor`, and
`core.fsmonitor` which can route to arbitrary executables. The
same applies to interpreters like `python3`, `node`, `osascript`,
`ruby` — argv constraints on an interpreter that loads a script
constrain only the script path, not what the script does.

This ADR accepts the limitation. Argv constraints meaningfully
defend against *unintentional* misuse of legitimate binaries
(e.g. `find -exec` smuggled into an "innocent" search) and against
*compositional* attacks where a plugin with `http::fetch` would
otherwise get to choose paths from a remote response. They do not
make a determined malicious plugin author safe; nothing short of
review/signing would. The right defense for that threat is
distribution-time, captured in the deferred install-consent
prompt (see Consequences).

### Termination semantics

Each spawn places the child in its own process group (`setpgid`
on Unix). On timeout or plugin disable, the host signals the
group: SIGTERM, 250 ms grace, SIGKILL. This catches grandchildren
(e.g. `git` spawning a pager). The bridge holds a `running_child`
slot per plugin so disable can find and kill the in-flight
process.

### Stdio is explicitly piped, not inherited

`stdout` and `stderr` are captured to `Vec<u8>` with a host-bounded
byte cap. They are *not* inherited from the host process — a
malicious plugin that spawned a binary printing attacker-controlled
bytes to inherited stdio would otherwise pollute the host's
terminal. When the byte cap is exceeded, the host kills the child
and returns the partial bytes captured so far via
`command-error::output-too-large(stdout, stderr)` so plugins still
have something to debug with.

### Environment handling

Base env = host process env minus a denylist, then plugin
overrides layered on top:

- Stripped: `LD_PRELOAD`, `LD_LIBRARY_PATH`,
  `DYLD_INSERT_LIBRARIES`, `DYLD_LIBRARY_PATH`, `SSH_AUTH_SOCK`,
  `GPG_AGENT_INFO`.
- Pattern-stripped: keys matching `*_TOKEN`, `*_KEY`,
  `*_PASSWORD`, `*_SECRET` (case-insensitive).
- `PATH` is the host's `PATH` with empty entries removed.

A plugin can re-set any stripped variable explicitly via
`command-options.env` if it genuinely needs to (`SSH_AUTH_SOCK`
for legitimate ssh-using plugins, etc.). Manifest review then
sees the explicit set rather than silent inheritance.

### Default working directory

When a call omits `cwd`, the host substitutes
`<app_data_dir>/plugin-home/<plugin-id>/exec-cwd/`, lazily
created. This is a predictable per-plugin scratch directory
distinct from the SQL state directory. Plugin can override per
call.

### Extending `opener` with `open-path` and `reveal-path`

`opener` (ADR 0037) currently exposes only `open-url` with a
scheme allowlist. This ADR extends it:

```wit
interface opener {
    variant opener-error {
        permission-denied(string),
        invalid-url(string),
        backend-failure(string),
    }
    open-url:    func(url: string)  -> result<_, opener-error>;
    open-path:   func(path: string) -> result<_, opener-error>;
    reveal-path: func(path: string) -> result<_, opener-error>;
}
```

The shared `opener-error` variant replaces the original `result<_,
string>` shape so plugins switch on a typed error value instead of
string-matching free-form messages, matching the pattern of
`http-error` and `command-error`. `invalid-url` is only emitted by
`open-url` (the path operations have no equivalent parse step).

Each new function is gated by a boolean in `[permissions.opener]`:

```toml
[permissions.opener]
schemes      = ["https"]
open-path    = true
reveal-path  = true
```

The booleans replace an originally-considered `path-roots`
allowlist. A plugin author who wanted unrestricted path access
would simply ship the malicious payload inside the plugin
archive itself and declare `path-roots = ["${plugin-archive}"]`,
making path-roots cosmetic against the malicious-author threat.
The boolean preserves the auditable signal — "this plugin can
launch registered handlers / reveal paths" — without false
precision.

### A small `platform` interface

```wit
interface platform {
    variant os   { macos, linux, windows, other(string) }
    variant arch { x86-64, aarch64, other(string) }
    os:   func() -> os;
    arch: func() -> arch;
}
```

WASM/WASI does not expose host OS or architecture. Plugins
making platform-conditional decisions (different binaries on
different OSes, different default paths) need this signal.
Independently useful and unblocks the bundled-executables work
deferred below. No permission required: this is read-only
information that doesn't widen the sandbox.

### A `paths` interface for resolving manifest variables at runtime

`[[permissions.command]]` rules use substitution variables
(`${plugin-data}`, `${plugin-archive}`, `${home}`,
`${xdg-config}`, `${xdg-data}`) in `path-under`, `literal`, and
per-rule `cwd` fields. The host resolves them at bridge
construction time. Plugins constructing argv strings that need to
satisfy a `path-under` constraint must be able to discover the
same resolved values — otherwise an `${plugin-archive}/helper`
constraint is unreachable from plugin code.

```wit
interface paths {
    variant resolve-error {
        unknown-variable(string),
        unterminated(string),
    }
    resolve: func(template: string) -> result<string, resolve-error>;
}
```

Single function, identical template syntax to the manifest. A plugin
declaring `path-under = "${plugin-archive}/repos"` in its manifest
calls `paths::resolve("${plugin-archive}/repos")` in code; both
sides walk the same recognized-variable list and produce the same
resolved path. No permission required — purely informational, no
I/O, the variables it can substitute are bounded.

### Audit logging

Every `command::run` call emits a structured `logging::log`
entry at `debug` level: plugin id, binary, full argv, exit code,
duration, stdout/stderr byte counts. Plugin authors are
responsible for argv hygiene; if a plugin pulls a secret from
`settings::get` and passes it as argv, that secret will appear
in debug logs. Debug-level keeps these out of normal operator
logs unless explicitly enabled. Plugin author responsibility
documented in the SDK docs.

## Alternatives considered

* **Generic `exec(binary, argv)` with no argv constraints** —
  rejected. The whole point of moving past Raycast's posture is
  that we *do* have a runtime chokepoint, and using it only for
  binary-level allowlisting wastes the leverage. Argv
  constraints add genuine defense-in-depth against
  unintentional misuse and compositional attacks even though
  they don't defeat a determined malicious author.

* **Per-call runtime consent prompts** — rejected for v1.
  Interrupting every `mdfind` call with a dialog destroys the
  ergonomics that justify having command exec at all. The
  trust decision is install-time, not call-time.

* **Manifest env allowlist (`env-allowed = [...]`)** — rejected
  during design discussion. There is no security distinction
  between a manifest allowlist and free per-call env overrides,
  since both are equally controlled by the plugin author.
  Either both are dangerous or neither is. Per-call overrides
  with a host-side credential denylist applied to the inherited
  base is the simpler answer with the same effective risk.

* **Hardcoded host PATH** (`/usr/bin:/bin:/usr/local/bin`)
  instead of inheriting the host process PATH — rejected.
  Inherited PATH adapts to user setups (Nix, Homebrew on Apple
  Silicon, custom prefixes); a hardcoded list breaks legitimate
  installations. The user's `$PATH` is already trusted by the
  rest of the application.

* **`max-args` cap and control-byte argv scrubbing** — rejected.
  `max-args` was DoS-framed; a plugin that wants to misbehave
  has many easier vectors. Control-byte scrubbing is moot in our
  non-shell model: Rust's `CString::new` already rejects embedded
  nulls, and other control bytes are opaque data to argv-honoring
  binaries (no shell re-tokenization step exists). Adding either
  would be theatre.

* **Interpreter-specific manifest section
  (`[[permissions.command.interpreter]]`)** — rejected. The
  regular argv vocabulary already expresses "run this script
  with this interpreter" via `binary = "python3"` and a
  `${plugin-archive}/foo.py` literal. A separate section was
  paternalism dressed as security: a manifest reviewer sees
  `binary = "python3"` regardless of which section it appears
  in. Uniform rules; no special-casing.

* **`opener` `path-roots` allowlist** — rejected. See "Extending
  `opener`" above. A malicious plugin author would declare
  `path-roots = ["${plugin-archive}"]` and ship the payload in
  the archive, so root-shape constraints don't narrow the
  malicious-author threat. Boolean capability flags preserve
  the auditable signal without false precision.

* **Hard-rejecting opener-class binaries (`open`, `xdg-open`,
  `start`, …) in `[[permissions.command]]`** — considered
  during design, rejected. The intent was to force these
  registered-handler launchers through the `opener` interface
  exclusively. In practice it would have been paternalism: the
  manifest reviewer can see `binary = "xdg-open"` in a command
  rule just as clearly as `open-path = true` under `opener`,
  and a plugin author with a specific reason to invoke the
  launcher directly should not be blocked. The `opener`
  interface remains the *preferred* path; the host does not
  enforce it.

* **Async / streaming output / persistent process handles** —
  rejected for v1. WIT resource handles for processes are
  meaningful complexity; the current call-and-wait shape
  matches `http::fetch` and the rest of the synchronous plugin
  API. Future work can add a `command::spawn` returning a
  resource handle if a real use case materializes.

* **Per-binary env quirk tables** (e.g., auto-injecting
  `GIT_CONFIG_NOSYSTEM=1` when `binary = "git"`) — rejected for
  v1. The general "config-reading binaries" escape is
  acknowledged as accepted; mechanism deferred until enough
  binaries warrant a maintained quirks table.

* **Hashed argv in audit logs at `debug`** — rejected. The
  earlier proposal was to log only `sha256(argv)` at debug to
  avoid leaking secrets a plugin might have pulled into argv.
  Discarded in favor of raw argv at debug, with the
  plugin-author argv-hygiene responsibility documented. Debug
  level is off by default in normal operator logs; readers who
  enable it accept what they see.

## Consequences

* WASM plugins can spawn system processes. The capability
  surface is meaningfully wider than v0; the `app-launcher`
  conversion path is unblocked, as are CLI-wrapping plugin
  genres broadly.
* The trust model shifts from "WIT shape alone" to "WIT shape +
  manifest-declared rule set + per-call runtime check". The
  argv matcher is the security-critical core; isolating it as a
  pure module (`argv_matcher.rs`) consuming a shared
  `path_safety` module makes it table-testable and
  independently reviewable.
* `path_safety::canonical_under_root` is extracted from day one
  with the bundled-executables todo and per-rule cwd validation
  as documented future consumers, avoiding a refactor when
  those land.
* Plugins that never declare `[[permissions.command]]` pay no
  runtime cost and no behavioral change; existing plugins are
  unaffected.
* Argv constraints are defense-in-depth, not a soundness proof.
  The ADR explicitly accepts the limitation that a determined
  malicious plugin author can ship payloads inside the archive
  or smuggle behavior through binary configuration files. The
  defense for that threat is distribution-time and is
  deferred — see deferred items below.
* Stdio capture (not inheritance) and process-group termination
  prevent the most common operational footguns: stdio leakage
  to the host terminal, and orphaned grandchildren of killed
  processes.
* Plugin authors must treat argv as potentially logged at debug.
  Secrets pulled from `settings::get` should not be passed as
  argv when the receiving binary supports stdin or env input.
  This is documented as plugin-author responsibility.
* Sets a precedent for layered runtime + manifest defense in
  future ADRs: heavier capabilities (filesystem read, network
  beyond `http::fetch`, etc.) get the same pattern — narrow WIT
  shape, manifest-declared rules with per-element constraints,
  pure host-side validator, ADR-documented threat model and
  rejected alternatives.

### Explicitly deferred

* **Bundled plugin executables** (`binary =
  "${plugin-archive}/helper"`). Tracked in
  `todos/wasm/01kq7y7k3j7p8z8ymbp0x7pga8-bundled-executables-and-platform-detection.md`.
  Requires archive-format mode preservation and (on macOS)
  quarantine-attribute handling and code-signing strategy.
* **Settings UI display of plugin permissions**. Tracked in
  `todos/wasm/01kq7x2ge7d3ykf7vxkz4fvr69-show-plugin-permissions-in-settings.md`.
* **Install-time consent prompt for User-source plugins
  declaring `[[permissions.command]]`**. Tracked in
  `todos/wasm/01kq7x2ge7d3ykf7vxkz4fvr6a-install-time-permission-consent.md`.
  Composes with future signing (ADR 0036).
* **Per-binary env quirks tables** (`GIT_CONFIG_NOSYSTEM`,
  equivalents). Mechanism deferred; limitation accepted.
* **Streaming output, async process handles, PTY**. Future
  ADR if a concrete need appears.
* **Rate limiting / per-plugin spawn quotas**. No deferred-todo
  filed; revisit if a misbehaving plugin pattern emerges.
