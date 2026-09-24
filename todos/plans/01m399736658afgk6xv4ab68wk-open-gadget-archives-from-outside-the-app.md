---
kind: plan
status: open
---

# Open `.torchsnap` archives from outside the app

Planned in full on 2026-09-24. Every open question from the earlier
drafts is decided (see "Decisions"). What remains unverified is listed
under "Unverified assumptions", each with the step that settles it.

## Goal

A user double-clicks a `.torchsnap` file, or picks it in the Gadgets
settings, or drops it there, or passes it on the command line.
Torchsnap stages a private copy, shows a review of what the gadget is
and what it may do, and installs or replaces it on confirmation.
Every entry point goes through one pipeline.

macOS ships first. The design already covers Linux (argv delivery,
MIME registration) and a later `torchsnap://` install link, which
stays parked.

## Decisions

Decided with the maintainer on 2026-09-24.

| Topic | Decision |
|---|---|
| In-app picker and drop zone | Go through the same queue, staging and review as external origins. `install_gadget_archive` is removed. |
| Where the review appears | A modal in Settings → Gadgets. The settings window opens (or comes to front) on the Gadgets section when a request arrives. |
| Minimal confirm vs full review | No minimal dialog. The full review dialog is part of the first version. |
| When the review appears | Always, for every request. |
| Several files at once | One dialog per request, in arrival order. Results collect into one summary with a single restart prompt at the end. |
| Already installed id | Offer to replace. The dialog names installed and incoming version. Replacing keeps the gadget's data (`gadget-home/<id>/` and settings). |
| Staging location | `<app_cache_dir>/install-staging/<request-ulid>.torchsnap`. Leftovers are removed at startup. |
| Archive size cap at staging | 16 MiB (16 × 1024 × 1024 bytes) on the archive file. Decompression caps stay with the zip-bomb todo. |
| Download provenance (macOS) | Shown in the dialog when `kMDItemWhereFroms` or `com.apple.quarantine` can be read. Missing metadata is silently skipped. |
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
  The real install and uninstall flows have no automated test.
- **No frontend tests exist.** No runner, no test files, no Tauri
  mocks. `@tauri-apps/api` 2.11.1 ships `mocks` (`mockIPC`,
  `mockWindows`, `clearMocks`).
- **`tauri::test` is available** behind tauri's `test` feature
  (`mock_builder`, `mock_context`, `noop_assets`, `get_ipc_response`).
  The project does not enable it yet.
- **The registry is frozen after setup.** `GadgetHost.slots` is
  append-only during setup. `gadget_sources()` returns a snapshot of
  it, so installs and uninstalls change disk but not the registry
  until restart.
- **Startup loading** (`load_wasm_gadgets`, `wasm::discovery`) only
  picks up `*.torchsnap` files and directories with a
  `manifest.toml` under `<app_data_dir>/gadgets/`. The first root
  that provides an id wins. Dotfiles and temp files are ignored.
- **The manifest has everything the review needs** except an author:
  `GadgetMeta { id, name, description, version, .. }` and
  `PermissionsDef` with `opener` (schemes, `open_path`,
  `reveal_path`), `http` (origins, literal `"*"` allowed),
  `filesystem` (read patterns), `command` rules (binary, argv
  constraints `Literal`/`Enum`/`Glob`/`Regex`, cwd, limits) and the
  flags `website_metadata`, `settings`, `frecency`, `sql_storage`,
  `clipboard`, `icon_cache`, `path_resolver`. All derive `Serialize`.
- **`RunEvent::Opened` timing.** tao handles only
  `application:openURLs:` and forwards it only when its callback is
  installed; there is no queue. Tauri runs `setup` on `Ready`. The
  intake queue is therefore created in `run()` before the builder and
  shared with the run-loop closure, so an early event is never lost
  even if `setup` has not finished.
- **Info.plist merging.** The bundler inserts every key of
  `src-tauri/Info.plist` into the generated plist, replacing whole
  top-level keys. `bundle.fileAssociations` generates
  `CFBundleDocumentTypes` and `UTExportedTypeDeclarations` but has no
  icon field. A document icon therefore means writing both keys in
  full in `src-tauri/Info.plist`. `fileAssociations` stays in
  `tauri.conf.json` for the Linux `.desktop` `MimeType=` line.
  `tauri dev` embeds only three keys of that file and never registers
  types.
- **Settings window.** Built on demand, destroyed on close, presented
  after the frontend emits `react-ready`. `SettingsPanel` keeps
  `activeSection` in local state with no way to set it from outside.
- **Launcher.** `toggle_launcher_window` hides a visible launcher, so
  the second-launch handler needs a show-only variant built from its
  show branch.
- **Single-instance plugin** 2.4.3: `init(|app, args, cwd| ..)`,
  must be registered first. On macOS it uses a Unix socket to hand
  over args from a directly started second binary; LaunchServices
  itself never starts a second bundle instance.

## Architecture

### Backend: `src-tauri/src/gadget_install/`

`gadget_install.rs` becomes a directory module. Everything except
`commands.rs` and the run-loop adapters is plain Rust that takes paths
and data, never `AppHandle`, so tests drive it directly.

| File | Responsibility |
|---|---|
| `mod.rs` | Module narrative, re-exports. |
| `paths.rs` | `InstallPaths { gadgets_dir, gadget_home_dir, staging_dir }`, built once from `AppHandle` in setup. |
| `store.rs` | `SettingsKeys` trait over the settings store (`keys`, `delete`, `save`), implemented for the tauri-plugin-store store and for an in-memory test double. |
| `pending.rs` | `PendingChanges`: per-id record of installs, replacements and uninstalls since startup. |
| `decision.rs` | `decide_install(registered, pending, incoming) -> InstallDecision` and `decide_uninstall(..)`. Pure. |
| `archive_ops.rs` | Filesystem steps: publish a staged archive as `<id>.torchsnap` (fresh or replace), remove a user gadget. |
| `staging.rs` | `StagingArea`: capped copy into the staging dir, open the staged copy with `ArchiveSource`, discard, startup sweep. |
| `review.rs` | `InstallReview` and `PermissionItem` built from a `Manifest` plus decision plus provenance. Severity rules live here. |
| `provenance.rs` | macOS xattr reader and the two parsers (`kMDItemWhereFroms` binary plist, `com.apple.quarantine` string). Empty on other platforms. |
| `intake.rs` | `InstallOrigin`, `InstallRequest`, normalization of raw inputs (paths, `file://` URLs, relative paths + cwd, extension filter), argv parsing. |
| `queue.rs` | `InstallQueue`: ordered requests, dedup by canonical path while pending, per-request state, change notification hook. |
| `commands.rs` | Tauri commands and the `install-queue-changed` event. Thin. |

Request lifecycle:

```text
raw input ──normalize──▶ InstallRequest ──stage──▶ Ready(review) ──confirm──▶ Installed / Replaced
   (intake.rs)             (queue.rs)     (staging.rs,     │                   (archive_ops.rs,
                                           review.rs)      └──dismiss──▶ gone   pending.rs)
                                             │
                                             └──error──▶ Failed(message) ──dismiss──▶ gone
```

Core types (names final unless a test shows otherwise):

```rust
pub enum InstallOrigin { SettingsPicker, SettingsDrop, OsOpenFile, CommandLine }

pub enum RequestState {
    Staging,
    Ready(InstallReview),
    Failed(String),
}

pub enum InstallDecision {
    Fresh,
    Replace { installed_version: String },
    Reject(RejectReason),
}

pub enum PendingChange {
    Installed { version: String },
    Replaced { from: String, to: String },
    Uninstalled,
}
```

`decide_install` rules:

| Registered kind (frozen) | Pending change | Decision |
|---|---|---|
| none | none | `Fresh` |
| none | `Installed`/`Replaced` | `Replace` (against the pending version) |
| none | `Uninstalled` | cannot happen (uninstall needs a registered or pending install) |
| `User` | none | `Replace` (against the registered version) |
| `User` | `Uninstalled` | `Fresh` (disk is already clean) |
| `User` | `Installed`/`Replaced` | `Replace` (against the pending version) |
| `Builtin`, `System`, `Dev` | any | `Reject` with today's kind-specific messages |

`decide_uninstall` allows `User` gadgets and pending installs, and
rejects everything else with a message that says when a restart is
needed.

Replacing writes the new archive through the existing temp-then-rename
step, so the running instance keeps its open handle to the old inode
until restart. It never touches `gadget-home/<id>/` or settings.

Severity rules in `review.rs` (tested one by one):

| Permission | Severity |
|---|---|
| any `command` rule | warning |
| `http` origins containing `"*"` | warning |
| other `http` origins | notice |
| `opener.open_path` | warning |
| `opener` schemes, `reveal_path` | notice |
| `filesystem.read` patterns | notice |
| `clipboard`, `website_metadata` | notice |
| `settings`, `frecency`, `sql_storage`, `icon_cache`, `path_resolver` | info |

The review lists every declared permission; severity only changes how
it is highlighted. Command rules show their binary and argv
constraints only. Their per-rule `cwd`, `timeout-ms-max`,
`max-output-bytes` and `max-stdin-bytes` are parsed but not enforced
(`todos/gadget-host/caps/01kwg1ajrvfsmxyjmm7bvcs04b-command-per-rule-limits-unenforced.md`),
so the review leaves them out rather than promise limits that do not
hold. They get added to the review when that todo is fixed.

### Tauri surface

| Command | Purpose |
|---|---|
| `install_queue_snapshot() -> Vec<InstallRequestView>` | Pull the queue on mount. |
| `install_queue_submit(paths: Vec<String>, origin)` | Picker and drop zone. |
| `install_queue_confirm(request_id) -> InstallOutcome` | Install or replace a `Ready` request. |
| `install_queue_dismiss(request_id)` | Cancel a request or clear a failed one; deletes its staged file. |
| `uninstall_user_gadget(gadget_id)` | Kept, now routed through `decide_uninstall` and `PendingChanges`. |

Event `install-queue-changed` (no payload) tells open windows to pull
again. Pull plus notify works whether or not the settings window
existed when the request arrived.

Entry points feeding `InstallQueue::submit`:

- `RunEvent::Opened { urls }` arm in the `app.run` closure (macOS).
- argv of the first instance, read at the end of `setup`.
- `tauri-plugin-single-instance` callback for later launches.
- the two settings commands above.

After a submit from outside the settings window, the backend calls
`show_settings_window`. The settings frontend switches to the Gadgets
section whenever the queue is non-empty on mount or on change.

### Frontend: `src/settings/install/`

| File | Responsibility |
|---|---|
| `types.ts` | TS mirrors of `InstallRequestView`, `InstallReview`, `PermissionItem`, `InstallOutcome`. |
| `useInstallQueue.ts` | Pull on mount, re-pull on `install-queue-changed`, expose `confirm`, `dismiss`, collected results. |
| `permissionText.ts` | Turns a `PermissionItem` into a title and plain-language lines. |
| `PermissionSummary.tsx` | Shared list of permissions with severity styling. Used by the modal and by installed gadget cards. |
| `InstallReviewModal.tsx` | Shows the first request: identity, source, provenance, replace notice, permissions, Install / Cancel. Escape cancels, focus is trapped. |
| `InstallResultBanner.tsx` | Summary of all results in the current batch, with one "Restart now". |

`GadgetsManagementPanel.tsx` loses `runInstall` and calls
`install_queue_submit` from the picker and the drop handler.
`SettingsPanel.tsx` gains the switch to the Gadgets section.

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
  green.
- **Coverage.** Every new or changed function gets tests for its
  success path and each error branch. Rust tests live in the module's
  `#[cfg(test)] mod tests`. Frontend tests sit next to the file as
  `*.test.ts(x)`.
- **Only the run-loop and plugin wiring stays untested
  automatically.** Each such piece is a few lines that call a tested
  function, and the manual checklist at the end covers it.
- **One commit per step**, grouped by meaning. Each commit builds and
  passes `just fullcycle`. User-visible steps add their
  `CHANGELOG.md` entry in the same commit. Todos a step resolves are
  deleted in that step's commit.

## Implementation steps

### Phase 0: test foundations

**Step 1: frontend test infrastructure.**
- Add Vitest, jsdom, `@testing-library/react`,
  `@testing-library/user-event` and `@testing-library/jest-dom`
  (`bun add -D`).
- `vitest.config.ts` reuses the aliases from `vite.config.ts`.
- `src/test/setup.ts` registers jest-dom and calls `clearMocks()`
  after each test.
- `src/test/tauri.ts` adds `mockCommands(handlers)`, a wrapper around
  `mockIPC` typed against `CommandMap`, plus a helper that emits a
  Tauri event into the mocked runtime.
- `package.json` gets a `test` script. `just/quality.just` gets
  `test-frontend`, added to `test` and so to `fullcycle`.
- Test first: `src/lib/command.test.ts` fails before the setup exists
  and covers the `command()` wrapper (arguments passed through,
  rejection logged and re-thrown).
- Trims `todos/frontend/infra/01kncj7kcsdjcc25ka2c42fzj6-frontend-test-infrastructure.md`
  to its remaining priority targets.

**Step 2: Rust test support.**
- Add `tauri = { version = "2", features = ["test"] }` under
  `[dev-dependencies]`.
- Move the archive builders from `wasm/source.rs` tests into a
  `pub(crate)` `test_support` module (`#[cfg(test)]`) so install tests
  can build real `.torchsnap` files. Add `archive_with_manifest(id,
  version, permissions_toml)`.
- Test first: a smoke test that builds an archive with the helper and
  opens it with `ArchiveSource::open`. The existing `source.rs` tests
  move to the helper and stay green.

### Phase 1: install core (pure Rust)

**Step 3: decouple install and uninstall from `AppHandle`.**
- `git mv src-tauri/src/gadget_install.rs src-tauri/src/gadget_install/mod.rs`,
  then split out `paths.rs`, `store.rs` and `archive_ops.rs`.
- Test first (characterization, against the new signatures): fresh
  install publishes `<id>.torchsnap` and leaves no temp file; each
  of the four kind-specific collision messages; non-zip and
  manifest-less archives are rejected with nothing written; uninstall
  removes archive, directory form and gadget-home; settings cleanup
  removes `enabled.<id>` and `gadgets.<id>.*` and keeps `calc`
  vs `calculator` apart (replacing the copied `keys_to_strip_for`
  tests with tests of the real code); a failing `save()` propagates
  as an error instead of being swallowed.
- The Tauri commands shrink to building `InstallPaths` and the store
  adapter.

**Step 4: pending changes and install decisions.**
- `pending.rs` and `decision.rs`. `GadgetHost` is untouched;
  `PendingChanges` is its own managed state behind a `Mutex`.
- Test first: one test per row of the `decide_install` table, plus
  `decide_uninstall` for registered user, pending install, builtin,
  system, dev and unknown id, and `PendingChanges` transitions
  (install then uninstall, uninstall then install, replace twice).
- Wire into install and uninstall. This resolves
  `todos/gadget-host/install/01kwh2e8mne5bd05tpb4paacwd-install-uninstall-blocked-until-restart.md`
  (deleted in this commit).

**Step 5: replace an installed user gadget.**
- `archive_ops::publish` handles `Replace`: same temp-then-rename,
  returns the previous version, leaves gadget-home and settings alone.
- Test first: replace swaps the archive contents; gadget-home files
  and `gadgets.<id>.*` keys survive; replacing a pending install works
  before restart; the outcome reports `from` and `to` versions.
- CHANGELOG `Added`: installing a newer version of an installed gadget
  replaces it and keeps its data.

**Step 6: staging area.**
- `staging.rs`: copy through `Read::take(CAP + 1)` into
  `<staging_dir>/<ulid>.torchsnap`, reject above 16 MiB, open the
  staged copy with `ArchiveSource`, `discard`, and `sweep` for
  startup.
- Test first: copy succeeds below the cap; exactly 16 MiB passes;
  one byte more fails and leaves no file; an invalid archive fails
  after copy and leaves no file; the source file changed after
  staging does not affect the staged copy; `discard` removes the
  file; `sweep` removes leftovers only inside the staging dir.
- Install now publishes from the staged copy. This carries out fix 2
  of `todos/gadget-host/install/01kwh4j9bptrayf451yzd2145q-gadget-install-uninstall-trust.md`
  ("validate what you publish"); that todo is trimmed in this commit.

**Step 7: review model.**
- `review.rs`: `InstallReview { gadget, source, provenance, decision,
  permissions }` and `PermissionItem`, serialized in camelCase.
- Test first: a manifest without permissions gives an empty list; one
  test per permission kind for its item shape; one test per severity
  row; a replace decision carries both versions; a rejected decision
  carries its message; command items carry binary and argv but no
  per-rule limits; serialization snapshot of a full review so the TS
  mirror has a fixed contract.

**Step 8: download provenance on macOS.**
- `provenance.rs`: `parse_where_froms(&[u8])` (binary plist array of
  strings, via the `plist` crate), `parse_quarantine(&[u8])`
  (`flags;timestamp;agent;uuid`), and `read_provenance(&Path)` using
  `libc::getxattr` with a size probe and a retry on `ERANGE`. Absent
  attributes give `None`, never an error. Non-macOS builds return
  `None`.
- Test first: both parsers with fixture bytes (valid, empty, wrong
  plist type, malformed); on macOS a test sets both attributes on a
  temp file with `libc::setxattr` and reads them back; a file without
  attributes gives `None`.

### Phase 2: intake and queue

**Step 9: intake normalization and queue.**
- `intake.rs` and `queue.rs`. `submit` normalizes each input, stages
  it on a blocking worker, builds the review and calls the change
  hook after every state change. Inputs that are not `*.torchsnap`
  are logged and dropped. The queue is created in `run()` before the
  builder and managed in `setup`.
- Test first: normalization of absolute paths, `file://` URLs
  (including percent-encoding), relative paths with a cwd, non-UTF-8
  safe handling, other extensions; dedup of the same canonical path
  while pending and acceptance after it resolved; request order is
  arrival order; staging failure gives `Failed` with the error chain;
  the change hook fires for add, ready, confirm and dismiss;
  `dismiss` deletes the staged file.

**Step 10: queue commands and event.**
- `commands.rs` with the four queue commands and the event. Remove
  `install_gadget_archive` from `generate_handler!` and `CommandMap`.
  Startup calls `StagingArea::sweep`.
- Test first, with `tauri::test::mock_builder`: submit then snapshot
  returns the request; confirm on a ready request installs into the
  temp gadgets dir and returns the outcome; confirm on an unknown id
  errors; dismiss removes the request; each call emits
  `install-queue-changed`.

### Phase 3: frontend

**Step 11: types and queue hook.**
- `types.ts`, `CommandMap` entries, `useInstallQueue.ts`.
- Test first: the hook pulls on mount; re-pulls on the event;
  `confirm` and `dismiss` call the right commands and record results;
  results collect across requests and reset when the batch is
  acknowledged; unlisten runs even if unmount happens before `listen`
  resolved (the race described in the drag-drop todo).

**Step 12: permission wording and summary component.**
- `permissionText.ts` and `PermissionSummary.tsx`. Gadget cards in
  `GadgetsManagementPanel.tsx` show the same summary for installed
  WASM gadgets, using the manifests `wasm_gadgets` already returns.
- Test first: one wording test per permission kind, including each
  argv constraint variant (`Literal`, `Enum`, `Glob`, `Regex`) and
  `"*"` origins; the
  component renders nothing for an empty list, orders warnings first
  and marks each severity; a gadget card shows its permissions.
- Resolves `todos/gadget-host/wasm/01kq7x2ge7d3ykf7vxkz4fvr69-show-gadget-permissions-in-settings.md`
  and section 1 of
  `todos/product/features/01krp751n5tddffjtb8fr7nnpr-permission-ui-transparency.md`.
- CHANGELOG `Added`: Settings → Gadgets shows each gadget's
  permissions.

**Step 13: review modal and results.**
- `InstallReviewModal.tsx`, `InstallResultBanner.tsx`, mounted in
  `GadgetsManagementPanel`. `SettingsPanel` switches to the Gadgets
  section when the queue is non-empty.
- Test first: the modal shows the first ready request only; name,
  version, id, source path and provenance line; replace notice with
  both versions; a rejected request shows its reason and only a
  dismiss action; Install calls confirm and moves on to the next
  request; Cancel and Escape dismiss; focus stays inside the modal; a
  staging request shows progress; a failed request shows its error;
  the banner lists every outcome and offers one restart; the panel
  switches to Gadgets when a request exists on mount.
- Resolves section 2 of the transparency todo (the todo is deleted)
  and `todos/gadget-host/wasm/01kq7x2ge7d3ykf7vxkz4fvr6a-install-time-permission-consent.md`
  (already folded into the review dialog todo, see below).
- Deletes `todos/gadget-host/install/01m399736658afgk6xv4ab68wn-install-review-dialog.md`.

**Step 14: picker and drop zone through the queue.**
- Replace `runInstall` with `install_queue_submit`. Drop the single
  banner slot for installs.
- Test first: the picker submits the chosen path with origin
  `SettingsPicker`; a drop submits all `.torchsnap` paths with
  `SettingsDrop`; a mixed drop is still rejected; the drag-drop
  listener is removed even when unmount beats registration.
- Resolves
  `todos/frontend/settings/01kwg3xjw8zpbb5et77vb06jjr-dragdrop-listener-registration-race.md`
  and
  `todos/frontend/settings/01kwg3xjw8zpbb5et77vb06jjs-multi-drop-banner-overwrites-errors.md`.
- CHANGELOG `Changed`: installing from Settings shows the review
  first; several dropped files are reviewed one after another.

At this point the in-app flow is complete and releasable on its own.

### Phase 4: OS entry points (macOS first)

**Step 15: open files from Finder.**
- `intake::paths_from_opened_urls(&[Url])` keeps `file://` URLs and
  logs others. The `RunEvent::Opened` arm submits them with origin
  `OsOpenFile` and shows the settings window.
- Test first: the URL filter (file URLs, non-file schemes, URLs that
  do not map to a path). The run-loop arm itself is covered by the
  manual checklist.
- Deletes `todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md`
  (implemented by steps 9 to 15).

**Step 16: argv and single instance.**
- Register `tauri-plugin-single-instance` first in `run()`. Its
  callback submits `intake::paths_from_args(args, cwd)` with origin
  `CommandLine`, or shows the launcher when there are none. Setup
  submits the first instance's own argv the same way.
- Extract `show_launcher_window` (show-only) from
  `toggle_launcher_window`.
- Test first: argv parsing (skip program name, skip `-` flags, keep
  paths and `file://` URLs, resolve relative paths against cwd);
  `show_launcher_window` does not hide a visible launcher (tested
  through the extracted decision function, since the panel calls are
  platform code).
- Deletes `todos/gadget-host/install/01m399736658afgk6xv4ab68wp-argv-intake-and-single-instance.md`.

**Step 17: register the file type in the bundle.**
- `bundle.fileAssociations` in `tauri.conf.json` with the decided
  identifiers (drives Linux later).
- `src-tauri/Info.plist` with complete `CFBundleDocumentTypes` and
  `UTExportedTypeDeclarations`, adding `CFBundleTypeIconFile` and
  `UTTypeIconFiles` for the document icon.
- Document icon `src-tauri/icons/gadget-document.icns`, derived from
  `app-icon-source.png` by a `just` recipe, bundled via
  `bundle.resources` into `Contents/Resources`.
- Test first: `just verify-bundle-plist`, a script that reads the
  built bundle's `Info.plist` with `plutil -extract` and fails unless
  the document type, UTI, conformance, MIME tag, role, rank and icon
  keys hold the decided values. It runs after `just build` and in the
  release recipe; it fails before the change. (Shell script goes
  through `shellcheck`.)
- CHANGELOG `Added`: `.torchsnap` files open in Torchsnap and show
  the install review.
- Deletes `todos/platform/01m399736658afgk6xv4ab68wq-macos-torchsnap-file-association.md`.

**Step 18: ADRs.**
- New ADR "Install pipeline with staging and review": one queue for
  all origins, staging copy, review always, replace keeps data,
  16 MiB cap, no control-socket origin. Amends ADR 0035 and ADR 0036
  (the review is the install-time consent 0036 left open; signing
  stays deferred).
- New ADR "Register `.torchsnap` as an exported type": identifiers,
  public.data conformance, Viewer/Owner, icon through
  `src-tauri/Info.plist`.

**Step 19: documentation.** Separate commit in `torchsnap-docs`:
- `start/settings.mdx`: install through picker, drop or double-click,
  the review dialog, replace.
- `development/packaging.mdx`: fix the "place the archive in the user
  gadgets directory" instruction, describe the review and the 16 MiB
  limit.

### Later, not in this plan's first delivery

- Linux registration and AppImage self-registration (their todos,
  blocked on Linux packaging).
- The URL scheme (parked todo).

## Manual verification checklist (bundled macOS build)

Run on `just build --release`, copied to `/Applications` and launched
once, after `just verify-bundle-plist` passed.

1. `mdls -name kMDItemContentType x.torchsnap` reports
   `app.torchsnap.gadget`; Finder shows the document icon.
2. App not running: double-click → app starts, settings opens on
   Gadgets, review shows.
3. App running, settings closed: same.
4. Settings open on another section: switches to Gadgets.
5. Three files opened at once: three reviews in a row, one summary,
   one restart prompt.
6. File downloaded with Safari: provenance line shows the host.
7. Replace: install v1, restart, open v2, confirm, restart, data kept.
8. Uninstall then reinstall before restart works.
9. Non-gadget renamed to `.torchsnap`: failed request with a readable
   error.
10. File above 16 MiB: rejected at staging.
11. `Contents/MacOS/<binary> x.torchsnap` while running: request
    arrives in the running instance, no second instance stays up.
12. Launch the binary again without args: launcher shows.
13. Notarized release DMG: same checks 1 and 2.

## Unverified assumptions

| Assumption | Settled by |
|---|---|
| On a cold launch AppKit delivers `openURLs` after `didFinishLaunching`, so `setup` has run. The design does not depend on it. | Checklist item 2. |
| `bundle.resources` can place the `.icns` so `CFBundleTypeIconFile` resolves. | Step 17, `verify-bundle-plist` plus checklist item 1. |
| A Unix-socket single-instance handover works for directly started binaries on macOS. | Checklist item 11. |
| `getxattr` on quarantined downloads works for an unsandboxed app. | Step 8 test plus checklist item 6. |

## Out of scope (tracked elsewhere)

- Decompression caps for archive reads:
  `todos/gadget-host/wasm/01kwh4j9bptrayf451yzd2145g-archive-decompressed-size-unbounded.md`.
  The 16 MiB file cap does not bound decompressed size.
- Loader dedup against builtin ids and the stem/id bijection (fixes 1,
  3, 4 of the trust todo).
- Uninstalling a running gadget deleting live state:
  `todos/gadget-host/install/01kwh2e8mne5bd05tpb4paacwc-uninstall-live-gadget-resurrects-state.md`.
  Replace does not delete state, so it is unaffected.
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

Existing todos this plan resolves: the install/uninstall
restart-blocking todo (step 4), the permissions-in-settings todo
(step 12), the permission-transparency todo (steps 12 and 13), the
install-time consent todo (folded into the review dialog todo), the
drag-drop race and multi-drop banner todos (step 14). The
frontend-test-infrastructure todo is started in step 1.
