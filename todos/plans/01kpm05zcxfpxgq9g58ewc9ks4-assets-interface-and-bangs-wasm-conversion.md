# Plan: `assets` host WIT interface and `bangs` WASM conversion

## Context

Two sequential PRs land together:

- **PR 1** introduces the `assets` host WIT interface so WASM gadgets can read files bundled inside their own `.torchsnap` archive (or development directory). No permission declaration — gadgets can always read their own assets, security is guaranteed by `validate_gadget_path` rejecting anything that escapes the gadget root. Mirrors the opener/http wiring landed in commit `a20c223`.
- **PR 2** ports the native `bangs` gadget to WASM, using the new `assets` interface (to load the bundled `bang.json` at enable-time) plus the already-landed `http` and `opener` interfaces. Deletes the native gadget, the native frontend settings component, and the `src-tauri/derived/bang.json` include.

Per-step discipline: each lettered step ends with `cargo build -p torchsnap-tauri` (and the relevant tests) and an atomic commit that names the step. Commit messages use the project's 3-step workflow (`.tmp-commit-msg` → `git commit -F` → `rm .tmp-commit-msg`).

**MPL header reminder (PR 1 & PR 2):** Every new `.rs`, `.ts`, `.tsx`, `.css`, `.html`, `.sh`, `.just` file gets the 3-line MPL 2.0 header at the top. The `.md` plan file itself lives under `todos/plans/` and does not need one.

---

## PR 1 — `assets` host interface

### Step 1a — Bridge owns its `GadgetSource`

**File:** `src-tauri/src/wasm/bridge.rs`

The bridge currently takes `&dyn GadgetSource` in `new()` (line 133), uses it locally for `read_wasm()` + migration-file reads, and drops it. `assets` needs the source handle to survive past construction so `enable()` can stash it on `GadgetState`. This is a prerequisite — the rest of PR 1 won't compile without it.

Changes:

1. Add a field to `WasmGadgetBridge` (next to `sql_config` at line 56):
   ```rust
   /// Retained so `enable()` can stash it on `GadgetState` for the
   /// `assets::read` / `assets::exists` host imports. Same shared
   /// reference the `wasm::protocol` module holds for frontend
   /// asset serving — both callers use `Arc::clone`.
   gadget_source: Arc<dyn GadgetSource + Send + Sync>,
   ```

2. Change the `new()` signature (line 129) to accept `source: Arc<dyn GadgetSource + Send + Sync>` instead of `source: &dyn GadgetSource`. The trait is already `Send + Sync` (see `source.rs:77`).

3. In `new()`, replace `source.read_wasm()` / `source.read_file(path)` with `source.read_wasm()` / `source.read_file(path)` calls on the Arc (no change — `Arc<dyn Trait>` auto-derefs), then at the end of the `Ok(Self { ... })` block add `gadget_source: source,`.

4. Update the single production call site — `load_single_wasm_gadget` at `src-tauri/src/lib.rs:1034`. The parameter `source: Arc<dyn wasm::source::GadgetSource>` is at line 1036; the `WasmGadgetBridge::new(..., source.as_ref(), ...)` call is at line 1045 inside the body. Change the parameter type to `Arc<dyn wasm::source::GadgetSource + Send + Sync>` and pass `Arc::clone(&source)` to `WasmGadgetBridge::new` (keep the existing `source_registry.insert(..., source)` line — the registry gets the Arc afterward). **Verify during implementation:** the `Arc<dyn GadgetSource>` at `lib.rs:1036` needs `+ Send + Sync` at its call site too (discovery layer builds the Arc); trace and update as needed.

5. Update every test call site inside `bridge.rs` (lines 812, 854, 897, 1037, 1114, 1236). The pattern inside each test is currently `WasmGadgetBridge::new(manifest, runtime, log_sender, &source, app_data_dir)` — change to `Arc::new(source)` wrapped equivalently. `DirectorySource` implements `GadgetSource + Send + Sync` by construction, so `Arc::new(source) as Arc<dyn GadgetSource + Send + Sync>` works.

After this step: `cargo test -p torchsnap-tauri wasm::bridge` — every existing test still passes, no behavior change yet. Commit: `wasm/bridge: retain GadgetSource as Arc for capability imports`.

---

### Step 1b — WIT: add the `assets` interface

**File:** `gadgets/gadget-sdk/wit/torchsnap-gadget.wit`

The file's interface ordering is logical-by-capability, not alphabetical (logging/clipboard/sql/frecency/settings/opener/http — see lines 8, 50, 78, 126, 163, 186, 207). Insert `assets` right after `http` (line 256) so capability-style imports stay grouped, before the `/// ─── Guest exports` divider at line 258.

Content to insert (matches the style of `http-error`):

```wit
/// Per-gadget asset access.
///
/// Every WASM gadget can read files from inside its own `.torchsnap`
/// archive (or development directory) via this interface. Gadgets do
/// NOT opt in — there is no `[permissions.assets]` section. The
/// guarantee is spatial: paths are validated by the same
/// `validate_gadget_path` guard that governs every other gadget-file
/// read on the host side, so the interface can only reach files
/// already inside the gadget root.
///
/// Bytes cross the WIT boundary in full on every `read` call — the
/// host holds no cached handle. Gadgets that need to retain an asset
/// should keep one copy in their own linear memory after reading.
///
/// Typical use: load a bundled JSON database at `enable()` time.
/// ```
/// let bytes = assets::read("data/bangs.json")?;
/// ```
interface assets {
  /// Error variants for `read` and `exists`. The shape mirrors
  /// `http-error`: one variant per semantic category, no catch-all.
  variant assets-error {
    /// The path is outside the gadget root, or syntactically invalid
    /// (absolute, backslash-separated, contains `..` traversal, NUL
    /// byte, Windows drive letter, or empty). Message includes the
    /// offending path.
    invalid-path(string),
    /// The path validates but no such file exists inside the gadget.
    not-found,
    /// The host's filesystem (or archive) layer reported an error
    /// while reading. Permission problems, corrupt archives, etc.
    io-error(string),
  }

  /// Read a file inside the gadget. `path` is relative to the gadget
  /// root, same convention as `manifest.toml` path fields.
  read: func(path: string) -> result<list<u8>, assets-error>;

  /// Check whether a file exists inside the gadget without reading
  /// its bytes. Useful for optional assets (e.g. a bundled fallback
  /// that may or may not ship depending on the build).
  exists: func(path: string) -> result<bool, assets-error>;
}
```

Then in `world gadget` (lines 433–446), **reorder the existing imports to match interface-definition order** (logging/clipboard/sql/frecency/settings/opener/http) and append the new `import assets;` at the end — so the world import order matches the order the interfaces are defined in the file. The current world import order (logging/settings/sql/clipboard/frecency/opener/http) diverges from definition order; this step fixes that inconsistency while adding the new import. The resulting block:

```wit
world gadget {
  import logging;
  import clipboard;
  import sql;
  import frecency;
  import settings;
  import opener;
  import http;
  import assets;

  export lifecycle;
  export search;
  export messaging;
  export tasks;
}
```

`bindings.rs` regenerates on next `cargo build` — no manual edit required.

After this step: `just check-wit && cargo build -p torchsnap-tauri`. The build fails because no `assets::Host` impl exists yet — that's expected; the commit still stages the WIT change so the subsequent steps have regenerated bindings to implement against. Commit: `wit: add assets interface for gadget self-file access`.

(Optional: if `cargo build` failing-for-expected-reasons is undesirable inside an atomic commit, fold this step into 1d. The opener/http plan took the same approach — WIT lands first, then the Rust impl in the next commit — and relies on reviewer familiarity with the pattern.)

---

### Step 1c — `GadgetSource::file_exists` trait method

**File:** `src-tauri/src/wasm/source.rs`

Add a new trait method on `GadgetSource` (around line 85, after `read_file`):

```rust
/// Check whether a file exists at `path` without reading its
/// bytes. Used by the `assets::exists` host import so gadgets
/// can probe optional assets cheaply.
///
/// `path` is validated the same way as `read_file` — the default
/// implementation is NOT provided because both backing stores
/// need to answer without incurring the full read.
fn file_exists(&self, path: &str) -> anyhow::Result<bool>;
```

Implementations:

- **`DirectorySource::file_exists` (add after `read_file` at line 177):** Runs `validate_gadget_path(path)?` first to reject traversal, then joins `self.root` with the normalized path, canonicalizes the root to guard against symlink escape (same pattern as `read_file` lines 148–168), and returns `Ok(canonical.starts_with(&canonical_root) && canonical.is_file())`. If `canonicalize` fails because the file simply doesn't exist, fall through to `normalize_path(...).starts_with(&canonical_root)` and return `Ok(false)` — same failure mode as `read_file`.

- **`ArchiveSource::file_exists` (add after `read_file` at line 398):**
  ```rust
  fn file_exists(&self, path: &str) -> anyhow::Result<bool> {
      let normalized = validate_gadget_path(path)?;
      let normalized_str = normalized.to_string_lossy();
      let mut archive = self.archive.lock().expect("archive mutex not poisoned");
      match archive.by_name(&normalized_str) {
          Ok(_) => Ok(true),
          Err(zip::result::ZipError::FileNotFound) => Ok(false),
          Err(e) => Err(anyhow::anyhow!("archive lookup for `{path}` failed: {e}")),
      }
  }
  ```
  **Verify during implementation:** the exact `zip::result::ZipError::FileNotFound` variant name — the `zip` crate major version pinned in `Cargo.toml` determines the module path. Alternate: `zip::ZipError::FileNotFound`. Check the crate's re-exports in `target/doc/zip/`.

Unit tests to add in the existing `tests` module (around line 600 and 850):

- `file_exists_returns_true_for_existing_directory_file` (uses `make_gadget_dir`)
- `file_exists_returns_false_for_missing_directory_file`
- `file_exists_rejects_traversal_path` — error contains `escapes`, same contract as `read_file`
- `file_exists_rejects_absolute_path` — error contains `relative`
- `archive_file_exists_returns_true_for_existing_entry`
- `archive_file_exists_returns_false_for_missing_entry` — crucial: this documents the `FileNotFound` → `Ok(false)` branch so a zip-crate upgrade that changes the error enum surfaces here first

After this step: `cargo test -p torchsnap-tauri wasm::source`. Commit: `wasm/source: add GadgetSource::file_exists for cheap asset probes`.

---

### Step 1d — Runtime: `GadgetState` field, setters, and `assets::Host` impl

**File:** `src-tauri/src/wasm/runtime.rs`

Add the field to `GadgetState` (insert after `http_client` at line 131, following the field comment style used for `clipboard_writer` / `http_client`):

```rust
/// The gadget's own source handle, stashed by the bridge on
/// `enable()` so the `assets::read` / `assets::exists` host
/// imports can read the gadget's bundled files without
/// re-opening the archive. `None` outside an enable lifetime;
/// the host imports return an io-error in that case, matching
/// the "call-out-of-lifetime" contract the other capability
/// stashes use.
gadget_source: Option<Arc<dyn super::source::GadgetSource + Send + Sync>>,
```

Initialize to `None` in `WasmRuntime::instantiate` (at `GadgetState { ... }` construction around line 900, next to the other capability `None`s).

Also add it to `#[cfg(test)] fn default_for_test` at line 1307.

Add setter/clearer on `WasmGadgetInstance` (next to `set_http_client` at line 1136):

```rust
/// Stash the gadget's `GadgetSource` handle. Called by the bridge
/// at `enable()`. The `assets::*` host imports use this to read
/// the gadget's bundled files on demand.
pub fn set_gadget_source(
    &self,
    source: Arc<dyn super::source::GadgetSource + Send + Sync>,
) {
    let mut store = self.store.lock().expect("store not poisoned");
    store.data_mut().gadget_source = Some(source);
}

pub fn clear_gadget_source(&self) {
    let mut store = self.store.lock().expect("store not poisoned");
    store.data_mut().gadget_source = None;
}
```

**Error classifier** (pure function, insert just above the `impl assets::Host` block so it can be unit-tested without WASM, mirroring `check_opener_scheme` at line 567):

```rust
/// Classify a `GadgetSource` error into the WIT `assets-error`
/// variant. The host-side trait returns `anyhow::Result<...>`;
/// the two known origins of an error are (a) `validate_gadget_path`
/// rejections (wrapped in `anyhow::ensure!` / `anyhow::bail!` by
/// the guard) and (b) filesystem / archive I/O. Path-validation
/// errors produce an `InvalidPath` variant; everything else maps
/// to `IoError`.
///
/// The simpler alternative — calling `validate_gadget_path` in the
/// host import BEFORE dispatching to the trait — is preferred
/// here: it removes the ambiguity of grep-ing error strings and
/// yields a structured `InvalidPath` without a classifier at all.
/// See the `Host` impl below for the actual dispatch.
fn into_assets_io_error(e: anyhow::Error) -> bindings::torchsnap::gadget::assets::AssetsError {
    bindings::torchsnap::gadget::assets::AssetsError::IoError(format!("{e:#}"))
}
```

**The `assets::Host` impl** (insert at the end of the `impl http::Host` block — the http impl begins at line 702 and ends in the 750s, before the `WasmRuntime` struct at line 773):

```rust
// =========================================================
// Assets host import
//
// Routes guest `assets::read` / `assets::exists` calls through the
// gadget's own `GadgetSource` (stashed by the bridge on `enable`).
// Path validation runs on the host side BEFORE touching the source,
// producing the `InvalidPath` variant directly. Only when validation
// succeeds do we delegate to the source; any error from the source
// (filesystem, archive lookup) is classified as `IoError`.
//
// "File not found" is surfaced differently in each direction:
// - `read`: the host calls `source.read_file`, which returns
//   `Err` on a missing file. Detecting that without magic strings
//   would require the trait to return a richer error; instead we
//   pre-probe with `file_exists` and only call `read_file` on hit.
// - `exists`: the source's `file_exists` returns `Ok(false)` on
//   miss, which maps directly to `Ok(Ok(false))` in WIT.
// =========================================================

impl bindings::torchsnap::gadget::assets::Host for GadgetState {
    fn read(
        &mut self,
        path: String,
    ) -> Result<Vec<u8>, bindings::torchsnap::gadget::assets::AssetsError> {
        use bindings::torchsnap::gadget::assets::AssetsError;

        // Validate first — produces a structured `InvalidPath`
        // variant without matching on error strings later.
        if let Err(e) = super::source::validate_gadget_path(&path) {
            return Err(AssetsError::InvalidPath(format!("{e:#}")));
        }

        let source = self
            .gadget_source
            .as_ref()
            .ok_or_else(|| AssetsError::IoError("assets not initialized".into()))?;

        match source.file_exists(&path) {
            Ok(true) => {}
            Ok(false) => return Err(AssetsError::NotFound),
            Err(e) => return Err(into_assets_io_error(e)),
        }

        source.read_file(&path).map_err(into_assets_io_error)
    }

    fn exists(
        &mut self,
        path: String,
    ) -> Result<bool, bindings::torchsnap::gadget::assets::AssetsError> {
        use bindings::torchsnap::gadget::assets::AssetsError;

        if let Err(e) = super::source::validate_gadget_path(&path) {
            return Err(AssetsError::InvalidPath(format!("{e:#}")));
        }

        let source = self
            .gadget_source
            .as_ref()
            .ok_or_else(|| AssetsError::IoError("assets not initialized".into()))?;

        source.file_exists(&path).map_err(into_assets_io_error)
    }
}
```

**Unit tests** (pure-function and via `default_for_test`, add to the `tests` module at line 1330):

- `assets_read_rejects_traversal_path` — construct a `GadgetState` via `default_for_test`, stash a source backed by a `make_gadget_dir`, call `read("../escape")` through the `Host` impl method directly. Assert `AssetsError::InvalidPath(msg)` with `msg.contains("escapes")`.
- `assets_read_rejects_absolute_path` — same but `/etc/passwd`; error contains `"relative"`.
- `assets_read_returns_not_found_for_missing_file` — asset genuinely not present in the dir.
- `assets_read_returns_bytes_for_existing_file` — binary file round-trips.
- `assets_exists_returns_true_when_present`, `assets_exists_returns_false_when_absent`.
- `assets_read_without_gadget_source_returns_io_error` — `gadget_source: None`, expect `AssetsError::IoError("assets not initialized")`.

**Verify during implementation:** the `bindings::torchsnap::gadget::assets::AssetsError` variant path — wit-bindgen may name it `Assets_Error` or similar. Check by running `cargo doc -p torchsnap-tauri` and looking under `src-tauri/target/doc/.../bindings/`.

After this step: `cargo test -p torchsnap-tauri wasm::runtime`. Commit: `wasm/runtime: implement assets host interface`.

---

### Step 1e — Bridge wire-up: stash source on enable, clear on disable

**File:** `src-tauri/src/wasm/bridge.rs`

In `Gadget::enable` (line 522), after `instance.set_http_client(...)` at line 572, add:

```rust
// Assets: stash a clone of the source Arc the bridge already
// holds. No permission allowlist — gadgets can always read
// their own bundled files; the guarantee is spatial
// (validate_gadget_path confines reads to the gadget root).
instance.set_gadget_source(Arc::clone(&self.gadget_source));
```

In `Gadget::disable` (line 602), next to `instance.clear_http_client()` at line 620, add:

```rust
instance.clear_gadget_source();
```

Follow the existing convention: `clear_*` is only called from `disable()`, never from `enable()`'s failure paths (lines 582, 595). In those paths `drop(instance) + take_instance()` already drops the `Arc<WasmGadgetInstance>` strong count to zero (scheduler not yet spawned), so every capability field on `GadgetState` is released automatically. Adding a lone `clear_gadget_source()` to the failure paths would diverge from the existing pattern — matching `disable()`-only is simpler and keeps the PR scoped.

**Verify during implementation:** no additional state fields needed on the bridge — the new `gadget_source: Arc<dyn GadgetSource + Send + Sync>` added in step 1a is the single source of truth.

After this step: `cargo build -p torchsnap-tauri`. Commit: `wasm/bridge: wire assets gadget source into enable/disable`.

---

### Step 1f — Test fixture: `assets-gadget`

**New directory:** `src-tauri/tests/fixtures/assets-gadget/`

Structure mirrors `opener-http-gadget/`:

- `Cargo.toml` — copy of `opener-http-gadget/Cargo.toml` with `package.name = "torchsnap-test-assets-gadget"`.
- `manifest.toml` — minimal, no `[permissions]`:
  ```toml
  [gadget]
  id = "assets-gadget"
  name = "Assets Test Gadget"
  description = "Test-only fixture exercising the assets host interface"
  version = "0.0.0"
  wasm = "assets_gadget.wasm"
  icon = "heroicons:beaker"
  ```
- `greeting.txt` — a bundled asset with known content (`Hello from the assets fixture\n`).
- `src/lib.rs` — `wit_bindgen::generate!` pointing at `../../../../gadgets/gadget-sdk/wit`, same as `opener-http-gadget/src/lib.rs:20–22`. `MessagingGuest::handle_message` dispatches:
  - `"assets.read"` with `payload = "<path>"` → call `assets::read(payload)`, success returns the decoded UTF-8 string, failure returns `format!("{e:?}")`.
  - `"assets.exists"` with `payload = "<path>"` → call `assets::exists(payload)`, returns `"true"` or `"false"`, error returns `format!("{e:?}")`.
- Empty stub impls for `LifecycleGuest`, `SearchGuest`, `TasksGuest`.

**Integration tests** (add to `src-tauri/src/wasm/runtime.rs` under the existing `tests` module, alongside the opener/http fixture tests):

- `assets_read_returns_bundled_file_contents` — call `handle_message("assets.read", "greeting.txt")`, assert response contains `"Hello from the assets fixture"`.
- `assets_exists_true_for_bundled_file` — `handle_message("assets.exists", "greeting.txt")` returns `"true"`.
- `assets_exists_false_for_missing_file` — returns `"false"`.
- `assets_read_rejects_traversal` — `"../../../etc/passwd"` surfaces as `InvalidPath`, the fixture's `format!("{e:?}")` includes `"InvalidPath"`.
- `assets_read_returns_not_found_for_missing_file` — error debug contains `"NotFound"`.

**Rebuild discipline:** `just build-test-fixtures` rebuilds every fixture under `src-tauri/tests/fixtures/` and re-commits the `.wasm` artifact. CI runs against the committed `.wasm`, so the fixture's `.wasm` needs to be checked in alongside the source changes.

After this step: `just build-test-fixtures && cargo test -p torchsnap-tauri -- wasm`. Commit: `wasm/tests: add assets-gadget fixture and integration tests`.

---

### Step 1g — SDK re-export

**File:** `gadgets/gadget-sdk/src/lib.rs`

Line 93 currently reads `pub use torchsnap::gadget::{clipboard, frecency, http, opener};`. Add `assets`:

```rust
pub use torchsnap::gadget::{assets, clipboard, frecency, http, opener};
```

And in the `prelude` module (line 112):

```rust
pub use super::{assets, clipboard, frecency, http, opener};
```

No other SDK changes — there's no higher-level `assets` helper module (unlike `sql` and `messaging`, which wrap resource handles / JSON parsing). Gadgets call `assets::read(path)` directly.

After this step: `cd gadgets && cargo check --workspace`. Commit: `gadget-sdk: re-export assets host import`.

---

### Step 1g-prelude — Fold the gadget boilerplate macros into the prelude

**Files:**
- `gadgets/gadget-sdk/src/lib.rs` — extend the `prelude` module.
- `gadgets/hello-world/src/lib.rs` — drop redundant macro imports.
- `gadgets/emoji-picker/src/lib.rs` — drop redundant macro imports.

**Rationale:** The SDK's `prelude` module (line 97, comment: "Common glob import for gadget authors") re-exports guest traits, search records, and host-import modules, but not the `#[macro_export]`-ed helpers `define_gadget!`, `impl_noop_messaging!`, `impl_noop_tasks!`. Every existing gadget has to write a separate `use torchsnap_gadget_sdk::{define_gadget, impl_noop_messaging, impl_noop_tasks};` line alongside the prelude glob. That's boilerplate the prelude exists to eliminate.

Rust 2018+ supports `pub use` of `#[macro_export]`-ed macros through modules, and glob imports pick them up. Verified by the existing `pub use super::{logging, messaging, settings, sql};` pattern in the same module — the same re-export mechanism applies to macros.

**Changes:**

1. `gadgets/gadget-sdk/src/lib.rs` — in the `prelude` module (line 97–113), add a new line after the existing `pub use super::{clipboard, frecency, http, opener};`:

   ```rust
   pub use super::{define_gadget, impl_noop_messaging, impl_noop_tasks};
   ```

   Also update the prelude's doc comment at line 98 to mention the macros:
   
   > `use torchsnap_gadget_sdk::prelude::*;` pulls in the four guest traits, the search record / variant types, the SDK's helper modules, and the `define_gadget!` / `impl_noop_*!` macros gadgets use to wire themselves up.

2. `gadgets/hello-world/src/lib.rs:19` — change `use torchsnap_gadget_sdk::{define_gadget, impl_noop_messaging, impl_noop_tasks};` to just drop the line (the macros now come through the prelude already imported one line earlier via `use torchsnap_gadget_sdk::prelude::*;`). **Verify during implementation:** confirm hello-world uses the prelude — if not, keep the explicit import line to avoid unrelated churn.

3. `gadgets/emoji-picker/src/lib.rs:37` — same change: drop the explicit macro import line if the prelude is already in scope.

**Why fold this into PR 1 and not a standalone PR:** it's a trivial SDK improvement, and PR 1 already touches `gadget-sdk/src/lib.rs` in step 1g. Two SDK-side edits in one atomic commit per step keeps history clean. The new bangs gadget in PR 2 then only needs the prelude glob.

After this step: `cd gadgets && cargo check --workspace`. Commit: `gadget-sdk: export define_gadget/impl_noop_* macros via prelude`.

---

### Step 1h — Documentation

**File:** `docs/api/gadget-development.md`

Add an `### Assets (WASM gadgets)` section before the `### Opener` section at line 772. Style matches the existing HTTP/Opener sections landed in commit `f4ea3c3`. Keep it short — the design is self-evident from the interface:

- Three-sentence overview: gadgets can read their own bundled files, no permission needed, paths are gadget-relative.
- A `manifest.toml` snippet showing a `bundled/ bang.json` layout (no `[permissions]` block, to emphasize the point).
- A Rust code sample showing `assets::read(path)` and the three `AssetsError` variants being matched.
- Key points bullet list: (1) paths validated by `validate_gadget_path`, (2) bytes cross the boundary in full — not a handle, (3) typical use is enable-time data loading, (4) `exists` is cheap, no bytes transferred.

After this step: commit as `docs: document assets host interface`.

---

### Step 1i — ADR 0039: WASM gadget assets API

**New file:** `docs/adr/0039-wasm-gadget-assets-api.md`

Runs `EDITOR=true adrs new "WASM gadget assets API"` to scaffold the file with the project's ADR template, then fills it in. Style matches the two prior capability ADRs: `0037-wasm-gadget-opener-api.md` and `0038-wasm-gadget-http-api.md`.

**Why an ADR:** This is the first WASM host capability shipped **without a permission allowlist**. Every prior capability (opener, http, clipboard, sql) requires a `[permissions.<iface>]` section in `manifest.toml`. Assets deliberately does not — the trust model is spatial, guaranteed by `validate_gadget_path` confining reads to the gadget root. That trust-model decision warrants a standalone record alongside ADR 0035 (distribution) and ADR 0036 (trust model), because a future auditor asking "why is this capability unguarded?" should find the answer in `docs/adr/` rather than having to reconstruct the reasoning from source comments.

**Content outline:**

- **Status:** Accepted, 2026-04-20.
- **Context:** WASM gadgets need to read their own bundled assets (e.g. bangs' 2.2 MB `bang.json`). The existing permission-gated capabilities (opener/http) don't fit — reading your own files isn't an external resource access. A new capability is needed, and the question is whether it requires a permission declaration.
- **Decision:** Add the `assets` WIT interface with no `[permissions.assets]` section. Security is enforced by `validate_gadget_path` rejecting any path outside the gadget root (`..` traversal, absolute paths, Windows drive letters, NUL bytes, backslashes), plus canonicalization guarding against symlink escape.
- **Consequences:**
  - **Positive:** Gadgets can load bundled data without boilerplate permission declarations. Matches the expectation that a gadget's own archive is trusted.
  - **Negative:** Establishes a precedent for permission-less capabilities. Future capabilities must justify whether they follow this pattern (spatial guarantee) or the opener/http pattern (explicit allowlist).
  - **Neutral:** `validate_gadget_path` becomes load-bearing for yet another callsite — regressions there now affect one more interface. Mitigated by the extensive test coverage on the guard itself.
- **Alternatives considered:**
  - `[permissions.assets]` with a path allowlist (e.g. `files = ["bang.json"]`): rejected as friction without benefit — the gadget already ships the files it's reading, so an allowlist would be redundant self-declaration.
  - Reading assets at load time and passing them via `enable()` arguments: rejected because it defeats the lazy-load use case and bloats the instantiation path.
  - Going through the filesystem interface (none exists yet, and would be a much larger capability surface): rejected as overkill.
- **References:**
  - ADR 0035 — gadget distribution via bundled and user-installable archives
  - ADR 0036 — gadget trust model and deferred signing
  - ADR 0037 — WASM gadget opener API (comparison: permission-gated)
  - ADR 0038 — WASM gadget HTTP API (comparison: permission-gated)

After this step: commit as `docs: add ADR 0039 for WASM gadget assets API`.

---

### PR 1 verification

```
just check-wit
just fmt-check-wit
cd gadgets && cargo check --workspace
cargo build -p torchsnap-tauri
cargo test -p torchsnap-tauri wasm
just build-test-fixtures
cargo test -p torchsnap-tauri -- wasm::runtime::tests::assets
```

---

## PR 2 — Port `bangs` to WASM

**Location:** `gadgets/bangs/` as a new workspace member.

**MPL header reminder:** Every new `.rs`, `.ts`, `.tsx`, `.css`, `.sql` file starts with the 3-line MPL 2.0 header.

### Step 2a — Scaffold the gadget crate and workspace membership

**New files:**

- `gadgets/bangs/Cargo.toml` — copied from `gadgets/calculator/Cargo.toml` (lines 1–20) with these changes:
  ```toml
  [package]
  name = "bangs-gadget"
  version = "0.1.0"
  edition = "2024"

  [dependencies]
  torchsnap-gadget-sdk = { path = "../gadget-sdk" }
  serde = { version = "1", features = ["derive"] }
  serde_json = "1.0.149"
  urlencoding = "2"      # used by find_bang_token → bang URL resolution

  [lib]
  crate-type = ["cdylib"]
  ```
  (No `evalexpr`/`regex`/`blake3`/`ulid` — those are calculator-specific.)

- `gadgets/bangs/manifest.toml`:
  ```toml
  [gadget]
  id = "bangs"
  name = "Bangs"
  description = "DuckDuckGo bang shortcuts for quick web searches"
  version = "0.1.0"
  wasm = "bangs_gadget.wasm"
  icon = "heroicons:arrow-top-right-on-square"

  [storage.sql]
  migrations = ["migrations/001_init.sql"]

  [permissions.opener]
  schemes = ["https", "http"]

  [permissions.http]
  origins = ["https://duckduckgo.com"]

  [frontend]
  settings-bundle = "frontend/dist/settings.js"
  settings-css = "frontend/dist/settings.css"

  [frontend.settings]
  component = "BangsSettings"
  ```
  (No `[[tasks]]`, no `launcher-bundle` — bangs has no custom launcher UI.)

- `gadgets/bangs/migrations/001_init.sql` — content extracted verbatim from `src-tauri/src/gadgets/bangs/schema.rs:21–36`:
  ```sql
  CREATE TABLE bangs (
      trigger TEXT PRIMARY KEY COLLATE NOCASE,
      service_name TEXT NOT NULL,
      url_template TEXT NOT NULL,
      domain TEXT NOT NULL,
      category TEXT,
      subcategory TEXT,
      rank INTEGER NOT NULL DEFAULT 0
  );

  CREATE TABLE metadata (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
  );
  ```

- `gadgets/bangs/assets/bang.json` — the 2.2 MB DDG bang database. Treated as a committed asset in the repo (same way `gadgets/calculator/frontend/dist/` is committed). Bootstrap: run `just asset-bang-data` and move the output (after step 2g updates it).

**Edit:** `gadgets/Cargo.toml` (line 20) — add `"bangs"` to the `members` array:
```toml
members = [
    "gadget-sdk",
    "hello-world",
    "calculator",
    "bangs",
    "emoji-picker",
    "template",
]
```

After this step: `cd gadgets && cargo check --workspace` (passes — no `lib.rs` yet so no real compilation happens beyond the manifest check; `bun build` is not yet runnable because there's no frontend). Commit: `gadgets/bangs: scaffold crate, manifest, migrations, assets`.

---

### Step 2b — Port the guest Rust code to `gadgets/bangs/src/lib.rs`

**New file:** `gadgets/bangs/src/lib.rs`.

Structure follows `gadgets/calculator/src/lib.rs`:

```rust
// MPL header

use std::cell::RefCell;

use serde::{Deserialize, Serialize};
use serde_json::json;

use torchsnap_gadget_sdk::prelude::*;
use torchsnap_gadget_sdk::define_gadget;
use torchsnap_gadget_sdk::http::{HttpMethod, HttpRequest};
use torchsnap_gadget_sdk::sql::{SqlHandle, SqlValue};

// The pending-URL shim uses RefCell because the stored type is
// `Option<String>`, which needs interior mutability beyond what
// `Cell` can give us. Calculator uses `Cell<bool>` / `Cell<u32>`
// because its stored types are `Copy`. See the calculator's
// `thread_local!` block at gadgets/calculator/src/lib.rs:75.
thread_local! {
    // HACK/FIXME: Stores the resolved URL from the most recent
    // search() so execute() can open it. execute() only receives
    // the entry_id (which is `!<trigger>` for frecency tracking)
    // and has no access to the original query or resolved URL.
    //
    // This MUST be replaced with the arbitrary data parameter
    // mechanism once todo
    // 01kn7v6ynyf580ax9jyyt25jgc-gadget-execute-data-param.md
    // lands. The UI can only execute the currently-displayed
    // result, and search() runs synchronously per query cycle,
    // so the stored URL always matches what's on screen.
    static PENDING_URL: RefCell<Option<String>> = const { RefCell::new(None) };
}

const BANG_SCORE: u32 = 1000;
```

Mirror the native layout inside the guest:

**`LifecycleGuest`:**
- `enable()`: acquire `let db = sql::connection();` (handle lives for the function body), check `SELECT COUNT(*) FROM bangs`, and if zero rows, call `import_from_best_source(&db)`. Log `"Bangs enabled"` via `logging::log`. No state cached in a struct — `sql::connection()` is cheap and the host serializes all guest calls, so acquiring a fresh handle per call is correct (the advisor's point — reduce state to just `PENDING_URL`).
- `disable()`: `PENDING_URL.with(|c| c.borrow_mut().take());` and a log line. No SQL state to release — the host's `clear_sql_storage` handles that.
- `on_setting_changed`: no-op (the bangs gadget has no settings other than the host-managed `enabled.bangs`).

**`SearchGuest`:**
- `entries()`: `Vec::new()` — bangs is query-only, no catalog.
- `search(query, _prefix)`: direct port of `src-tauri/src/gadgets/bangs/mod.rs:160–229`. Differences:
  - `lookup_bang` takes `&SqlHandle` instead of `&SqlStorage` and calls `db.query(...)` returning `Vec<Vec<SqlValue>>` — port the helper to work off the WIT result shape. Example:
    ```rust
    fn lookup_bang(db: &SqlHandle, trigger: &str) -> Option<BangRecord> {
        let rows = db.query(
            "SELECT service_name, url_template, domain FROM bangs WHERE trigger = ?1",
            &[SqlValue::Text(trigger.to_string())],
        ).ok()?;
        let mut iter = rows.into_iter().next()?.into_iter();
        Some(BangRecord {
            service_name: expect_text(iter.next().as_ref())?,
            url_template: expect_text(iter.next().as_ref())?,
            domain: expect_text(iter.next().as_ref())?,
        })
    }
    ```
  - Icon is unconditionally `EntryIcon::HeroIcon("arrow-top-right-on-square".to_string())` — no favicon enrichment (deferred to the `website-metadata` WIT interface, tracked by todo `01kpkt8mfs679ghhs9s7c7q0g1`).
  - `ScoredEntry` no longer has `ActionKeybinding` inside `Action` (WIT has no keybinding field today — check the WIT `action` record). If the Copy action is still desired, its label alone surfaces; if the host now assigns keybindings from `ActionId::Copy`, that's enough. **Verify during implementation:** confirm `Action` in `search::Action` (WIT line 296) has no `keybinding` field — the plan ports only `id` and `label`.
  - `Utf16Positions::from_substring` is a native helper — in the WASM port, compute title_highlight_positions inline as a plain `Vec<u32>` (calculator leaves it empty for history rows; bangs can do the same — the native highlighting was a nice-to-have, not load-bearing). Or ship a small helper inline.
  - `PENDING_URL.with(|c| *c.borrow_mut() = Some(resolved_url.clone()));` replaces the `Mutex` write.

- `execute(entry_id, action_id)`: takes from `PENDING_URL`, dispatches:
  - `ActionId::Open` → `opener::open_url(&url).map_err(|e| format!("open: {e}"))?;`  Returns `PostAction::Dismiss`.
  - `ActionId::Copy` → `clipboard::write_text(&url).map_err(|e| format!("copy: {e}"))?;`  Returns `PostAction::Dismiss`.
  - Anything else → `Err(format!("unsupported action: {action_id:?}"))`.

**`MessagingGuest`:**
Port `src-tauri/src/gadgets/bangs/mod.rs:282–316` dispatch:
- `"stats"` → read the `metadata` table via `db.query`, build `BangStats` struct, return `serde_json::to_string(&stats)`.
- `"refresh"` → call `try_import_from_network(&db)`. On success, return fresh stats; on failure, return `json!({ "error": format!("{e}") }).to_string()`. Same JSON shapes as the native version.
- Unknown method → `Err(format!("unknown message method: {method}"))`.

**`TasksGuest`:** use `impl_noop_tasks!(BangsGadget);` from the SDK (see `gadgets/gadget-sdk/src/lib.rs:166`). No scheduled refresh — matches the native behavior exactly (network-try-only on enable if DB empty).

**Import helpers — port `src-tauri/src/gadgets/bangs/import.rs`:**

- `parse_bang_json(json: &str) -> Result<Vec<BangEntry>, String>` — verbatim port; returns `String` error (flatten `anyhow`).
- `import_bangs(db: &SqlHandle, entries: &[BangEntry], source: &str) -> Result<(), String>`:
  - **Landmine:** the WIT `sql` interface exposes only `execute` and `query` — NO `transaction` wrapper. The guest must explicitly bracket:
    ```rust
    db.execute("BEGIN", &[]).map_err(|e| format!("begin: {e}"))?;
    let result = (|| -> Result<(), String> {
        db.execute("DELETE FROM bangs", &[]).map_err(|e| format!("clear bangs: {e}"))?;
        db.execute("DELETE FROM metadata", &[]).map_err(|e| format!("clear metadata: {e}"))?;
        for entry in entries {
            db.execute(
                "INSERT OR IGNORE INTO bangs (trigger, service_name, url_template, domain, category, subcategory, rank) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                &[
                    SqlValue::Text(entry.t.clone()),
                    SqlValue::Text(entry.s.clone()),
                    SqlValue::Text(entry.u.clone()),
                    SqlValue::Text(entry.d.clone()),
                    match &entry.c { Some(c) => SqlValue::Text(c.clone()), None => SqlValue::Null },
                    match &entry.sc { Some(sc) => SqlValue::Text(sc.clone()), None => SqlValue::Null },
                    SqlValue::Integer(entry.r),
                ],
            ).map_err(|e| format!("insert bang: {e}"))?;
        }
        // metadata rows (import_date, source, bang_count, domain_count) — port verbatim
        Ok(())
    })();
    match result {
        Ok(()) => db.execute("COMMIT", &[]).map(|_| ()).map_err(|e| format!("commit: {e}")),
        Err(e) => {
            let _ = db.execute("ROLLBACK", &[]);
            Err(e)
        }
    }
    ```
  A 13 000-row bulk insert inside a single transaction keeps enable latency sub-second; without the BEGIN/COMMIT bracketing every INSERT is auto-committed and the enable flow becomes a multi-second freeze.

**Data-source helpers:**

- `try_import_from_network(db: &SqlHandle) -> Result<(), String>`:
  ```rust
  let request = HttpRequest {
      url: "https://duckduckgo.com/bang.js".to_string(),
      method: HttpMethod::Get,
      headers: vec![],
      body: None,
      timeout_ms: Some(30_000),  // generous; 2MB download over variable links
      max_body_size: Some(10 * 1024 * 1024),  // 10 MB ceiling
  };
  let response = http::fetch(&request).map_err(|e| format!("fetch: {e:?}"))?;
  if response.status < 200 || response.status >= 300 {
      return Err(format!("DDG returned HTTP {}", response.status));
  }
  let body = String::from_utf8(response.body).map_err(|e| format!("utf-8: {e}"))?;
  let entries = parse_bang_json(&body)?;
  import_bangs(db, &entries, "network")
  ```

- `import_from_builtin(db: &SqlHandle) -> Result<(), String>`:
  ```rust
  let bytes = assets::read("assets/bang.json").map_err(|e| format!("read bundled bang.json: {e:?}"))?;
  let body = String::from_utf8(bytes).map_err(|e| format!("bundled bang.json utf-8: {e}"))?;
  let entries = parse_bang_json(&body)?;
  import_bangs(db, &entries, "builtin")
  ```
  This is the load-bearing call to the new `assets::read` — the reason PR 1 exists.

- `import_from_best_source`: try network, on `Err(e)` log a warning and fall through to bundled. Match native behavior exactly.

**Query-token helpers** (`find_bang_token`, `byte_offset_of_token`, `remove_bang_token`): verbatim port from `src-tauri/src/gadgets/bangs/mod.rs:360–413`. These are pure functions with no host dependencies.

**Pure-function tests** (`#[cfg(test)] mod tests { ... }`): port the query-token test cases from the native implementation (search for `#[test]` in the old file — there are a handful of scattered ones inline; if not, add fresh ones covering `find_bang_token` and `remove_bang_token`). Do NOT test SQL/HTTP paths here — those need a wasmtime fixture, same separation calculator uses.

After this step: `cd gadgets && cargo check --manifest-path bangs/Cargo.toml` and `just build-gadget bangs` (produces `gadgets/bangs/bangs_gadget.wasm` and `gadgets/bangs.torchsnap`). Commit: `gadgets/bangs: port guest to WASM`.

---

### Step 2c — Port the frontend settings component

**New directory:** `gadgets/bangs/frontend/`, scaffolded from `gadgets/calculator/frontend/`:

- `package.json` — copy calculator's (no dependencies differ — same SDK + vite + tailwindcss).
- `vite.config.ts` — copy calculator's but simplify: only a `settings` entry (no `launcher`). Remove the `launcher` key from the `entries` object and simplify the `emptyOutDir` branching (always `true` since there's only one entry):
  ```ts
  const entries: Record<string, string> = {
    settings: resolve(__dirname, "src/BangsSettings.tsx"),
  };
  ```
- `tsconfig.json`, `env.d.ts` — verbatim copy from calculator.
- `styles/settings.css` — empty file that just imports tailwind base (copy calculator's `settings.css`).
- `src/BangsSettings.tsx` — moved from `src/gadgets/bangs/BangsSettings.tsx` (181 lines) using `git mv src/gadgets/bangs/BangsSettings.tsx gadgets/bangs/frontend/src/BangsSettings.tsx` so git tracks the rename and history survives. After the `git mv`, apply the following import rewrites in place:
  - `import { sendGadgetMessage } from "../../lib/gadgetMessage";` → remove; replace with the SDK hook pattern used by calculator:
    ```tsx
    import { useGadgetRuntime } from "@torchsnap/gadget-sdk/hooks";
    import { Section } from "@torchsnap/gadget-sdk/components";
    ```
    Then inside the component: `const { sendMessage } = useGadgetRuntime();` and replace the old `gadgetMessage<T>(method, payload)` wrapper with `sendMessage<unknown, T>(method, payload)` calls.
  - `import { Section } from "../../settings/Section";` → `import { Section } from "@torchsnap/gadget-sdk/components";` (calculator does this at `gadgets/calculator/frontend/src/settings/CalculatorSettings.tsx:27`).
  - Remove the module-level `const PLUGIN_ID = "bangs"` — the SDK's `sendMessage` is already scoped to the current gadget.
  - Import the styles: `import "../styles/settings.css";`
  - **Convert to a named export** to match the calculator convention (`gadgets/calculator/frontend/src/settings/CalculatorSettings.tsx:40` uses `export function CalculatorSettings()`). Change the current `export default function BangsSettings()` to `export function BangsSettings()`. The manifest's `[frontend.settings] component = "BangsSettings"` resolves against this named export, same as calculator's `component = "CalculatorSettings"`.

After this step: `cd gadgets/bangs/frontend && bun install && bun run build` produces `gadgets/bangs/frontend/dist/settings.{js,css}`. Then `just build-gadget bangs` re-packages. Commit: `gadgets/bangs: port settings frontend from legacy gadgets/bangs/`.

---

### Step 2d — Update `just/bangs.just` to target the new asset location

**File:** `just/bangs.just`

Change the `dst` path at line 13 from `src-tauri/derived/bang.json` to `gadgets/bangs/assets/bang.json`. The recipe becomes:

```just
# Download DuckDuckGo bang database into gadgets/bangs/assets/
asset-bang-data:
    #!/usr/bin/env bash
    set -euo pipefail
    dst="gadgets/bangs/assets/bang.json"
    mkdir -p "$(dirname "$dst")"
    echo "Downloading DuckDuckGo bang database..."
    curl -sL 'https://duckduckgo.com/bang.js' -o "$dst"
    echo "Bang data: done ($(wc -l < "$dst" | tr -d ' ') lines)."
```

No other changes needed. The `package-gadget` recipe in `just/gadgets.just` already blacklists `src/`, `Cargo.toml`, `frontend/src/`, etc. — it does not exclude `assets/`, so the committed `bang.json` ships inside the `.torchsnap` archive automatically. **Verify during implementation:** confirm `gadgets/bangs/assets/` isn't accidentally excluded by the blacklist (reading `just/gadgets.just:127-141` shows it is not).

After this step: `just asset-bang-data` (writes the 2.2 MB file to the new location). Commit: `just/bangs: move asset download into gadgets/bangs/assets/`.

---

### Step 2e — Add `bangs` to the bundled gadgets list

**File:** `gadgets/bundled.toml`

Change line 13–16:
```toml
gadgets = [
    "calculator",
    "emoji-picker",
    "bangs",
]
```

After this step: `just stage-bundled-gadgets` produces `target/bundled-gadgets/bangs.torchsnap`. Commit: `gadgets: add bangs to bundled.toml`.

---

### Step 2f — Delete the native implementation

**Files to delete:**

- `src-tauri/src/gadgets/bangs/mod.rs` (497 lines)
- `src-tauri/src/gadgets/bangs/import.rs` (127 lines)
- `src-tauri/src/gadgets/bangs/schema.rs` (36 lines)
- `src-tauri/derived/bang.json` (2.2 MB)
- `src/gadgets/bangs/` — remove the now-empty directory (`BangsSettings.tsx` was `git mv`'d in step 2c, so git already tracks the rename; nothing to delete from inside this directory).

**Files to edit:**

- `src-tauri/src/gadgets/mod.rs:23` — remove `pub mod bangs;` line.
- `src-tauri/src/lib.rs:658–663` — remove the `host.register(Box::new(gadgets::bangs::BangsGadget::new(...)), wasm::source::GadgetSourceKind::Builtin);` block. `metadata_service` stays — it's still used by `open_url` at line 653.
- `src/gadgets/registry.ts:87–92` — remove the `registerGadget("bangs", { ... })` block. The WASM gadget's frontend is registered automatically via the gadget bundle's manifest (calculator does the same).

**Verify during implementation:** the native `BangsGadget` used `network::website_metadata::WebsiteMetadataService` for favicon enrichment — the WASM port doesn't (favicons deferred). Confirm nothing else imports or constructs `metadata_service` specifically for bangs; `open_url` still uses it, so the construction stays.

After this step: `cargo build -p torchsnap-tauri && cargo test -p torchsnap-tauri` — the native tests related to bangs are gone along with the module; host test suite stays green. `bun run typecheck` (frontend) passes — `src/gadgets/registry.ts` no longer references the removed file. Commit: `bangs: remove native gadget and derived bang.json`.

---

### PR 2 verification

```
just check-wit
cd gadgets && cargo check --workspace
just build-gadget bangs
just stage-bundled-gadgets       # produces target/bundled-gadgets/bangs.torchsnap
cargo build -p torchsnap-tauri
cargo test -p torchsnap-tauri
bun run typecheck                # frontend
bun run lint                     # frontend
# Manual smoke test: launch app, type "!g rust", press Enter → opens google.com/search
# Manual smoke test: open Settings → Bangs, see stats populated, click Refresh → fresh DDG download
```

---

## Files touched

| File | Change | PR |
|---|---|---|
| `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` | Add `assets` interface + reorder world imports to match definition order + add `import assets;` | 1 |
| `src-tauri/src/wasm/source.rs` | `GadgetSource::file_exists` trait method + `DirectorySource`/`ArchiveSource` impls + tests | 1 |
| `src-tauri/src/wasm/runtime.rs` | `GadgetState::gadget_source` field, setter/clearer on `WasmGadgetInstance`, `assets::Host` impl, unit + integration tests | 1 |
| `src-tauri/src/wasm/bridge.rs` | `gadget_source: Arc<dyn GadgetSource + Send + Sync>` field, `new()` signature, stash on enable, clear on disable | 1 |
| `src-tauri/src/lib.rs` | Update `load_single_wasm_gadget` call site to pass `Arc::clone(&source)`; remove native `BangsGadget` registration | 1, 2 |
| `src-tauri/tests/fixtures/assets-gadget/` | New test fixture (`Cargo.toml`, `manifest.toml`, `src/lib.rs`, `greeting.txt`, committed `.wasm`) | 1 |
| `src-tauri/src/wasm/bindings.rs` | Auto-regenerated | 1 |
| `gadgets/gadget-sdk/src/lib.rs` | Re-export `assets` via `pub use` and in `prelude`; fold `define_gadget!`/`impl_noop_*!` macros into `prelude` | 1 |
| `gadgets/hello-world/src/lib.rs` | Drop redundant `use torchsnap_gadget_sdk::{define_gadget, impl_noop_messaging, impl_noop_tasks};` now covered by prelude | 1 |
| `gadgets/emoji-picker/src/lib.rs` | Drop redundant macro imports now covered by prelude | 1 |
| `docs/api/gadget-development.md` | Add `### Assets` section before `### Opener` | 1 |
| `docs/adr/0039-wasm-gadget-assets-api.md` | New ADR documenting the permission-less capability decision | 1 |
| `gadgets/Cargo.toml` | Add `"bangs"` to workspace `members` | 2 |
| `gadgets/bangs/Cargo.toml` | New crate manifest | 2 |
| `gadgets/bangs/manifest.toml` | New gadget manifest with opener+http permissions | 2 |
| `gadgets/bangs/migrations/001_init.sql` | SQL schema (extracted from `schema.rs`) | 2 |
| `gadgets/bangs/assets/bang.json` | 2.2 MB bundled DDG bang database | 2 |
| `gadgets/bangs/src/lib.rs` | WASM guest port | 2 |
| `gadgets/bangs/frontend/` | Scaffold (`package.json`, `vite.config.ts`, `tsconfig.json`, `env.d.ts`, `styles/settings.css`) | 2 |
| `gadgets/bangs/frontend/src/BangsSettings.tsx` | Moved + SDK-adjusted from `src/gadgets/bangs/BangsSettings.tsx` | 2 |
| `gadgets/bundled.toml` | Add `"bangs"` | 2 |
| `just/bangs.just` | `dst="gadgets/bangs/assets/bang.json"` | 2 |
| `src-tauri/src/gadgets/mod.rs` | Remove `pub mod bangs;` | 2 |
| `src-tauri/src/gadgets/bangs/{mod,import,schema}.rs` | Deleted | 2 |
| `src-tauri/derived/bang.json` | Deleted | 2 |
| `src/gadgets/bangs/BangsSettings.tsx` | `git mv` to `gadgets/bangs/frontend/src/BangsSettings.tsx` | 2 |
| `src/gadgets/bangs/` | Empty dir removed after `git mv` | 2 |
| `src/gadgets/registry.ts` | Remove `registerGadget("bangs", ...)` block | 2 |

---

## Deleted / deferred todos

**Resolved by this plan:**
- `todos/wasm/01kpk5kyd7cfeat7rerg2gygt2-convert-bangs-to-wasm.md` — bangs WASM conversion.
- `todos/wasm/01kpk8jp3s737phqs239s9vt59-gadget-assets-wit-interface.md` — gadget-assets host interface (renamed to `assets`).

**NOT resolved (deliberately deferred):**
- `todos/wasm/01kpkt8mfs679ghhs9s7c7q0g1-wasm-host-website-metadata-interface.md` — `website-metadata` WIT interface. Needed to re-enable favicon enrichment in the bangs result icon (currently hardcoded to `arrow-top-right-on-square`). The bangs port intentionally ships without enrichment so this PR can land independently.
- `todos/01kn7v6ynyf580ax9jyyt25jgc-gadget-execute-data-param.md` — arbitrary-data parameter for `execute`. Required to remove the `PENDING_URL` thread-local hack (port-directly from the native `pending_url: Mutex<Option<String>>`). The `// HACK/FIXME` comment in `gadgets/bangs/src/lib.rs` references this todo explicitly so the next author has a thread to pull.

---

## Known landmines captured in the plan

1. The bridge does NOT currently own its `GadgetSource` — step 1a adds the field and rewrites `new()` to take `Arc<_>`.
2. `file_exists` vs. error classification — the host-side `assets::Host::read` runs `validate_gadget_path` BEFORE delegating to the trait, producing `InvalidPath` structurally and leaving `IoError` as the only failure category for the trait call.
3. `ArchiveSource::file_exists` needs `zip::result::ZipError::FileNotFound` detection — flagged as "verify during implementation" because the zip crate's error enum path depends on the pinned version.
4. The WIT `sql` interface has no `transaction` wrapper — step 2b spells out explicit `BEGIN`/`COMMIT`/`ROLLBACK` bracketing for the 13 000-row bulk import.
5. Calculator uses `Cell`, not `RefCell` — the plan uses `RefCell` for `PENDING_URL` because `Option<String>` isn't `Copy`, and comments the distinction so the next reader doesn't assume the idiom was copied verbatim.
6. The bangs port does NOT need a `BangState` struct — `sql::connection()` is cheap and acquired per call, so the `thread_local!` only holds `PENDING_URL`. Simpler than the native code.
7. 2.2 MB `bang.json` crosses the WIT boundary once per enable (cold-fallback path only) — not a hot path, no concern.
8. No permission allowlist on `assets` — path validation is the security boundary. The WIT doc comment and `docs/api/gadget-development.md` both make this explicit.

---

## Critical files for implementation

- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/gadgets/gadget-sdk/wit/torchsnap-gadget.wit`
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/wasm/source.rs`
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/wasm/runtime.rs`
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/src-tauri/src/wasm/bridge.rs`
- `/Users/jakob/Development/github/jakobwesthoff/torchsnap/gadgets/bangs/src/lib.rs` (new)
