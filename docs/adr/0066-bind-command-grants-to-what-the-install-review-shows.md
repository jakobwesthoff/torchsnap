# 66. Bind command grants to what the install review shows

Date: 2026-10-21

## Status

Accepted

Amends [36. Plugin trust model and deferred signing](0036-plugin-trust-model-and-deferred-signing.md) (command grants no longer rest on a trusted author)

Amends [40. WASM plugin command API](0040-wasm-plugin-command-api.md) (binary names, working directory)

Amends [51. Install gadgets through a review queue with staged copies](0051-install-gadgets-through-a-review-queue-with-staged-copies.md) (the review shows a rule's `cwd`)

## Context

ADR 40 lets gadgets run programs through `[[permissions.command]]`
rules that name a binary and constrain each argument. It treats the
argument constraints as defense in depth against a trusted author, not
as a boundary. A call's working directory was used as the guest passed
it, a rule's `cwd` was parsed and then ignored, and a call without a
working directory ran in `gadget-home/<id>/exec-cwd/`, a directory the
gadget's earlier calls could write to. ADR 36 bounds a gadget by what
its WIT imports allow. ADR 51 shows every command rule in the install
review and leaves the per-rule limits out because the host did not
enforce them.

A program reads configuration from its working directory and the
directories above it. A gadget that chooses the working directory, or
that wrote files there earlier, makes the program run code that the
rule does not show.

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

### Install review

The review shows each command rule's `cwd` as declared, so a rule whose
`cwd` changes is a new grant. Per-rule timeout, output and stdin limits
stay out of the review while the host does not enforce them.

## Consequences

- A call that passes a working directory under a rule without `cwd` is
  denied with `permission-denied`.
- A program run without a working directory starts in an empty
  directory; nothing from earlier calls is there.
- The location and permissions of the per-call directory are chosen for
  macOS; other platforms have their own open decision in todo
  `01M5DN71VBN0H0AVSQT2GMDK49-decide-how-command-confinement-works-on-linux-and-windows.md`.
