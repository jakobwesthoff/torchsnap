# 66. Bind command grants to what the install review shows

Date: 2026-10-21

## Status

Accepted

Amends [36. Plugin trust model and deferred signing](0036-plugin-trust-model-and-deferred-signing.md) (command grants no longer rest on a trusted author)

Amends [40. WASM plugin command API](0040-wasm-plugin-command-api.md) (binary names, working directory, environment)

Amends [51. Install gadgets through a review queue with staged copies](0051-install-gadgets-through-a-review-queue-with-staged-copies.md) (the review shows a rule's `cwd`)

## Context

ADR 40 lets gadgets run programs through `[[permissions.command]]`
rules that name a binary and constrain each argument. It treats the
argument constraints as defense in depth against a trusted author, not
as a boundary. A call's working directory was used as the guest passed
it, a rule's `cwd` was parsed and then ignored, and a call without a
working directory ran in `gadget-home/<id>/exec-cwd/`, a directory the
gadget's earlier calls could write to. The program inherited the host's
environment minus a denylist of variable names, including the host's
`PATH` and `HOME`; ADR 40 rejected a hardcoded `PATH` because the
inherited one adapts to user setups. ADR 36 bounds a gadget by what
its WIT imports allow. ADR 51 shows every command rule in the install
review and leaves the per-rule limits out because the host did not
enforce them.

A program reads configuration from its working directory and the
directories above it, and from `HOME`. A gadget that chooses the
working directory, or that wrote files there earlier, makes the program
run code that the rule does not show. The denylist let through
credentials whose names it did not list, such as `PGPASSWORD` or
`DATABASE_URL`.

## Decision

### Permissions are binding

What the install review shows bounds what a gadget can do. A grant read
as "may run `git log`" must not mean more. For command grants this
replaces the trusted-author reasoning of ADR 36 and ADR 40.

### Binary

A rule's `binary` is an absolute path or a bare program name. A relative
path such as `bin/tool` is rejected at parse time, since it would be
looked up in the working directory, which the guest may choose.

### Working directory

A rule's `cwd` is the root of the working directories a call may use.
Its variables resolve like those of `path-under` roots. The manifest
parser requires it to be non-empty, to start with `/` or a variable,
and to contain no `..` segment.

| Rule `cwd` | Call passes a `cwd` | Result |
|---|---|---|
| present | yes | The path must be absolute, contain no `..` component, and canonicalize under the root; otherwise the call is denied. A path inside the root that is missing or not a directory fails to spawn. |
| present | no | The root, created when it does not exist. |
| absent | yes | Denied. |
| absent | no | An empty directory created for this one call. |

The program starts in the canonical path, so it runs where the check
looked. A path with `..` is refused because a lexical check reads
`root/link/..` as `root`, while the kernel resolves it against the
link's target.

The directory for a call without a working directory is created per
call, mode 0700, below the host's temporary directory, and removed when
the call returns, after the directories inside it are made writable for
the owner. It replaces `exec-cwd/`.

### Environment

The program inherits nothing but `LANG`, `LANGUAGE`, `LC_*`, `TZ` and
`__CF_USER_TEXT_ENCODING`. Its `PATH` is fixed to
`/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin`, which
replaces the inherited `PATH` of ADR 40; the Homebrew directories are in
it because command grants exist mostly for Homebrew-installed tools,
whose own subprocess lookups need them. `HOME` and `TMPDIR` point at two
further empty directories of the call, next to its working directory.

### Install review

The review shows each command rule's `cwd` as declared, so a rule whose
`cwd` changes is a new grant. Per-rule timeout, output and stdin limits
stay out of the review while the host does not enforce them.

## Consequences

- A call that passes a working directory under a rule without `cwd` is
  denied with `permission-denied`.
- A program run without a working directory starts in an empty
  directory; nothing from earlier calls is there.
- A bare `binary` name resolves only through the fixed `PATH`; a program
  installed elsewhere needs an absolute `binary`.
- Programs that find their configuration through `HOME`, such as git,
  do not find the user's files there. Programs that look up the home
  directory in the user database, such as ssh, or use the per-user
  temporary and cache directories macOS reports without the
  environment, still reach the user's real directories.
- The per-call directories, the fixed `PATH` and the inherited
  `__CF_USER_TEXT_ENCODING` are chosen for macOS; other platforms have
  their own open decision in todo
  `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`.
