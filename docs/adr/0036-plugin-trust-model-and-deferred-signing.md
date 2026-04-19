# 36. Plugin trust model and deferred signing

Date: 2026-04-17

## Status

Accepted

## Context

ADR 0035 ships a user-installable plugin flow: users drop a
`.torchsnap` downloaded from the internet onto the Plugins
settings panel and it loads on the next restart. That immediately
raises a trust question: what prevents a malicious `.torchsnap`
from doing damage?

The surface area worth reasoning about, concretely:

- A plugin's WASM component runs inside wasmtime with host-provided
  imports defined in `plugins/plugin-sdk/wit/torchsnap-plugin.wit`. Available imports
  today: `logging`, `clipboard` (write-only), `sql`, `settings`,
  `messaging`. **There is no `network` interface.** Guest code
  cannot open sockets, make HTTP requests, or read arbitrary files
  outside what the host chooses to expose.
- The manifest (`manifest.toml`) references files within the
  plugin source — the WASM binary, the frontend bundle, icons, SQL
  migrations. At load time the host calls
  `PluginSource::read_file(path)` on those references. A
  maliciously crafted path (`../../.ssh/id_rsa`) would otherwise
  get the host to read files outside the plugin root and hand them
  to the webview as "the plugin's frontend bundle" — escalating
  past the WIT sandbox entirely.
- `.torchsnap` files are plain zip archives with no signature.
  Nothing today proves who built a given archive or that it has
  not been tampered with in transit.

The question this ADR settles: given those constraints, **what is
the minimum we ship in v1 and what do we explicitly defer?**

## Decision

### The WIT capability surface is the primary sandbox

Guest code's reach is exactly what the WIT imports allow. Today
that means no network egress, no arbitrary filesystem reads, no
process spawning. Adding any of those in the future requires a new
WIT interface — which is an ADR-worthy decision each time, not an
accidental widening.

The clipboard interface is write-only on purpose: no read-clipboard
import exists, so a plugin cannot silently scrape what the user
has copied.

### Path traversal is blocked at the manifest parser

A centralized `validate_plugin_path` helper runs on every
manifest-referenced path (`plugin.wasm`, asset-form icon,
`frontend.launcher_bundle`/`settings_bundle`/`launcher_css`/`settings_css`,
`storage.sql.migrations[*]`) at parse time, and again at the
`DirectorySource::read_file` / `ArchiveSource::read_file`
boundaries as defense in depth. The guard rejects:

- Empty strings, NUL bytes.
- Backslashes (paths are forward-slash by policy, matching zip
  semantics, regardless of host OS).
- Absolute POSIX paths (leading `/`).
- Windows-style absolute paths (`C:\…`, `\\…`) — rejected on all
  platforms so a Windows-authored malicious plugin still fails on
  macOS.
- Running depth below zero after lexical normalization of `..` and
  `.` segments — catches `../foo`, `a/../../b`, and deeper
  variants.

Manifests that fail the guard do not parse, so the plugin never
loads. The tests pin each reject case; they are not optional.

### Install-time validation

Install opens the source via `ArchiveSource::open`, which
implicitly runs the manifest parser. A corrupt zip, a zip with no
manifest, a malformed manifest, or a manifest that violates the
path guard all reject the install before any file is copied into
`<app_data_dir>/plugins/`.

### ID-collision rejection

A crafted `.torchsnap` claiming to be a system plugin id is
rejected at install with a kind-specific error. This does not rely
on signature checking — the host already knows which ids are
Builtin / System / Dev and the install flow refuses anything that
would shadow them.

### Signing is deferred

**No signature scheme ships in v1.** Distribution today is
person-to-person: users download an archive, drop it into the
panel, it installs. The trust model for that flow is "whoever you
got the file from, you trust them" — the same model as installing
a `.dmg` or `.exe` downloaded from the internet.

The decision to defer is active, not an oversight. Reasons:

- The WIT sandbox already bounds what any plugin — trusted or
  not — can do. The damage a malicious plugin can inflict is
  limited to the host-provided capabilities (no network egress
  means no exfiltration vector; no arbitrary fs means no data
  theft outside the plugin's own state tree).
- Signing is only useful once there is a distribution channel to
  tie an identity to. A plugin marketplace or update mechanism
  would want signing; ad-hoc hand-sharing does not.
- A premature signing scheme becomes a maintenance burden and is
  usually wrong by the time it matters — better to wait until the
  real threat model is visible.

### When to revisit

Revisit this ADR when any of the following becomes true:

- A `network` (or filesystem-read) WIT import is added, widening
  the attack surface beyond what the current model bounds.
- A distribution channel beyond direct download appears (plugin
  catalog, auto-update, marketplace).
- An actual exploit emerges in the wild against this model.

## Consequences

### What this enables

- v1 ships without the engineering cost of a signing scheme.
- The sandbox is expressible in one sentence: "what WIT lets them
  do." That is audit-friendly and gives plugin authors a clear
  mental model of what is and isn't in scope for their code.
- The path guard is load-bearing, and the tests treat it that way:
  regressions surface immediately.

### What this costs

- Supply-chain risk is entirely on the user: they must trust the
  source they downloaded a `.torchsnap` from. No host-level proof
  of authorship or integrity.
- A tampered archive that passes the manifest parser and WIT
  imports is indistinguishable from a legitimate one. The blast
  radius is bounded by the sandbox, but "bounded" is not "zero."
- The decision to add a `network` interface in the future is now
  explicitly a trust-model revisit trigger, not just a feature
  request.

### Non-consequences (for clarity)

- This is not a statement that plugins are "safe to run from
  strangers." It is a statement that the sandbox limits damage,
  not that damage is impossible.
- The WIT sandbox does not protect against denial-of-service
  (infinite loops, memory exhaustion inside the guest). Those are
  runtime resource concerns, outside this ADR's scope.
