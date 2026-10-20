---
title: "Command rule binary is looked up on PATH at every spawn instead of once at load"
kind: bug
component: gadget-host
impact: high
tags: [security, capabilities]
---
# Command rule binary is looked up on PATH at every spawn instead of once at load

A `[[permissions.command]]` rule can name its binary without a path, and every spawn resolves that name through the child's `PATH`. The child's `PATH` is a fixed constant and a gadget cannot set it, but it includes the Homebrew prefixes, which the user can write without sudo. A binary placed there under a permitted name that the system directories do not have runs in place of the program the user expected. Resolving each rule's binary once at load narrows that to the moment of loading.

## Problem
A rule binary may be a bare name (`argv_matcher.rs` stores it verbatim,
matching is exact-string `rule.binary == binary`), and the spawn is
`tokio::process::Command::new(binary)` in `src-tauri/src/caps/command.rs`.
For a bare name std searches the `PATH` set on the command.

That `PATH` is `COMMAND_PATH` in `src-tauri/src/caps/command.rs`:
`/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin`. A gadget
cannot override it (`PATH` cannot be declared in `[permissions.command.env]`),
and the host's own `PATH` is not inherited. The remaining vector: on
default single-user macOS the Homebrew prefixes `/usr/local/bin` and
`/opt/homebrew/bin` are writable by the user, so any same-user process,
or a granted program whose arguments let it write files there, can place
a binary under a permitted name that the system directories do not
provide (for example `rg` or `jq`), and every later spawn runs it.

## Impact
Native code execution as the user from a grant read as a narrow one,
gated on a writer to a Homebrew prefix.

## Suggested fix
Resolve each rule's binary to an absolute path once at load
(`CommandCap::new` / `argv_matcher::compile_rule`) against
`COMMAND_PATH`, store the resolved `PathBuf` on `CompiledCommandRule`,
and spawn that path, never the guest string. Keep the guest `binary`
string only for rule selection. Fail closed: a name that does not
resolve at load fails the cap construction, like an uncompilable rule.

Do not require authors to hardcode absolute paths in the manifest: that
breaks portability between the system and the Homebrew copy of a tool
with no security gain.

Caveat to document: a TOCTOU between load-time resolution and spawn
remains, far narrower than a lookup per spawn. If Windows becomes a
target, absolute resolution is the only portable answer, since
`Command`'s PATH search has documented Windows limitations plus implicit
cwd and `.exe` semantics.

## Relations
- Relates to: [Per-rule command limits for timeout, output and stdin are parsed but never enforced](01KWG1AJRVFSMXYJMM7BVCS04B-per-rule-command-limits-for-timeout-output-and-stdin-are-parsed-but.md), per-rule constraint fields dropped at compile
