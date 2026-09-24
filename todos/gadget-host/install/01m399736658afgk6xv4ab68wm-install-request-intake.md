---
kind: feature
status: needs-discussion
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
---

# Install request intake: one pipeline for every way a gadget archive arrives

## Why

Gadget archives will soon reach the app from several places, each
with its own delivery mechanism and timing:

| Origin | Delivery | Platforms | Todo |
|---|---|---|---|
| Settings file picker | frontend `invoke` | all | exists (`GadgetsManagementPanel.tsx:102`) |
| Settings drop zone | frontend `invoke` | all | exists (`GadgetsManagementPanel.tsx:119-155`) |
| OS "open file" | `RunEvent::Opened { urls }` | macOS | `platform/01m399736658afgk6xv4ab68wq-*` |
| Command line, first instance | `std::env::args()` in `setup` | Linux, Windows, macOS when the binary runs directly | `gadget-host/install/01m399736658afgk6xv4ab68wp-*` |
| Command line, second instance | single-instance plugin callback `(app, args, cwd)` | all | same |
| `torchsnap://` link | `RunEvent::Opened` on macOS, argv on Linux | all | `product/features/01m399736658afgk6xv4ab68wt-*` |

Wiring each of these straight into `install_gadget_archive` would
repeat buffering, window handling and confirmation in every path. The
point of this todo is one intake that all of them feed, and whose UI
does not care where a request came from beyond showing it.

## Constraints from the current code

- **Nothing may install without a confirm step once the trigger comes
  from outside the app.** In-app picker and drop are deliberate
  gestures. A double-click in Finder is not.
- **The settings window usually does not exist.** It is built on
  demand and destroyed on close (`show_settings_window`,
  `src-tauri/src/lib.rs:279`; close behavior at `:985-988`). It shows
  only after the frontend emits `react-ready` (`lib.rs:253`). A
  fire-and-forget event to it gets lost, which is exactly the bug in
  `todos/backend/settings/01kwh386ce7p2p40xksx8gph3j-open-gadget-settings-event-has-no-listener.md`.
- **Requests can arrive before the app is ready.** On a cold start by
  LaunchServices or with argv, the request can exist before `setup`
  has built `GadgetHost` (`lib.rs:648-981`). When exactly
  `RunEvent::Opened` fires relative to `setup` on a cold launch is
  unverified. The queue has to exist before `setup` runs (created in
  `run()` and moved into both closures) or be tolerant of arriving
  first.
- **Install is restart-based.** `install_impl` returns
  `requires_restart: true` (`src-tauri/src/gadget_install.rs:167-172`).
  With several queued requests, the user should see one restart
  prompt at the end, not one per file.
- **The app has no Dock icon.** The activation policy is `Accessory`
  (`lib.rs:932-935`), so "drop onto the Dock icon" is not an entry
  point on macOS.
- **The install step validates, then copies from the original path**
  (`gadget_install.rs:118` then `:165`). If the user approves what
  they saw and the file changes before the copy, the wrong bytes get
  installed. See fix 2 in
  `todos/gadget-host/install/01kwh4j9bptrayf451yzd2145q-gadget-install-uninstall-trust.md`.

## Proposed shape (for discussion)

Stages, each owned by one piece:

1. **Receive.** Per-origin adapters turn their raw input into
   `InstallRequest`s. Only adapters know about `RunEvent`, argv or
   plugins.
2. **Normalize.** Convert `file://` URLs to paths, resolve relative
   argv paths against the reported cwd, canonicalize, and drop
   anything that is not `*.torchsnap` (with a log line, not
   silently).
3. **Stage.** Copy the archive into app-owned space, e.g.
   `<app_cache_dir>/install-staging/<request-ulid>.torchsnap`, and
   parse it there with `ArchiveSource::open`. Everything after this
   (review, install) works on the staged copy, so what the user
   approved is what lands. A remote URL (the later URL scheme) plugs
   in here as "download into staging" instead of "copy into staging".
4. **Queue.** Managed state, e.g. `InstallQueue`, holds pending
   requests with their parsed manifest summary or their staging
   error. It deduplicates by canonical source path while a request
   is pending. On macOS a cold start could plausibly deliver the same
   file through both argv and `Opened`; to verify.
5. **Present.** Open the settings window on the Gadgets section.
   The frontend **pulls** the queue through a command once mounted
   and listens for a `install-queue-changed` event while open. Pull
   plus notify survives the window not existing yet, which a plain
   event does not.
6. **Confirm.** The minimal step for the basic system: gadget name,
   version, id, source (path, later URL), Install / Cancel. The full
   review is
   `todos/gadget-host/install/01m399736658afgk6xv4ab68wn-install-review-dialog.md`.
7. **Install.** Run the existing install logic against the staged
   file, record a per-request result, and delete the staged file on
   success, cancel or app exit.

Sketch of the core types, names open:

```rust
struct InstallRequest {
    id: Ulid,
    origin: InstallOrigin,
    source: InstallSource,
    received_at: SystemTime,
}

enum InstallOrigin {
    SettingsPicker,
    SettingsDrop,
    OsOpenFile,
    CommandLine,
    UrlScheme,
}

enum InstallSource {
    LocalFile(PathBuf),
    // Added with the URL scheme todo.
    Remote(Url),
}
```

## Open discussion points

- **Should the in-app picker and drop zone go through the queue and
  the confirm step as well?** Pro: one flow, one code path, and the
  review dialog applies everywhere. Con: an extra click after a
  deliberate action. I lean towards yes, because the review dialog
  exists to show permissions, and those matter no matter how the
  file arrived.
- **Where does the UI live?** Options are a modal in Settings →
  Gadgets, where the install UI already is, or a small dedicated
  "Install gadget" window. The settings window is 720×520 and brings
  the whole settings app along. A dedicated window is more focused
  and would suit URL-scheme installs too. It is also one more
  window type to maintain.
- **Queue behavior with several files.** One dialog per file in
  sequence, or one list with a checkbox per file? Multi-select
  double-click in Finder delivers all URLs in one `Opened` event.
- **Where to stage.** `app_cache_dir` can be wiped by the OS, which is
  fine for transient data, but the request has to fail cleanly if its
  staged file disappears. Clean up leftovers at startup.
- **Size limits at staging.** A size check at copy time is the
  natural place for the install-time cap discussed in
  `todos/gadget-host/wasm/01kwh4j9bptrayf451yzd2145g-archive-decompressed-size-unbounded.md`.
- **What happens when the id already exists?** See the upgrade
  question in the plan. At minimum the confirm step should tell the
  user before they click Install, instead of failing after.
- **Where the line sits between the minimal confirm and the full
  review dialog.** The proposal here is that the minimal step shows
  identity and source only, with no permissions. If the full dialog
  follows soon anyway, building the minimal one could be skipped and
  the review dialog made part of the basic system.
- **Control socket as an origin?** The JSON-RPC control socket has no
  authentication
  (`todos/backend/control/01kwh4j9bptrayf451yzd2145v-control-socket-no-auth.md`).
  Do not add an install command there until that is resolved.

## Done when

- All in-app and OS entry points create `InstallRequest`s and nothing
  calls the install logic directly.
- Cold start, warm start, and settings window open or closed all end
  with the request visible to the user.
- Cancel leaves no staged file behind. Install works on the staged
  copy.
- Tests cover normalization (non-torchsnap, `file://` URLs, relative
  paths), dedup, and the staged-copy install.
