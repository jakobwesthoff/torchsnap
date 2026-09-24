---
kind: bug
severity: medium
status: open
area: [src-tauri/src/wasm/source.rs]
tags: [security]
---

# ArchiveSource pre-allocates from attacker zip metadata and never caps decompressed size (zip-bomb / OOM DoS, host boot-loop)

## Problem
`ArchiveSource::read_file` (`source.rs:446-466`) pre-allocates from
the zip entry's self-reported uncompressed size and then reads the
whole entry with no output bound:

```rust
let mut buf = Vec::with_capacity(entry.size() as usize); // attacker-controlled size
entry.read_to_end(&mut buf)...;
```

`entry.size()` is attacker-controlled: `ZipFile::size()` returns the
central-directory / local-header `uncompressed_size` (including the
ZIP64 64-bit override), and the `zip` crate validates it against
nothing at parse time. Two independent failure modes, both confirmed
against `zip` 8.5.0:

**Mode 1 — oversized `with_capacity` (needs a lying, inflated
header).** `Vec::with_capacity(entry.size() as usize)` allocates up
front, before any body byte is read. The `u64 as usize` cast is an
identity on the 64-bit target (no saving truncation). For a huge
declared size: `> isize::MAX` panics via `capacity_overflow()`
(and release does not set `panic = "abort"`, so it unwinds); merely
huge but `<= isize::MAX` (e.g. 100 GB) asks the allocator, which
aborts unconditionally via `handle_alloc_error` on refusal, or under
memory overcommit reserves virtually and then faults pages in on the
`read_to_end` until the OOM killer fires.

**Mode 2 — zip bomb (needs an honest small `compressed_size`, large
actual output).** This is the more important mode and it defeats the
declared size entirely. The crate bounds only the *input* stream
(`take(compressed_size)`); nothing bounds the *output*. For
`Deflated`, the decoder ignores the declared `uncompressed_size`
(that argument is `#[allow(unused)]` for deflate; only LZMA/legacy
codecs use it), and `read_to_end` grows the `Vec` until the deflate
stream ends. A few-KB entry can decode to gigabytes. The CRC32 check
does not help: a *truthful* bomb (real 4 GB of zeros, honest CRC,
honest `uncompressed_size`) passes it. So `read_to_end` will happily
exceed even a truthful-looking `size()` — the declared size is never
used as an output bound.

The `zip` crate offers no decompressed-size limit in 8.5.0 (no
`read::Config` option; its own `ZipFile::extract` has the identical
`Vec::with_capacity(file.size() as usize)` bug). The cap must live in
Torchsnap.

**All uncapped read sites, and when they fire:**
- `read_file` (`source.rs:446-466`) — reached by WASM asset serving
  (`protocol.rs:147`), sidebar-icon reads (`bridge.rs:193`), the
  `assets::read` host import (`bridge.rs:861`), and **SQL-migration
  reads at gadget load** (`bridge.rs:192-193`, from
  `load_single_wasm_gadget`, `lib.rs:1188`).
- `read_wasm` → `read_file` (`cached_component.rs:244-247`), fired
  lazily on first activation/instantiation.
- `ArchiveSource::open`'s `manifest.toml` `read_to_string`
  (`source.rs:423-426`) — **also uncapped**, and it runs
  unconditionally at both install (on the staged copy,
  `gadget_install/staging.rs:74`) and startup load
  (`open_gadget_source`, `lib.rs:1139-1145`).

Install now rejects archives whose file is larger than 16 MiB
(`gadget_install/staging.rs:26`). That caps the compressed size only;
a zip bomb under 16 MiB still expands without limit on these reads.

## Impact
Denial of service only — no memory disclosure, no code execution —
gated on the user installing a malicious `.torchsnap`, which is the
plugin system's expected threat surface. That keeps it out of high.

It is above low because the trigger is reliable and zero-effort and
hits at the worst time: the `open()` manifest read and the migration
reads fire during **startup gadget load**, not on some obscure later
asset request. A crafted installed gadget can therefore crash the
host **on launch**, and because installed user gadgets are
auto-loaded every start, the crash **persists across restarts** until
the archive is manually removed from `<app_data_dir>/gadgets/` — the
app cannot be used to uninstall it because it dies before the UI
comes up. That boot-loop property is why this rates **medium** rather
than low. (The fact that it also fires on `read_wasm` at activation
raises the practical rating within DoS: from "OOM when you open one
of its screens" to "prevents the host from starting at all.")

## Suggested fix
The runtime output cap is the mandatory, sufficient control;
everything else is defense-in-depth / fail-fast ergonomics.

**(a) Bound the decompressed read (fixes mode 2 — the real defense).**
Replace the unbounded read with a `take`-limited read plus a
sentinel:

```rust
let mut buf = Vec::with_capacity(std::cmp::min(entry.size(), CAP) as usize);
entry.take(CAP + 1).read_to_end(&mut buf)   // io::Take caps peak memory at CAP+1
    .with_context(|| format!("decompressing gadget file `{path}`"))?;
anyhow::ensure!(
    buf.len() as u64 <= CAP,
    "gadget file `{path}` exceeds the {CAP}-byte decompressed size limit"
);
```

`ZipFile: Read` and we own `entry`, so `.take(n)` (by value into
`io::Take<ZipFile>`) is fine. `CAP + 1` lets exactly-`CAP` pass while
making `> CAP` detectable. Peak memory is bounded to `CAP+1`
regardless of the declared size or deflate ratio.

**(b) Fix the pre-allocation (fixes mode 1).** The
`min(entry.size(), CAP)` clamp above already caps the initial
allocation at `CAP`, so a lying inflated header can no longer trigger
the huge alloc. `try_reserve` is optional gold-plating (with the
clamp, the pre-alloc is at most tens of MB; if that fails the host is
already out of memory). A tiny fixed initial capacity + growth would
remove all trust in `size()` at the cost of some reallocs; the clamp
is preferable because it keeps the single-allocation fast path for
honest files. Either is correct.

**(c) `manifest.toml` read — same treatment, tighter cap:**

```rust
entry.take(MAX_MANIFEST_BYTES + 1).read_to_string(&mut toml_source)...;
anyhow::ensure!(toml_source.len() as u64 <= MAX_MANIFEST_BYTES, "...");
```

`read_to_string` already rejects non-UTF-8, so no behaviour change
for legit manifests.

**(d) Do NOT rely on header / ratio validation at `open()` as the
primary control.** Validating `entry.size()` against a cap catches
mode 1 only; a zip bomb declares an honest size and defeats it. A
compression-ratio heuristic is likewise unreliable (the declared
size can be honest-huge or lying-small). Header validation is an
acceptable cheap early-out for mode 1 but must not replace the
runtime `take` cap. Recommendation: skip the ratio heuristics, keep
the hard runtime cap.

**(e) Install-time fail-fast (optional hardening).**
Staging already copies the archive and opens the copy
(`gadget_install/staging.rs:64-74`); adding a capped `read_wasm()`
there rejects an
oversized/bomb WASM at install with a clear error rather than at
first activation. Low cost; optional given the runtime cap.

**(f) Where the caps live / distinct WASM cap.** `read_wasm` is a
trait default delegating to `read_file`, but WASM binaries warrant a
larger cap than generic assets. Add a private
`ArchiveSource::read_entry_capped(path, cap)`, have `read_file` call
it with the asset cap, and **override `read_wasm` on `ArchiveSource`**
to call it with the WASM cap. Define the caps as named module consts.

**Concrete cap values** (grounded in measured artifacts: real gadget
WASM ranges 152 KB–1.6 MB, bundled archives 172–540 KB):

| Read | Const | Cap | Rationale |
|------|-------|-----|-----------|
| WASM binary (`read_wasm`) | `MAX_WASM_BYTES` | 64 MiB | ~40× the largest current gadget; room for future multi-MB components while bounding a single alloc. |
| Generic assets (`read_file`: bundles, images, migrations, data) | `MAX_ASSET_BYTES` | 32 MiB | Biggest legit assets sit under 1 MB today; generous headroom. |
| `manifest.toml` | `MAX_MANIFEST_BYTES` | 1 MiB | Manifests are hundreds of bytes to low KB. |

SQL migrations flow through `read_file`, so 32 MiB covers them; a
dedicated tighter migration cap (e.g. 4 MiB) is reasonable but not
necessary. The chosen values are 20–40× above current maxima, so no
realistic gadget breaks; raise the asset const if future gadgets
bundle large media. The `take`-cap adds no measurable cost to honest
reads (the decoder stops naturally well before `CAP+1`).

## Caveats
- Header validation alone is insufficient (mode 1 only); ship the
  runtime `take` cap regardless — it closes the zip-bomb vector.
- The `zip` crate offers no decompression limit in 8.5.0 (its own
  `extract` has the identical bug); do not wait for an upstream fix.
- CRC does not protect you — a truthful bomb passes it.
- `DirectorySource` is a secondary vector: its reads use
  `std::fs::read`, pre-allocating from real file metadata rather than
  an attacker's lie, and its sources are `Dev` (repo-local,
  debug-only) / `Builtin` — trusted. Capping it for symmetry is
  defensible but lower priority than `ArchiveSource`, where untrusted
  input enters.
- 32-bit hosts would truncate `entry.size() as usize` and
  accidentally soften mode 1, but that is not the shipping target;
  the fix makes it moot. `file_exists` is unaffected (no body read).

## Tests to accompany the fix
- Zip-bomb read: a Deflated entry decompressing past the cap (zeros
  compress ~1000:1, so a small fixture works); assert `read_file` /
  `read_wasm` return `Err`, not OOM. Make the cap overridable in
  `cfg(test)` (or expose the const) so the fixture stays small.
- Oversized manifest: `manifest.toml` exceeding `MAX_MANIFEST_BYTES`;
  assert `open()` returns `Err`.
- Within-cap regression: a legit ~1–2 MB WASM and normal assets still
  read byte-for-byte (existing archive tests keep passing).
- Note: forging a lying inflated `uncompressed_size` (pure mode 1) is
  awkward with `ZipWriter` (it writes truthful sizes); cover the
  clamp indirectly by asserting peak allocation stays bounded, or by
  hand-patching the central directory for an explicit mode-1 case.

## Key files
`source.rs:406-466` (both bugs), `cached_component.rs:244-247`
(`read_wasm` at activation), `bridge.rs:188-199` (migration reads at
load), `lib.rs:1139-1147,1188` (startup load path),
`gadget_install/staging.rs:64-74` (install path), `protocol.rs:147` (asset
serving).
