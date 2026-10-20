---
title: "Authenticate the compiled-component cache before unsafe deserialization"
kind: bug
component: gadget-host
impact: high
tags: [security]
---
# Authenticate the compiled-component cache before unsafe deserialization

The compiled-component cache is loaded with wasmtime's `unsafe` `deserialize_file`, and its file contents are never checked. The file name is predictable, so a gadget holding a `command` grant can plant a crafted `.cwasm` and get native code running in the host. The fix is a keyed MAC over the cache file before deserialization, or dropping the cache so the component is always recompiled.

## Problem
`WasmRuntime::deserialize_component` loads a compiled component from a
cache file via wasmtime's `unsafe` `Component::deserialize_file`
(`WasmRuntime::deserialize_component` in `engine.rs`), whose own doc comment states it "trusts the
serialized byte stream. The caller must ensure the file was written by
us." `CachedComponent::acquire` calls this on
`<app_data_dir>/gadget-home/<id>/compile-cache/<blake3(wasm)>.<engine_hash>.cwasm`
(`CachedComponent::first_acquire` and `cache_filename` in
`cached_component.rs`). The load path is simply
`if cache_path.exists() { deserialize(...) }`. Whatever bytes sit at
the predictable path are trusted verbatim.

The filename is not a secret: `blake3(source_wasm) . engine_compat_hash
. cwasm`. The source WASM ships in the gadget archive, and the engine
compat hash is a deterministic function of the pinned wasmtime build,
so any attacker computes the exact filename offline or reads it from
the directory. Crucially, the blake3 hashes the **source WASM to name
the file**. The `.cwasm` artifact itself is **never hashed or
verified**.

## Impact

### Unsafety confirmed (a genuine trust boundary)
A `.cwasm` is a serialized image of compiled native machine code plus
vmcontext and relocation metadata. wasmtime mmaps it and, after a
version and compatibility header check, executes it with trust. wasmtime
documents deserialize as loading only trusted input with no validation
against adversarial images. The compat check the code mirrors via
`precompile_compatibility_hex` is a *compatibility* guard (don't load
honest-but-incompatible images), not an integrity or authenticity guard.
A structurally valid but malicious image can embed arbitrary native
code or corrupt metadata, yielding native code execution or memory
unsafety in the **host** process, outside the wasm sandbox. The
"corrupt cache, recompile" branch in `first_acquire` only
fires on deserialize *Err*. An image crafted to deserialize
*successfully* into a hostile artifact bypasses it entirely. The
content-hash filename provides zero protection, because the artifact is
never re-hashed and blake3 is unkeyed and recomputable.

### Write surface (who can plant the `.cwasm`)
Every capability audited:
- **filesystem**: read-only. Its public surface is `read_file` /
  `file_exists` / `metadata` (`FilesystemCap` in `filesystem.rs`), with no write
  path. It cannot write the cache.
- **sql_storage** exposes arbitrary SQL (`execute`/`query` in the
  `sql-storage` interface of `torchsnap-gadget.wit`, implemented in
  `storage/sql_storage.rs`). It used to allow `ATTACH DATABASE` to create a
  file at an arbitrary path. ATTACH, DETACH and VACUUM are now denied and
  defensive mode is on (ADR 0065), so SQL no longer writes outside the
  gadget's own database file. It was never a code-exec planting primitive,
  since ATTACH only produces SQLite-format files.
- **icon_cache / website_metadata / settings / frecency / clipboard /
  http / opener**: none write attacker-controlled bytes to a
  gadget-chosen path under gadget-home.
- **command** is the gadget-reachable write vector. `CommandCap::run`
  spawns a native subprocess as the user, outside the sandbox
  (`run_child_with_caps` in `caps/command.rs`), gated only by the
  `[[permissions.command]]` matcher. Since ADR 0066 the host keeps the
  working directory and path arguments out of the app data directory
  (`WritableAreas` in `caps/command.rs`), so a call cannot name
  `compile-cache/` itself. That does not stop a granted program that
  writes files on its own terms. Any command grant broad enough to write
  a file (a shell, `python`, `cp`, `tee`, `dd`, or a granted helper) can
  still plant a file in the sibling `compile-cache/` directory, and can
  `ls` it to read the exact target filename.

Realistic write vectors: (1) a gadget's own `command` subprocess able
to write a file; (2) same-user external malware; (3) any future host
arbitrary-file-write bug (install traversal, symlink, a scoped write
grant), which this issue silently upgrades to RCE.

Directory permissions: `create_dir_all` with no explicit mode
(`CachedComponent::first_acquire`) inherits the umask, which gives 0755 on
default macOS and Linux. 0755 lets other local users read. It does
nothing to stop a same-user writer, which is the actual threat.

### Severity: high (untrusted-gadget model)
Precondition: a local same-user write to `compile-cache/`. Payoff:
host-process native code execution outside the wasm sandbox, with the
host's full ambient authority (whole filesystem, network, process
spawn, OS keychain). The payoff is persistent, because it re-executes on
every load until pruned.

The decisive case is the **command-grant sandbox escape (vector 1)**. A
gadget holding a `command` grant that can write a file computes its own
`.cwasm` filename, plants a hostile image at the sibling path, and
triggers a reload (disable and enable, or restart). On reload the host
deserializes it and runs attacker native code during instantiation.
That escalates a capability-bounded gadget to arbitrary host-native
code, defeating the wasm-sandbox and capability model that is the
system's security premise. Vector 2 (same-user malware) is real but
tempered, since such an attacker already has code-exec paths. What this
issue uniquely does across all vectors is upgrade a write-only foothold
into host RCE. That upgrade, plus the gadget-reachable escape, warrants
high. It is not critical or remote, because the precondition is a local
write that a pure-WASM-cap gadget cannot reach on its own.

## Suggested fix

### Primary: authenticate the artifact with a keyed MAC before `deserialize_file`
Enforce the doc comment's "written by us" invariant cryptographically:
- On write (`first_acquire`, after producing `serialized`): compute
  `tag = blake3::keyed_hash(&mac_key, &serialized)` and persist it
  (sidecar `<name>.cwasm.mac` or a small prepended header). blake3's
  keyed mode is a purpose-built MAC and PRF, and `blake3::Hash`'s `==` is
  constant-time, so no extra crate is needed. **blake3 is already a
  dependency** (`Cargo.toml`).
- On load (before the `unsafe` deserialize): recompute the tag over the
  on-disk bytes and compare it in constant time against the stored tag. On
  mismatch, treat it exactly like the existing corrupt path (delete and
  recompile). Call `deserialize_file` only after the MAC verifies.
- A plain (unkeyed) hash is useless, because the attacker recomputes it.
  Security rests entirely on **key secrecy**, not on where the tag lives.
  The tag may sit next to the file, since the attacker cannot forge a
  valid tag without the key.

### Key-storage bootstrap (state honestly, do not hand-wave)
The precondition is a same-user writer, and a same-user attacker can
generally also *read* same-user files. Therefore:
- A **0600 key file** under a host-only dir defends only against other
  local users and against the gadget's own WASM caps. Its read-only,
  allowlist-gated `filesystem` cap cannot read an arbitrary host key
  file. It does **not** defend against the two vectors that matter. A
  `command` subprocess and same-user malware both run as the user and
  can read a 0600 file, forge the MAC, and plant a validly tagged
  hostile image. A flat key file must not be mistaken for a fix.
- The meaningful option on **macOS is the Keychain**: generate a
  32-byte CSPRNG key on first run, and store it as a generic-password item
  with an access-control ACL bound to the signed app. A spawned
  `sh` or helper is not the signed app and is gated from reading it, so
  the command-grant self-plant vector is genuinely closed on macOS.
  Residual risk: same-user malware against an unlocked login keychain can
  sometimes extract items. This raises the bar rather than being
  absolute. On **Linux**, use the Secret Service or kernel keyring,
  falling back to a documented 0600 file (with the reduced guarantee
  stated) where no keyring exists.

### Defense-in-depth (secondary, not the fix)
- Restrict `compile-cache/` (and ideally `gadget-home/<id>/`) to
  **0700** via `create_dir_all` and `PermissionsExt::set_mode`. This only
  removes cross-*user* exposure. It does not stop the same-user
  attacker. Keep it, but do not represent it as the fix.
- Do **not** rely on "relocate the cache to a host-only root". Gadget
  WASM caps already cannot write there, and the realistic writers
  (command subprocess, external malware) run as the same UID and ignore
  any app-level "host-only" designation. Relocation is a false fix.

### Strongest-security alternative: eliminate the trust boundary
Stop persisting compiled artifacts and recompile from the trusted
source WASM on each cold acquire. This removes the `unsafe` deserialize
and the poisoning surface entirely. The engine already uses Winch
(single-pass, chosen for low RSS per ADR 0043), so compile latency is
comparatively cheap. The cost is losing the file-backed, mmap'd
residency benefit the cache exists to provide. This is the only option
that removes the RCE trust boundary rather than authenticating across
it. It is presented as the trade: eliminate the unsafe (recompile) versus
keep the cache and MAC it.

### Recommendation
Primary: keyed-MAC verification before `deserialize_file`, with the key in
the OS keychain (macOS Keychain, Linux Secret Service) and a documented 0600
fallback. The flat-file fallback does not close the same-user vector, and
that is stated explicitly. Defense in depth: 0700 directory permissions plus
the documented same-user trust assumption. If the RSS and latency budget
tolerates it, prefer recompile-always, which removes the `unsafe` outright.

## Key files
- `engine.rs`, `WasmRuntime::deserialize_component`: the `unsafe` deserialize.
- `cached_component.rs`, `CachedComponent::first_acquire` (load,
  corrupt-only recompile, directory creation) and `cache_filename`
  (filename derivation).
- `caps/command.rs`, `run_child_with_caps` and `WritableAreas`: the
  gadget-reachable native-subprocess write vector.
- `caps/filesystem.rs`: read-only fs cap (no write path).
- `storage/sql_storage.rs` and the `sql-storage` interface in
  `torchsnap-gadget.wit`: arbitrary SQL, confined to the gadget's own
  database file by ADR 0065.
- `Cargo.toml`: blake3 already present (keyed mode is a MAC, no new
  dependency).
