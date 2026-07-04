# Compiled-component cache is deserialized with `unsafe deserialize_file` from an unauthenticated `.cwasm` (command-grant → host RCE)

**Kind:** bug (security)
**Severity:** high
**Area:** src-tauri/src/wasm/runtime/engine.rs, src-tauri/src/wasm/runtime/cached_component.rs

## Problem
`WasmRuntime::deserialize_component` loads a compiled component from a
cache file via wasmtime's `unsafe` `Component::deserialize_file`
(`engine.rs:86-91`), whose own doc comment states it "trusts the
serialized byte stream. The caller must ensure the file was written by
us." `CachedComponent::acquire` calls this on
`<app_data_dir>/gadget-home/<id>/compile-cache/<blake3(wasm)>.<engine_hash>.cwasm`
(`cached_component.rs:259-266,336-340`). The load path is simply
`if cache_path.exists() { deserialize(...) }` — whatever bytes sit at
the predictable path are trusted verbatim.

The filename is not a secret: `blake3(source_wasm) . engine_compat_hash
. cwasm`. The source WASM ships in the gadget archive and the engine
compat hash is a deterministic function of the pinned wasmtime build,
so any attacker computes the exact filename offline (or reads it from
the directory). Crucially, the blake3 hashes the **source WASM to name
the file**; the `.cwasm` artifact itself is **never hashed or
verified**.

## Impact

### Unsafety confirmed (a genuine trust boundary)
A `.cwasm` is a serialized image of compiled native machine code plus
vmcontext/relocation metadata that wasmtime mmaps and, after a
version/compatibility header check, executes with trust. wasmtime
documents deserialize as loading only trusted input with no validation
against adversarial images; the compat check the code mirrors via
`precompile_compatibility_hex` is a *compatibility* guard (don't load
honest-but-incompatible images), not an integrity/authenticity guard.
A structurally valid but malicious image can embed arbitrary native
code or corrupt metadata, yielding native code execution / memory
unsafety in the **host** process, outside the wasm sandbox. The
"corrupt cache → recompile" branch (`cached_component.rs:267-276`) only
fires on deserialize *Err*; an image crafted to deserialize
*successfully* into a hostile artifact bypasses it entirely. The
content-hash filename provides zero protection (the artifact is never
re-hashed, and blake3 is unkeyed and recomputable).

### Write surface (who can plant the `.cwasm`)
Every capability audited:
- **filesystem** — read-only; public surface is `read_file` /
  `file_exists` / `metadata` (`filesystem.rs:114,128,134`), no write
  path. Cannot write the cache.
- **sql_storage** — exposes arbitrary SQL (`execute`/`query`,
  `torchsnap-gadget.wit:98,102` → `storage/sql_storage.rs:308,328`). A
  gadget could `ATTACH DATABASE '<cache_path>'` to create a file at an
  arbitrary path, but ATTACH only produces a SQLite-format file — it
  cannot emit attacker-chosen bytes at chosen offsets, so it cannot
  synthesize a valid hostile `.cwasm` (at most a cache-poisoning DoS:
  deserialize fails → recompile). Not a code-exec planting primitive.
- **icon_cache / website_metadata / settings / frecency / clipboard /
  http / opener** — none write attacker-controlled bytes to a
  gadget-chosen path under gadget-home.
- **command — the gadget-reachable write vector.** `CommandCap::run`
  spawns a native subprocess as the user, outside the sandbox
  (`command.rs:285`), gated only by the `[[permissions.command]]`
  matcher, and the guest may pass an explicit cwd / absolute paths
  (`command.rs:177-189`). Any command grant broad enough to write a
  file (a shell, `python`, `cp`/`tee`/`dd`, or a granted helper) lets
  that subprocess write the sibling `compile-cache/` directory, and can
  `ls` it to read the exact target filename.

Realistic write vectors: (1) a gadget's own `command` subprocess able
to write a file; (2) same-user external malware; (3) any future host
arbitrary-file-write bug (install traversal, symlink, a scoped write
grant) — which this issue silently upgrades to RCE.

Directory permissions: `create_dir_all` with no explicit mode
(`cached_component.rs:300-301`) inherits umask → 0755 on default
macOS/Linux. 0755 lets other local users read; it does nothing to stop
a same-user writer, which is the actual threat.

### Severity: high (untrusted-gadget model)
Precondition: local same-user write to `compile-cache/`. Payoff:
host-process native code execution outside the wasm sandbox with the
host's full ambient authority (whole-filesystem, network, process
spawn, OS keychain), persistent (re-executes every load until pruned).

The decisive case is the **command-grant sandbox escape (vector 1)**: a
gadget holding a `command` grant that can write a file computes its own
`.cwasm` filename, plants a hostile image at the sibling path, and
triggers a reload (disable/enable or restart); on reload the host
deserializes it and runs attacker native code during instantiation.
That escalates a capability-bounded gadget to arbitrary host-native
code, defeating the wasm-sandbox + capability model that is the
system's security premise. Vector 2 (same-user malware) is real but
tempered (such an attacker already has code-exec paths); what this
issue uniquely does across all vectors is upgrade a write-only foothold
into host RCE. That upgrade plus the gadget-reachable escape warrants
high; it is not critical/remote because the precondition is a local
write a pure-WASM-cap gadget cannot reach on its own.

## Suggested fix

### Primary — authenticate the artifact with a keyed MAC before `deserialize_file`
Enforce the doc comment's "written by us" invariant cryptographically:
- On write (`first_acquire`, after producing `serialized`): compute
  `tag = blake3::keyed_hash(&mac_key, &serialized)` and persist it
  (sidecar `<name>.cwasm.mac` or a small prepended header). blake3's
  keyed mode is a purpose-built MAC/PRF and `blake3::Hash`'s `==` is
  constant-time, so no extra crate is needed — **blake3 is already a
  dependency** (`Cargo.toml:33`).
- On load (before the `unsafe` deserialize): recompute the tag over the
  on-disk bytes, constant-time compare against the stored tag; on
  mismatch treat exactly like the existing corrupt path (delete +
  recompile), and only call `deserialize_file` after the MAC verifies.
- A plain (unkeyed) hash is useless — the attacker recomputes it.
  Security rests entirely on **key secrecy**, not tag location (the tag
  may sit next to the file since the attacker cannot forge a valid tag
  without the key).

### Key-storage bootstrap (state honestly, do not hand-wave)
The precondition is a same-user writer, and a same-user attacker can
generally also *read* same-user files. Therefore:
- A **0600 key file** under a host-only dir defends only against other
  local users and against the gadget's own WASM caps (its read-only,
  allowlist-gated `filesystem` cap cannot read an arbitrary host key
  file). It does **not** defend against the two vectors that matter — a
  `command` subprocess and same-user malware both run as the user and
  can read a 0600 file, forge the MAC, and plant a validly-tagged
  hostile image. Say this explicitly so a flat key file is not mistaken
  for a fix.
- The meaningful option on **macOS is the Keychain**: generate a
  32-byte CSPRNG key on first run, store it as a generic-password item
  with an access-control ACL bound to the signed app. A spawned
  `sh`/helper is not the signed app and is gated from reading it, so
  the command-grant self-plant vector is genuinely closed on macOS.
  Residual: same-user malware against an unlocked login keychain can
  sometimes extract items — this raises the bar rather than being
  absolute. On **Linux** use the Secret Service / kernel keyring,
  falling back to a documented 0600 file (with the reduced guarantee
  stated) where no keyring exists.

### Defense-in-depth (secondary, not the fix)
- Restrict `compile-cache/` (and ideally `gadget-home/<id>/`) to
  **0700** via `create_dir_all` + `PermissionsExt::set_mode`. This only
  removes cross-*user* exposure; it does not stop the same-user
  attacker. Keep it, but do not represent it as the fix.
- Do **not** rely on "relocate the cache to a host-only root": gadget
  WASM caps already cannot write there, and the realistic writers
  (command subprocess, external malware) run as the same UID and ignore
  any app-level "host-only" designation. Relocation is a false fix.

### Strongest-security alternative — eliminate the trust boundary
Stop persisting compiled artifacts and recompile from the trusted
source WASM on each cold acquire. This removes the `unsafe` deserialize
and the poisoning surface entirely. The engine already uses Winch
(single-pass, chosen for low RSS per ADR 0043), so compile latency is
comparatively cheap; the cost is losing the file-backed/mmap'd
residency benefit the cache exists to provide. This is the only option
that removes the RCE trust boundary rather than authenticating across
it. Present as the trade: eliminate-the-unsafe (recompile) vs.
keep-the-cache-and-MAC-it.

### Recommendation
Primary = keyed-MAC verification before `deserialize_file`, key in the
OS keychain (macOS Keychain / Linux Secret Service), documented 0600
fallback with the explicit note that the flat-file fallback does not
close the same-user vector; defense-in-depth = 0700 dir perms plus the
documented same-user trust assumption. If the RSS/latency budget
tolerates it, prefer recompile-always to remove the `unsafe` outright.

## Key files
- `engine.rs:86-91` — the `unsafe` deserialize.
- `cached_component.rs:259-276` (load + corrupt-only recompile),
  `:300-301` (dir perms), `:336-340` (filename derivation).
- `caps/command.rs:171,177-189,285` — the gadget-reachable
  native-subprocess write vector.
- `caps/filesystem.rs:114,128,134` — read-only fs cap (no write path).
- `storage/sql_storage.rs:308,328` + `torchsnap-gadget.wit:98,102` —
  arbitrary SQL (ATTACH → SQLite-format only, not a hostile-`.cwasm`
  primitive).
- `Cargo.toml:33` — blake3 already present (keyed mode = MAC, no new
  dep).
