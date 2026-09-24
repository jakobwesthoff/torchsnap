---
kind: plan
status: needs-discussion
---

# Open `.torchsnap` archives from outside the app

The intake abstraction is to be discussed in detail before any implementation starts.

## Goal

A user double-clicks a `.torchsnap` file (or uses "Open With", runs a
command, or clicks a link) and Torchsnap takes it from there: it shows
what is about to be installed, asks, and installs. Today the only
entry points are the file picker and the drop zone in Settings →
Gadgets (`src/settings/sections/GadgetsManagementPanel.tsx`, picker at
`:102`, drop handling at `:119-155`, `runInstall` at `:388`).

macOS is the first target. Linux must fit the same design without a
rework, even though no Linux packaging exists yet
(`torchsnap-docs/src/content/docs/start/installation.mdx` lists
AppImage and `.deb` as planned).

## Decisions so far (2026-09-24)

- **OS file association first** (Tauri `bundle.fileAssociations`).
  This was option 1 of four considered: file association, custom URL
  scheme, Linux runtime self-registration, plain argv.
- **One intake abstraction for every entry point.** It gets designed
  up front so file association, argv, the URL scheme and the existing
  in-app picker and drop zone all feed the same pipeline. See the
  intake todo.
- **argv handling may land together with the macOS work.** It is
  cross-platform code, needed on Linux anyway, and cheap once the
  intake exists.
- **The custom URL scheme is deferred.** It needs download handling,
  validation and its own trust decisions. Fully written up in its own
  todo.
- **The confirmation work is split in two.** The basic system ships
  with a minimal confirm step, because a double-click must never
  install silently. The full review dialog (permissions, warnings,
  origin details) is a separate todo. Where exactly the line sits is
  an open point in the intake todo.
- **Linux runtime self-registration** is an add-on to the Linux
  implementation, only relevant for AppImage.

## Todos in this plan

Suggested order:

1. `todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md`
   is the shared pipeline and its minimal confirm step. Design
   discussion comes first.
2. `todos/platform/01m399736658afgk6xv4ab68wq-macos-torchsnap-file-association.md`
   covers the Info.plist registration and `RunEvent::Opened`.
3. `todos/gadget-host/install/01m399736658afgk6xv4ab68wp-argv-intake-and-single-instance.md`
   covers command-line paths and forwarding from a second instance.
   It can ship with step 2.
4. `todos/gadget-host/install/01m399736658afgk6xv4ab68wn-install-review-dialog.md`
   is the full review dialog. It can follow right after step 2, and
   the URL scheme must not ship without it.
5. `todos/platform/linux/01m399736658afgk6xv4ab68wr-linux-torchsnap-file-association.md`
   is the deb/rpm registration. It is blocked on Linux packaging.
6. `todos/platform/linux/01m399736658afgk6xv4ab68ws-linux-runtime-mime-self-registration.md`
   covers AppImage and only matters if AppImage ships.
7. `todos/product/features/01m399736658afgk6xv4ab68wt-torchsnap-url-scheme-install.md`
   is the `torchsnap://` install link. It needs an ADR first.

## Open questions that span several todos

- **Type identifiers.** The UTI (e.g. `app.torchsnap.gadget`) and the
  MIME type (e.g. `application/x-torchsnap-gadget` or
  `application/vnd.torchsnap.gadget+zip`) must be picked once and
  shared by macOS and Linux. Once shipped they are hard to change,
  because other apps and user defaults refer to them.
- **Zip conformance.** Should the type declare itself a zip
  (`public.zip-archive` on macOS, `sub-class-of application/zip` on
  Linux)? Doing so makes archive tools offer to open it and lets
  Quick Look treat it as a zip. Declaring plain data keeps it opaque.
  I lean towards plain data because users have no reason to unpack a
  gadget, but it is not decided.
- **Upgrade by double-click.** Opening a newer version of an
  installed gadget is the obvious way to update it. Today install
  rejects any existing id ("Uninstall the existing version, then
  retry", `src-tauri/src/gadget_install.rs:137-139`), and
  uninstall-then-install is broken until restart
  (`todos/gadget-host/install/01kwh2e8mne5bd05tpb4paacwd-install-uninstall-blocked-until-restart.md`).
  Decide whether "replace" belongs in this plan or stays an error.

## Records and docs to update when this lands

- ADR for the file association and type identifiers. ADR 0035 covers
  distribution and 0036 the trust model. The URL scheme needs its own
  ADR, because it is a new distribution channel and 0036 names that
  as a revisit trigger.
- `torchsnap-docs/src/content/docs/start/settings.mdx:94` describes
  install as drop zone or file picker only.
- `torchsnap-docs/src/content/docs/development/packaging.mdx:132-163`
  already tells users to "place its `.torchsnap` archive in the user
  gadgets directory", which is not what the app does. Fix that while
  touching the page.
- `CHANGELOG.md` entry under `[Unreleased]`.

## Existing todos this touches

- `todos/gadget-host/install/01kwh4j9bptrayf451yzd2145q-gadget-install-uninstall-trust.md`:
  the validate-then-copy TOCTOU and the "validate what you publish"
  reorder. This matters more once files arrive from Downloads.
- `todos/gadget-host/wasm/01kwh4j9bptrayf451yzd2145g-archive-decompressed-size-unbounded.md`:
  a zip-bomb archive opened by double-click boot-loops the host just
  like a dropped one.
- `todos/gadget-host/wasm/01kq7x2ge7d3ykf7vxkz4fvr6a-install-time-permission-consent.md`
  and section 2 of
  `todos/product/features/01krp751n5tddffjtb8fr7nnpr-permission-ui-transparency.md`:
  both are folded into the review dialog todo.
- `todos/backend/settings/01kwh386ce7p2p40xksx8gph3j-open-gadget-settings-event-has-no-listener.md`:
  it has the same "event emitted before the settings window exists"
  problem the intake has to solve.
- `todos/frontend/settings/01kwg3xjw8zpbb5et77vb06jjs-multi-drop-banner-overwrites-errors.md`:
  per-file results for multi-file opens.
- `todos/gadget-host/wasm/01kpdsvj5at6agxst1jva5eeva-gadget-hot-lifecycle.md`:
  without it, every install still ends in "restart required".
