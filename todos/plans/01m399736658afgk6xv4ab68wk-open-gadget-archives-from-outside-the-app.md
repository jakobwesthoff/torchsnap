---
kind: plan
status: open
---

# Open `.torchsnap` archives from outside the app

Planned in full on 2026-09-24, then reviewed against the code and
revised the same day (review findings and the follow-up discussion are
folded in). Every open question is decided (see "Decisions"). What
remains unverified is listed under "Unverified assumptions", each with
the step that settles it.

## Goal

A user double-clicks a `.torchsnap` file, or picks it in the Gadgets
settings, or drops it there, or passes it on the command line.
Torchsnap stages a private copy, shows a review of what the gadget is
and what it may do, and installs or replaces it on confirmation. The
user can undo each install until the next restart. Every entry point
goes through one pipeline.

macOS ships first. The design already covers Linux (argv delivery,
MIME registration) and a later `torchsnap://` install link, which
stays parked.

## Decisions

Decided with the maintainer on 2026-09-24.

| Topic | Decision |
|---|---|
| In-app picker and drop zone | Go through the same queue, staging and review as external origins. `install_gadget_archive` is removed. |
| Where the review appears | A modal in Settings → Gadgets. The settings window opens (or comes to front) on the Gadgets section when the queue goes from empty to non-empty. |
| Minimal confirm vs full review | No minimal dialog. The full review dialog is part of the first version. |
| When the review appears | Always, for every request. |
| Several files at once | One dialog per request, in arrival order. Results collect into one banner with a single restart prompt. |
| Already installed id | Offer to replace. Replacing keeps the gadget's data (`gadget-home/<id>/` and settings). |
| Permission changes on replace | The review marks each permission as added or unchanged compared with the installed version and lists removed ones. |
| Downgrades | Allowed with a warning. Rejected when the incoming version declares fewer SQL migrations than the installed one, because the gadget could not open its database and would silently fail to load. |
| Replace over a directory-form user gadget | Rejected with "installed as a directory under gadgets/, remove it by hand". |
| Undo | Every install and replace in the result banner has "Undo" until the banner is dismissed or the app restarts. Dismissing the banner gives up undo. |
| Uninstall cleanup | Deferred: uninstall removes the archive and writes a marker; the next startup deletes `gadget-home/<id>/` and the settings keys before any gadget loads. |
| Staging location | `<app_cache_dir>/install-staging/<request-ulid>.torchsnap`. Leftovers are removed at startup. |
| Archive size cap at staging | 16 MiB (16 × 1024 × 1024 bytes) on the archive file. Decompression caps stay with the zip-bomb todo. |
| Download provenance (macOS) | Shown when `kMDItemWhereFroms` or `com.apple.quarantine` can be read. Missing metadata is silently skipped. |
| Permission wording | Rust returns structured permission items with a severity. The frontend words them. |
| MIME type | `application/vnd.torchsnap.gadget+zip` |
| UTI | `app.torchsnap.gadget`, conforming to `public.data` only |
| Zip conformance | None on either platform, although the MIME name keeps `+zip`. Linux behaviour caused by the suffix is checked when Linux ships and accepted. |
| `CFBundleTypeRole` / `LSHandlerRank` | `Viewer` / `Owner` |
| Document icon | In the first version, derived from the app icon. |
| argv and single-instance | Ship together with the macOS work. |
| Second launch without files | Show the launcher (do nothing if it is already visible). |
| Control socket install command | No, until the socket has authentication. |
| AppImage self-registration | On by default, with a settings toggle and a "remove integration" action. Only relevant once AppImage ships. |
| URL scheme | Parked. When revisited: `torchsnap://install?url=<https-url>[&sha256=<hex>]`, ask before downloading, any HTTPS host with the final host shown. |

## Findings that shape the design

Each was checked in source or docs on 2026-09-24.

- **Testability of install today.** `install_impl` and
  `uninstall_impl` in `src-tauri/src/gadget_install.rs` depend on
  `AppHandle` only for `app.path().app_data_dir()` and
  `app.store("settings.json")`. Their existing tests cover a copy of
  the settings-key matcher and two `ArchiveSource::open` error paths.
- **`GadgetHost` cannot live in a mock app.** `GadgetHost::new` takes
  `Arc<Store<tauri::Wry>>`, so a `tauri::test::MockRuntime` app cannot
  manage one. Commands must not take `State<Arc<GadgetHost>>`; `setup`
  manages plain snapshot values instead.
- **No frontend tests exist.** No runner, no test files, no Tauri
  mocks. `@tauri-apps/api` 2.11.1 ships `mocks` (`mockIPC`,
  `mockWindows`, `clearMocks`). The project uses Vite 8 (rolldown),
  and `vite.config.ts` exports an async function.
- **`tauri::test` is available** behind tauri's `test` feature
  (`mock_builder`, `mock_context`, `noop_assets`, `get_ipc_response`).
  Its `MockRuntime` returns a dangling AppKit window pointer, so no
  tested command may reach `show_settings_window` (which dereferences
  `ns_window()`).
- **The registry is frozen after setup.** `GadgetHost.slots` is
  append-only during setup. `gadget_sources()` returns a snapshot, so
  installs and uninstalls change disk but not the registry until
  restart. `wasm::protocol::GadgetSourceRegistry` (managed in setup)
  holds every loaded source with its manifest and `root_path()`.
- **Startup loading** (`load_wasm_gadgets`, `wasm::discovery`) picks
  up `*.torchsnap` files and directories with a `manifest.toml` under
  `<app_data_dir>/gadgets/`. Within one root an archive shadows a
  directory of the same stem. Dotfiles and temp files are ignored.
- **Manifest permissions** are `PermissionsDef` with `opener`
  (schemes, `open_path`, `reveal_path`), `http` (origins, literal
  `"*"` allowed), `filesystem` (read patterns), `command` rules
  (binary, argv constraints `Literal`/`Enum`/`Glob`/`Regex`, cwd,
  limits) and the flags `website_metadata`, `settings`, `frecency`,
  `sql_storage`, `clipboard`, `icon_cache`, `path_resolver`. They
  serialize with kebab-case keys, and `WasmGadgetManifest` in
  `src/lib/command.ts` has no `permissions` field, so the frontend
  gets permissions only through the new review types.
- **Per-rule command limits are not enforced.** `cwd`,
  `timeout-ms-max`, `max-output-bytes` and `max-stdin-bytes` are
  parsed and dropped
  (`todos/gadget-host/caps/01kwg1ajrvfsmxyjmm7bvcs04b-command-per-rule-limits-unenforced.md`).
- **SQL downgrades break loading.** `SqlStorage::open` runs
  `Migrations::to_latest`; rusqlite_migration 2.6 returns
  `DatabaseTooFarAhead` when the database `user_version` exceeds the
  migration count. The error fails gadget registration, and load
  failures are silent today
  (`todos/gadget-host/wasm/01m37q57cegzwx50404fgzg20v-gadget-load-failure-is-silent.md`).
- **The compile cache is safe across replace.** Entries are keyed by
  the blake3 hash of the WASM and stale ones are pruned
  (`wasm/runtime/cached_component.rs`).
- **`RunEvent::Opened` timing.** tao handles only
  `application:openURLs:` and forwards it whenever its callback is
  installed, without a queue. Tauri runs `setup` on `Ready`. AppKit is
  believed to deliver launch-time document opens before
  `applicationDidFinishLaunching`, so on a Finder cold start `Opened`
  probably arrives before `setup`, when no install dependency exists
  yet. The queue therefore buffers raw inputs until `setup` starts it.
  The `Opened` variant only exists on macOS, iOS and Android, so the
  run-loop arm is `#[cfg(target_os = "macos")]`.
- **Info.plist merging.** The bundler inserts every key of
  `src-tauri/Info.plist` into the generated plist, replacing whole
  top-level keys. `bundle.fileAssociations` generates
  `CFBundleDocumentTypes` and `UTExportedTypeDeclarations` but has no
  icon field, so a document icon means writing both keys in full in
  `src-tauri/Info.plist`. `fileAssociations` stays in
  `tauri.conf.json` for the Linux `.desktop` `MimeType=` line.
  `tauri dev` embeds the whole file into the binary, but a bare binary
  is never registered with LaunchServices, so dev builds register no
  types.
- **Settings window.** Built on demand, destroyed on close, presented
  after the frontend emits `react-ready`. `SettingsPanel` keeps
  `activeSection` in local state with no way to set it from outside.
  The gadget list only shows registered gadgets, so pending installs
  never appear in it before restart.
- **Launcher.** `toggle_launcher_window` hides a visible launcher, so
  the second-launch handler needs a show-only variant.
- **Single-instance plugin** 2.4.3: `init(|app, args, cwd| ..)` with
  `args: Vec<String>`, must be registered first. On macOS it uses a
  Unix socket to hand over args from a directly started second binary;
  LaunchServices itself never starts a second bundle instance.

## Architecture

### Backend: `src-tauri/src/gadget_install/`

`gadget_install.rs` becomes a directory module. Everything except
`commands.rs` and the run-loop adapters takes paths and data, never
`AppHandle` or `GadgetHost`, so tests drive it directly.

| File | Responsibility |
|---|---|
| `mod.rs` | Module narrative, re-exports. |
| `paths.rs` | `InstallPaths { gadgets_dir, gadget_home_dir, staging_dir }`. |
| `store.rs` | `SettingsKeys` trait (`keys`, `delete`, `save`), implemented for the tauri-plugin-store store and for an in-memory test double. |
| `registered.rs` | `RegisteredGadgets`: snapshot taken in `setup` of every gadget's kind, and for User gadgets their manifest and whether the source is a directory. |
| `pending.rs` | `PendingChanges`: per-id record of installs, replacements and uninstalls since startup. |
| `decision.rs` | `decide_install` and `decide_uninstall`. Pure. |
| `archive_ops.rs` | Filesystem steps: publish (fresh or replace, with `.prev` backup), undo, uninstall with marker, startup processing of markers and backups. |
| `staging.rs` | `StagingArea`: capped copy, open the staged copy with `ArchiveSource`, discard, startup sweep. |
| `review.rs` | `InstallReview`, `PermissionItem`, change marking, version relation, severity rules. |
| `provenance.rs` | macOS xattr reader and parsers. `None` on other platforms. |
| `intake.rs` | `InstallOrigin`, raw input normalization, argv and opened-URL parsing. |
| `queue.rs` | `InstallQueue`: buffering before start, ordered requests, dedup, request state machine, change hook. |
| `commands.rs` | Tauri commands and the `install-queue-changed` event. Thin. |

`setup` manages plain values: `InstallPaths`,
`Arc<dyn SettingsKeys>`, `RegisteredGadgets`,
`Mutex<PendingChanges>`, and starts the `InstallQueue` that `run()`
created. Commands take only these (plus `AppHandle` for `emit`), so
tests manage temp dirs, a fixed registry snapshot and the in-memory
store on a mock app.

Request lifecycle:

```text
raw input ──▶ buffered ──start──▶ normalize ──stage──▶ Ready(review) ──confirm──▶ Installing ──▶ done
 (before setup)          (setup)  (intake.rs)  (staging.rs,  │                  (re-decide,        (outcome in
                                               review.rs)    │                   archive_ops.rs,    the batch)
                                                  │          └──dismiss──▶ gone  pending.rs)
                                                  └──error──▶ Failed(message) ──dismiss──▶ gone
```

Core types (names final unless a test shows otherwise):

```rust
pub enum InstallOrigin { SettingsPicker, SettingsDrop, OsOpenFile, CommandLine }

pub enum RequestState {
    Staging,
    Ready(InstallReview),
    Installing,
    Failed(String),
}

pub enum InstallDecision {
    Fresh,
    Replace { previous: Manifest, relation: VersionRelation },
    Reject(RejectReason),
}

pub enum VersionRelation { Upgrade, Same, Downgrade, Unknown }

pub enum PendingChange {
    Installed { manifest: Manifest },
    Replaced { previous_version: String, manifest: Manifest },
    Uninstalled,
}
```

`VersionRelation` compares `GadgetMeta.version` with the `semver`
crate (`cargo add semver`); either side unparsable gives `Unknown`.

`decide_install` rules:

| Registered (frozen snapshot) | Pending change | Decision |
|---|---|---|
| none | none | `Fresh` |
| none | `Installed`/`Replaced` | `Replace` against the pending manifest |
| `User`, archive source | none | `Replace` against the registered manifest |
| `User`, directory source | none | `Reject` (installed as a directory) |
| `User` | `Uninstalled` | `Fresh` |
| `User` | `Installed`/`Replaced` | `Replace` against the pending manifest |
| `Builtin`, `System`, `Dev` | any | `Reject` with today's kind-specific messages |
| any `Replace` above | | `Reject` if incoming declares SQL storage and fewer migrations than the previous manifest ("this version cannot open the data of the installed one; uninstall first, then install this version") |

`PendingChanges` transitions (each pinned by a test):

| Before | Event | After |
|---|---|---|
| none | fresh install | `Installed { manifest }` |
| `Installed` | uninstall or undo | record removed (so "none + `Uninstalled`" never exists) |
| none, registered | replace | `Replaced { previous_version: registered, manifest }` |
| `Replaced` | replace again | `Replaced` keeps its `previous_version`, takes the new manifest |
| `Replaced` | undo | record removed |
| none, registered | uninstall | `Uninstalled` |
| `Uninstalled` | fresh install | `Installed { manifest }` |

`decide_uninstall` allows `User` gadgets and pending installs, and
rejects everything else.

Locking: confirm takes the `InstallQueue` lock, then the
`PendingChanges` lock, always in that order. Under both it sets the
request to `Installing`, re-runs `decide_install` against the current
registry and pending state, and fails the request with "changed since
review, open the file again" if the decision or the previous manifest
differs from what was reviewed. Confirm on `Staging`, `Installing` or
`Failed` is rejected.

Filesystem operations in `archive_ops.rs`:

- **Publish fresh:** temp-then-rename to `<id>.torchsnap`.
- **Publish replace:** if `.<id>.torchsnap.prev` does not exist,
  rename the current `<id>.torchsnap` to it (the running instance
  keeps its open handle to the same inode), then temp-then-rename the
  new archive. The backup therefore always holds what ran at startup,
  or the pending install that was replaced.
- **Undo:** with a backup, rename it back over `<id>.torchsnap`; without
  one (fresh install), remove `<id>.torchsnap`. Then adjust
  `PendingChanges` per the table.
- **Uninstall:** remove `<id>.torchsnap` and `.<id>.torchsnap.prev`,
  write the marker `.<id>.uninstall`. `gadget-home/<id>/` and settings
  stay until the next startup.
- **Startup processing** (first thing in `setup` after the settings
  store opens, before settings init and gadget loading): for each
  `.<id>.uninstall`, delete `gadget-home/<id>/`, strip `enabled.<id>`
  and `gadgets.<id>.*`, save the store, delete the marker. Then delete
  all `.*.torchsnap.prev`. Processing is idempotent, so a crash
  halfway repeats cleanly.

Replace never touches `gadget-home/<id>/` or settings.

Review model (`review.rs`):

- `InstallReview { gadget, source, provenance, decision, permissions,
  removed_permissions }`, serialized in camelCase.
- Permission items are fine-grained: one per HTTP origin, per
  filesystem pattern, per opener scheme, per command rule, per opener
  flag and per boolean flag. Each carries `severity` and
  `change: Added | Unchanged` (always `Added` for a fresh install).
- Change detection compares a content struct with `PartialEq` that
  excludes `severity` and `change`. A changed argv constraint on the
  same binary is one removed and one added item.
- Command items carry binary and argv constraints only. The per-rule
  limits stay out until they are enforced.

Severity rules (tested one by one):

| Permission | Severity |
|---|---|
| any `command` rule | warning |
| `http` origin `"*"` | warning |
| other `http` origins | notice |
| `opener.open_path` | warning |
| `opener` schemes, `reveal_path` | notice |
| `filesystem.read` patterns | notice |
| `clipboard`, `website_metadata` | notice |
| `settings`, `frecency`, `sql_storage`, `icon_cache`, `path_resolver` | info |

### Tauri surface

| Command | Purpose |
|---|---|
| `install_queue_snapshot() -> Vec<InstallRequestView>` | Pull the queue. |
| `install_queue_submit(paths: Vec<String>, origin)` | Picker and drop zone. Never opens a window. |
| `install_queue_confirm(request_id) -> InstallOutcome` | Install or replace a `Ready` request. |
| `install_queue_dismiss(request_id)` | Cancel a request or clear a failed one; deletes its staged file. |
| `install_undo(gadget_id) -> UndoOutcome` | Undo a fresh install or a replace made in this session. |
| `uninstall_user_gadget(gadget_id)` | Kept, now through `decide_uninstall`, `PendingChanges` and the marker. |
| `gadget_permissions() -> HashMap<String, Vec<PermissionItem>>` | Permission items of every registered WASM gadget for the gadget cards, built by `review.rs`. |

Event `install-queue-changed` (no payload) tells open windows to pull
again.

Entry points feeding `InstallQueue::submit`:

- `RunEvent::Opened { urls }` arm in the `app.run` closure (macOS).
- argv of the first instance (`std::env::args_os`), submitted at the
  end of `setup`.
- `tauri-plugin-single-instance` callback for later launches.
- `install_queue_submit` from the settings window.

Submits from outside the settings window call `show_settings_window`
from the adapter, never from a command.

### Frontend: `src/settings/install/`

| File | Responsibility |
|---|---|
| `types.ts` | TS mirrors of `InstallRequestView`, `InstallReview`, `PermissionItem`, `InstallOutcome`, `UndoOutcome`. |
| `useInstallQueue.ts` | Pull on mount, re-pull on `install-queue-changed`, `confirm`, `dismiss`, `undo`, collected batch results. |
| `permissionText.ts` | Turns a `PermissionItem` into a title and plain-language lines. |
| `PermissionSummary.tsx` | Shared permission list with severity styling and added/removed markers. Used by the modal and the gadget cards. |
| `InstallReviewModal.tsx` | First request: identity, source, provenance, version relation, permissions with changes, Install / Cancel. Escape cancels, focus is trapped. |
| `InstallResultBanner.tsx` | All outcomes of the batch, Undo per install or replace, one "Restart now". |

`GadgetsManagementPanel.tsx` loses `runInstall` and calls
`install_queue_submit`. `SettingsPanel.tsx` switches to the Gadgets
section when the queue goes from empty to non-empty (on mount or on
change), and never again while it stays non-empty.

## Working rules for every step

- **Track each phase with the task tool.** Before starting a phase,
  create one task per step of that phase (with the step's tests and
  resolved todos in the description), mark a task in progress when
  work on it starts and completed only after its commit landed. This
  keeps progress visible and makes an interrupted session easy to
  resume.
- **Test first.** Write the tests for the step, run them, and see
  them fail for the expected reason (a compile error for a missing
  type counts only when no test can be written against an existing
  API). Then implement until they pass, then refactor with the tests
  green. Where a step cannot have an automated failing test (step 19),
  the plan says so and names the manual observation instead.
- **Coverage.** Every new or changed function gets tests for its
  success path and each error branch. Rust tests live in the module's
  `#[cfg(test)] mod tests`; tests that go through `spawn_blocking` use
  `#[tokio::test]` (dev `tokio` has `rt` and `macros`). Frontend tests
  sit next to the file as `*.test.ts(x)`.
- **Only the run-loop and plugin wiring stays untested
  automatically.** Each such piece is a few lines that call a tested
  function, and the manual checklist covers it.
- **License headers.** Every new source file (`.rs`, `.ts`, `.tsx`,
  `vitest.config.ts`, `.just`, shell scripts) starts with the MPL-2.0
  header from `CLAUDE.md`.
- **One commit per step**, grouped by meaning. Each commit builds and
  passes `just fullcycle`. User-visible steps add their
  `CHANGELOG.md` entry in the same commit. Todos a step resolves are
  deleted in that step's commit.

## Implementation steps

### Phase 0: test foundations

**Step 1: frontend test infrastructure.**
- First check that the current Vitest release supports Vite 8; pin a
  version that does.
- Add Vitest, jsdom, `@testing-library/react`,
  `@testing-library/user-event` and `@testing-library/jest-dom`
  (`bun add -D`).
- `vitest.config.ts` awaits the `vite.config.ts` factory and merges it
  (`mergeConfig`), so the three aliases stay in one place.
- `src/test/setup.ts` registers jest-dom and calls `clearMocks()`
  after each test. `src/test/tauri.ts` adds `mockCommands(handlers)`,
  a `mockIPC` wrapper typed against `CommandMap`, and a helper that
  emits a Tauri event into the mocked runtime.
- `package.json` gets a `test` script. `just/quality.just` gets
  `test-frontend` (depending on `asset-mascot-data`, like
  `check-types`), added to `test` and so to `fullcycle`.
- Test first: `src/lib/command.test.ts` covers the `command()`
  wrapper (arguments passed through, rejection logged and re-thrown).
- Trims `todos/frontend/infra/01kncj7kcsdjcc25ka2c42fzj6-frontend-test-infrastructure.md`
  to its remaining priority targets.

**Step 2: Rust test support.**
- Add `tauri = { version = "2", features = ["test"] }` under
  `[dev-dependencies]`.
- Add `archive_with_manifest(id, version, extra_toml)` next to
  `minimal()` in `wasm/manifest/test_helpers.rs`, building a real
  `.torchsnap` in a temp dir. The private builders in `wasm/source.rs`
  tests switch to it.
- Test first: the helper's archive opens with `ArchiveSource::open`
  and carries the given id, version and permissions. Existing
  `source.rs` tests stay green.

### Phase 1: install core (pure Rust)

**Step 3: decouple install and uninstall from `AppHandle`.**
- `git mv src-tauri/src/gadget_install.rs src-tauri/src/gadget_install/mod.rs`,
  then split out `paths.rs`, `store.rs`, `registered.rs` and
  `archive_ops.rs`. `setup` manages `InstallPaths`,
  `Arc<dyn SettingsKeys>` and `RegisteredGadgets`; the two existing
  commands take those instead of `AppHandle` paths and
  `State<Arc<GadgetHost>>`.
- Test first (characterization against the new signatures): fresh
  install publishes `<id>.torchsnap` and leaves no temp file; the
  four kind-specific collision messages; non-zip and manifest-less
  archives are rejected with nothing written; uninstall removes the
  archive and the directory form; settings cleanup removes
  `enabled.<id>` and `gadgets.<id>.*` and keeps `calc` and
  `calculator` apart (replacing the copied `keys_to_strip_for`
  tests); a failing `save()` propagates as an error; uninstall logs
  when the expected archive is absent (fix 4 of the trust todo).
- Trims fix 4 from
  `todos/gadget-host/install/01kwh4j9bptrayf451yzd2145q-gadget-install-uninstall-trust.md`.

**Step 4: pending changes and install decisions.**
- `pending.rs` and `decision.rs`, wired into install and uninstall.
  `setup` manages `Mutex<PendingChanges>`.
- Test first: one test per row of the `decide_install` table except
  the replace-specific rows (step 6); `decide_uninstall` for
  registered user, pending install, builtin, system, dev and unknown
  id; every `PendingChanges` transition row.
- Resolves
  `todos/gadget-host/install/01kwh2e8mne5bd05tpb4paacwd-install-uninstall-blocked-until-restart.md`
  (deleted).
- CHANGELOG `Fixed`: uninstalling and reinstalling a gadget, or
  removing a just-installed one, works without restarting in between.

**Step 5: deferred uninstall cleanup.**
- Uninstall writes `.<id>.uninstall` instead of deleting
  `gadget-home/<id>/` and settings; startup processing handles the
  markers before settings init and gadget loading.
- Test first: uninstall leaves gadget-home and settings in place and
  writes the marker; startup processing deletes gadget-home, strips
  the keys, saves and removes the marker; a gadget reinstalled before
  restart starts with empty data; processing a marker twice is
  harmless; markers for other ids are untouched.
- Resolves
  `todos/gadget-host/install/01kwh2e8mne5bd05tpb4paacwc-uninstall-live-gadget-resurrects-state.md`
  (deleted).
- CHANGELOG `Fixed`: uninstalling a running gadget no longer leaves
  behind or recreates its data.

**Step 6: replace with backup and undo.**
- `archive_ops` publish-replace with the `.prev` backup, undo for
  fresh installs and replaces, backup sweep at startup. Replace rows
  of `decide_install`, including the directory-source reject and the
  SQL-migration reject. `VersionRelation` with `semver`.
- Test first: replace swaps the archive and keeps gadget-home and
  `gadgets.<id>.*`; the backup holds the original archive and a second
  replace keeps it; undo of a replace restores the original bytes and
  removes the pending record; undo of a fresh install removes the
  archive; uninstall removes the backup; startup deletes leftover
  backups; directory-source reject; fewer migrations rejects, equal or
  more passes, no SQL storage passes; `VersionRelation` for upgrade,
  same, downgrade and unparsable versions.
- CHANGELOG `Added`: installing another version of an installed gadget
  replaces it and keeps its data; installs can be undone until
  restart.

**Step 7: staging area.**
- `staging.rs`: copy through `Read::take(CAP + 1)` into
  `<staging_dir>/<ulid>.torchsnap` (created with `create_new`, so an
  existing path or symlink is never followed), reject above 16 MiB,
  open the staged copy with `ArchiveSource`, `discard`, `sweep`.
- Test first: copy below the cap; exactly 16 MiB passes; one byte
  more fails and leaves no file; an invalid archive fails and leaves
  no file; a source changed after staging does not affect the staged
  copy; `discard` removes the file; `sweep` only touches the staging
  dir.
- Install publishes from the staged copy. Carries out fix 2 of the
  trust todo ("validate what you publish"); trimmed in this commit.

**Step 8: review model and gadget permissions.**
- `review.rs` per the architecture section, plus the
  `gadget_permissions` command built from `RegisteredGadgets`.
- Test first: no permissions gives empty lists; one test per
  permission kind for its fine-grained items; one test per severity
  row; a fresh install marks everything `Added`; a replace marks
  unchanged, added and removed items correctly, including an argv
  change on the same binary; severity changes alone do not count as a
  change; command items carry no per-rule limits; version relation
  and reject reasons appear in the review; serialization snapshot of a
  full review fixes the TS contract; `gadget_permissions` returns
  items for registered WASM gadgets only.

**Step 9: download provenance on macOS.**
- `provenance.rs`: `parse_where_froms(&[u8])` (binary plist array of
  strings via `plist`), `parse_quarantine(&[u8])`
  (`flags;timestamp;agent;uuid`), `read_provenance(&Path)` with
  `libc::getxattr`, a size probe and a retry on `ERANGE`. Absent
  attributes give `None`. Other platforms return `None`.
- Test first: both parsers with fixture bytes (valid, empty, wrong
  plist type, malformed); on macOS a test sets both attributes with
  `libc::setxattr` on a temp file and reads them back; a file without
  attributes gives `None`.

### Phase 2: intake and queue

**Step 10: intake normalization and queue.**
- `intake.rs` and `queue.rs`. `InstallQueue::new()` in `run()` only
  buffers raw inputs. `start(deps)` in `setup` hands over paths,
  registry, pending changes, store and the change hook, and drains the
  buffer. After start, `submit` normalizes, stages on a blocking
  worker, builds the review and fires the hook on every state change.
  Inputs that are not `*.torchsnap` are logged and dropped.
- Test first: normalization of absolute paths, `file://` URLs
  (percent-encoded), relative paths with a cwd, non-UTF-8 `OsStr`
  paths, other extensions; submit before start buffers and start
  processes it; dedup of the same canonical path while pending and
  acceptance after it resolved; arrival order; staging failure gives
  `Failed` with the error chain; the hook fires for add, ready,
  installing, done and dismiss; `dismiss` deletes the staged file;
  confirm on `Staging`, `Installing` and `Failed` is rejected; a
  second confirm of the same request is rejected; confirm fails with
  "changed since review" when another request for the same id
  installed first and when the gadget was uninstalled in between.

**Step 11: queue commands and event.**
- `commands.rs` with `install_queue_snapshot`, `install_queue_submit`,
  `install_queue_confirm`, `install_queue_dismiss`, `install_undo` and
  the event. `install_gadget_archive` stays registered until step 15.
  Startup sweeps staging.
- Test first, on `tauri::test::mock_builder` with managed temp paths,
  a fixed `RegisteredGadgets` and the in-memory store: submit then
  snapshot returns the request; confirm installs into the temp gadgets
  dir and returns the outcome; confirm on an unknown id errors;
  dismiss removes the request; undo after confirm restores the
  previous state; every mutating call emits `install-queue-changed`;
  no command opens a window.

### Phase 3: frontend

**Step 12: types and queue hook.**
- `types.ts`, `CommandMap` entries, `useInstallQueue.ts`.
- Test first: the hook pulls on mount; re-pulls on the event;
  `confirm`, `dismiss` and `undo` call the right commands and update
  batch results; results reset when the banner is dismissed; unlisten
  runs even if unmount happens before `listen` resolved.

**Step 13: permission wording and summary component.**
- `permissionText.ts` and `PermissionSummary.tsx`. Gadget cards in
  `GadgetsManagementPanel.tsx` show the summary from
  `gadget_permissions`.
- Test first: one wording test per permission kind, including each
  argv constraint variant and `"*"` origins; the component renders
  nothing for an empty list, orders warnings first, marks severity,
  marks added items and lists removed ones; a gadget card shows its
  permissions.
- Resolves `todos/gadget-host/wasm/01kq7x2ge7d3ykf7vxkz4fvr69-show-gadget-permissions-in-settings.md`
  (deleted) and section 1 of
  `todos/product/features/01krp751n5tddffjtb8fr7nnpr-permission-ui-transparency.md`.
- CHANGELOG `Added`: Settings → Gadgets shows each gadget's
  permissions.

**Step 14: review modal and result banner.**
- `InstallReviewModal.tsx` and `InstallResultBanner.tsx`, mounted in
  `GadgetsManagementPanel`. `SettingsPanel` switches to Gadgets on
  the empty-to-non-empty transition.
- Test first: only the first ready request shows; name, version, id,
  source path and provenance line; upgrade, same-version and
  downgrade notices (downgrade as a warning); added and removed
  permissions on replace; a rejected request shows its reason and only
  a dismiss action; Install confirms and moves to the next request;
  Cancel and Escape dismiss; focus stays in the modal; staging shows
  progress; failed shows the error; the banner lists every outcome,
  offers Undo per install and replace and one restart; Undo calls
  `install_undo` and updates the entry; the section switch happens on
  the transition only.
- Resolves section 2 of the transparency todo (deleted) and
  `todos/gadget-host/install/01m399736658afgk6xv4ab68wn-install-review-dialog.md`
  (deleted).

**Step 15: picker and drop zone through the queue.**
- Replace `runInstall` with `install_queue_submit`; remove
  `install_gadget_archive` from `generate_handler!`, the Rust code
  and `CommandMap` in the same commit.
- Test first: the picker submits the chosen path with origin
  `SettingsPicker`; a drop submits all `.torchsnap` paths with
  `SettingsDrop`; a mixed drop is still rejected; the drag-drop
  listener is removed even when unmount beats registration.
- Resolves
  `todos/frontend/settings/01kwg3xjw8zpbb5et77vb06jjr-dragdrop-listener-registration-race.md`
  and
  `todos/frontend/settings/01kwg3xjw8zpbb5et77vb06jjs-multi-drop-banner-overwrites-errors.md`
  (both deleted).
- CHANGELOG `Changed`: installing from Settings shows a review first;
  several dropped files are reviewed one after another.

At this point the in-app flow is complete and releasable on its own.

### Phase 4: OS entry points (macOS first)

**Step 16: open files from Finder.**
- `intake::paths_from_opened_urls(&[Url])` keeps `file://` URLs and
  logs others. A `#[cfg(target_os = "macos")]` `RunEvent::Opened` arm
  submits them with origin `OsOpenFile`; after the queue has started,
  it shows the settings window, otherwise `setup` shows it once it
  drains a non-empty buffer.
- Test first: the URL filter (file URLs, non-file schemes, URLs that
  do not map to a path); the queue reports whether draining the buffer
  produced requests, which `setup` uses to decide on showing the
  window. The run-loop arm itself is covered by the manual checklist.
- Deletes `todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md`.

**Step 17: argv and single instance.**
- Register `tauri-plugin-single-instance` first in `run()`. Its
  callback passes `args` and `cwd` to `intake::paths_from_args` and
  submits with origin `CommandLine`, or shows the launcher when there
  are none. `setup` submits the first instance's `std::env::args_os()`
  the same way.
- Extract `show_launcher_window` (show-only) from
  `toggle_launcher_window`.
- Test first: `paths_from_args` over `OsString`s (skip program name,
  skip `-` flags, keep paths and `file://` URLs, resolve relative
  paths against cwd, keep non-UTF-8 paths); the extracted show
  decision does not hide a visible launcher.
- Deletes `todos/gadget-host/install/01m399736658afgk6xv4ab68wp-argv-intake-and-single-instance.md`.
- CHANGELOG `Added`: `.torchsnap` paths on the command line open the
  install review; starting Torchsnap again shows the launcher.

**Step 18: ADRs.**
- "Install pipeline with staging and review": one queue for all
  origins, staging copy, review always with permission changes,
  replace keeps data, undo until restart via `.prev`, deferred
  uninstall cleanup, SQL-migration downgrade reject, 16 MiB cap, no
  control-socket origin. Amends ADR 0035 and ADR 0036 (the review is
  the install-time consent 0036 left open; signing stays deferred).
- "Register `.torchsnap` as an exported type": identifiers,
  `public.data` conformance, Viewer/Owner, icon through
  `src-tauri/Info.plist`.

**Step 19: register the file type in the bundle.**
- `bundle.fileAssociations` in `tauri.conf.json` with the decided
  identifiers.
- `src-tauri/Info.plist` with complete `CFBundleDocumentTypes` and
  `UTExportedTypeDeclarations`, adding `CFBundleTypeIconFile` and
  `UTTypeIconFiles`.
- `src-tauri/icons/gadget-document.icns` generated from
  `app-icon-source.png` by a `just` recipe, bundled with the
  `bundle.resources` entry
  `"icons/gadget-document.icns": "gadget-document.icns"`.
- `just verify-bundle` (guarded to macOS with `os()`) runs a
  shellchecked script that reads the built bundle's `Info.plist` with
  `plutil -extract` and fails unless type, UTI, conformance, MIME tag,
  role, rank and icon keys hold the decided values and the icon file
  exists in `Contents/Resources`. `release-build` runs it.
- No automated red: `fullcycle` never builds a bundle. The red is
  observed by hand: `just build` then `just verify-bundle` fails
  before the change and passes after.
- CHANGELOG `Added`: `.torchsnap` files open in Torchsnap and show the
  install review.
- Deletes `todos/platform/01m399736658afgk6xv4ab68wq-macos-torchsnap-file-association.md`.

**Step 20: documentation.** Separate commit in `torchsnap-docs`:
- `start/settings.mdx`: install through picker, drop or double-click,
  the review, replace, undo.
- `development/packaging.mdx`: fix the "place the archive in the user
  gadgets directory" instruction, describe the review, the 16 MiB
  limit and the SQL-migration downgrade rule.

### Later, not in this plan's first delivery

- Linux registration and AppImage self-registration (their todos,
  blocked on Linux packaging).
- The URL scheme (parked todo).
- A UI for pending installs outside the result banner. Hot lifecycle
  makes it unnecessary.

## Manual verification checklist (bundled macOS build)

Run on `just build --release`, copied to `/Applications` and launched
once, after `just verify-bundle` passed.

1. `mdls -name kMDItemContentType x.torchsnap` reports
   `app.torchsnap.gadget`; Finder shows the document icon.
2. App not running: double-click → app starts, settings opens on
   Gadgets, review shows.
3. App running, settings closed: same.
4. Settings open on another section: switches to Gadgets once, and
   stays where the user navigates afterwards.
5. Three files opened at once: three reviews in a row, one banner,
   one restart prompt.
6. File downloaded with Safari: provenance line shows the host.
7. Replace: install v1, restart, open v2 with an extra permission,
   the added permission is marked, confirm, restart, data kept.
8. Undo a fresh install and a replace, restart: the previous state
   loads.
9. Downgrade without SQL storage: warning shown, install works.
   Downgrade with fewer migrations: rejected with the reason.
10. Uninstall then reinstall before restart: after restart the gadget
    runs with fresh data.
11. Non-gadget renamed to `.torchsnap`: failed request with a readable
    error. File above 16 MiB: rejected at staging.
12. `Contents/MacOS/<binary> x.torchsnap` while running: request
    arrives in the running instance, no second instance stays up.
    Starting it again without args shows the launcher.
13. Notarized release DMG: checks 1 and 2.

## Unverified assumptions

| Assumption | Settled by |
|---|---|
| On a Finder cold start `Opened` arrives before `setup`. The design handles both orders through buffering. | Checklist item 2, with a log line in the run-loop arm noting whether the queue had started. |
| `bundle.resources` places the `.icns` so `CFBundleTypeIconFile` resolves. | Step 19, `verify-bundle` plus checklist item 1. |
| The single-instance Unix-socket handover works for directly started binaries on macOS. | Checklist item 12. |
| `getxattr` works on quarantined downloads for an unsandboxed app. | Step 9 test plus checklist item 6. |
| The bundler merges `src-tauri/Info.plist` as described (read from the GitHub source of `tauri-cli-v2.11.5`, not locally). | `verify-bundle` in step 19. |

## Out of scope (tracked elsewhere)

- Decompression caps for archive reads:
  `todos/gadget-host/wasm/01kwh4j9bptrayf451yzd2145g-archive-decompressed-size-unbounded.md`.
  The 16 MiB file cap does not bound decompressed size.
- Loader dedup against builtin ids and the stem/id bijection (fixes 1
  and 3 of the trust todo).
- Enforcing per-rule command limits (they then join the review).
- Surfacing gadget load failures:
  `todos/gadget-host/wasm/01m37q57cegzwx50404fgzg20v-gadget-load-failure-is-silent.md`.
- Hot install without restart:
  `todos/gadget-host/wasm/01kpdsvj5at6agxst1jva5eeva-gadget-hot-lifecycle.md`.

## Todos in this plan

- `todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md`
- `todos/gadget-host/install/01m399736658afgk6xv4ab68wn-install-review-dialog.md`
- `todos/gadget-host/install/01m399736658afgk6xv4ab68wp-argv-intake-and-single-instance.md`
- `todos/platform/01m399736658afgk6xv4ab68wq-macos-torchsnap-file-association.md`
- `todos/platform/linux/01m399736658afgk6xv4ab68wr-linux-torchsnap-file-association.md`
- `todos/platform/linux/01m399736658afgk6xv4ab68ws-linux-runtime-mime-self-registration.md`
- `todos/product/features/01m399736658afgk6xv4ab68wt-torchsnap-url-scheme-install.md`

Existing todos this plan resolves: install/uninstall restart blocking
(step 4), uninstall resurrecting live state (step 5), permissions in
settings (step 13), permission transparency (steps 13 and 14),
drag-drop listener race and multi-drop banner (step 15), and fixes 2
and 4 of the install trust todo (steps 7 and 3). The
frontend-test-infrastructure todo is started in step 1.
