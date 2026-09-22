# 46. Allow executable compile-cache mappings under the hardened runtime

Date: 2026-09-22

## Status

Accepted

## Context

Notarizing a macOS app requires signing it with the hardened runtime.
The host loads every gadget's compiled code with
`Component::deserialize_file`, which maps the compile-cache file
(`gadget-home/<id>/compile-cache/*.cwasm`, ADR 0044) and makes that
mapping executable. The cache files are written at runtime and are not
code signed.

A test on Apple Silicon (2026-09-22, wasmtime 49 with Winch, all
builds ad-hoc signed) established how the hardened runtime treats this.
A standalone probe with the host's engine configuration compiled a small
module, loaded it through each path, and called it:

| Signing | `Module::new`, call | `deserialize` from bytes, call | `deserialize_file`, call |
| --- | --- | --- | --- |
| no hardened runtime | works | works | works |
| hardened runtime | killed | killed | load fails: `Permission denied (os error 13)` |
| + `allow-jit` | killed | killed | load fails |
| + `allow-unsigned-executable-memory` | works | works | load fails |
| + `disable-library-validation` | killed | killed | killed |
| + `disable-library-validation` + `allow-unsigned-executable-memory` | works | works | works |

"Killed" is `SIGKILL (Code Signature Invalid)` with termination
namespace `CODESIGNING`, "Invalid Page", on the first instruction
executed from the unsigned page.

The full app behaved the same way. Counting the `.cwasm` files mapped
executable with `vmmap`: 6 of 6 gadgets without the hardened runtime, 0
with it and any single entitlement, 6 of 6 with both entitlements, on a
cold and on a warm cache. With `disable-library-validation` alone the
app was killed at startup.

With the hardened runtime and without both entitlements, the app keeps
running but loads no gadget. `CachedComponent` logs "corrupt compile
cache, recompiling" to the gadget's devtools stream, rewrites the file,
and the second load fails too. Nothing tells the user.

## Decision

`src-tauri/Entitlements.plist` grants
`com.apple.security.cs.disable-library-validation` and
`com.apple.security.cs.allow-unsigned-executable-memory`.
`tauri.conf.json` references it through `bundle.macOS.entitlements`.
The host keeps loading gadgets with `deserialize_file`.

Tauri applies the entitlements whenever it signs the bundle and enables
the hardened runtime with them (`bundle.macOS.hardenedRuntime` defaults
to `true`). A build without a signing identity is not signed by Tauri and
stays as before (`adhoc,linker-signed`, no entitlements). An ad-hoc
signed build (`signingIdentity: "-"`) was verified: hardened runtime
flag, both entitlements, 6 of 6 gadgets mapped and running.

Rejected alternative: load the cache with `Component::deserialize` from
bytes and grant only `allow-unsigned-executable-memory`. That gives up
the file-backed mapping, which ADR 0044 measured at an idle RSS of
~52 MB against ~110 MB without it.

## Consequences

- Gadgets run in hardened-runtime builds, which notarization requires,
  and the file-backed compile cache stays.
- `disable-library-validation` also lets the process load native code
  that is not signed by Apple or by the same team.
- Notarization with these entitlements has not been tried yet. Seven
  notarized, hardened-runtime apps installed on the development machine
  carry both entitlements (OBS, BambuStudio and OBSBOT Center carry
  exactly this pair). Apple's "Resolving common notarization issues"
  names `com.apple.security.get-task-allow` as a rejection cause and
  neither of these.
- Only Apple Silicon was tested.
