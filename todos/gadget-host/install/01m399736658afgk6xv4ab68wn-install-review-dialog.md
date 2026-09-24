# Install review dialog: show what a gadget asks for before installing it

**Kind:** feature
**Status:** deferred until the intake exists. Required before the URL
scheme ships.
**Plan:** `todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md`

## Why

The intake todo
(`todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md`)
gives every external install a minimal confirm step: name, version,
id and source. That stops silent installs. It does not tell the user
what they are agreeing to.

Gadgets can now spawn processes (`[[permissions.process]]`), make
HTTP requests, read files and open URLs. A file opened by
double-click from Downloads, or later fetched through a `torchsnap://`
link, is exactly where the user needs to see that before saying yes.

## Existing todos this replaces

Two todos describe the same dialog from different angles. Fold them
into this one when work starts and delete or trim the originals:

- `todos/gadget-host/wasm/01kq7x2ge7d3ykf7vxkz4fvr6a-install-time-permission-consent.md`
  wants a consent prompt for User gadgets with process permissions,
  broad HTTP origins and opener path roots. It asks for a
  `requires_consent(manifest, source, signature_status)` shape so
  signing can plug in later (ADR 0036 defers signing).
- `todos/product/features/01krp751n5tddffjtb8fr7nnpr-permission-ui-transparency.md`,
  section 2, wants a permission review in plain language ("Can read
  files matching ~/.config/myapp/*" rather than the glob).

Section 1 of the transparency todo and
`todos/gadget-host/wasm/01kq7x2ge7d3ykf7vxkz4fvr69-show-gadget-permissions-in-settings.md`
cover showing permissions of already-installed gadgets. That is the
same rendering, so build one permission-summary component and use it
in both places.

## What the dialog shows

- Identity: name, version, id, author if the manifest carries one,
  description.
- Source: the local path, or for URL-scheme installs the full URL
  with the host shown prominently.
- Permissions, grouped and in plain language. Broad grants get a
  warning: HTTP origin `*`, process rules with free-form argv, opener
  path roots `*`.
- A collision notice if the id is already installed, before Install
  is clicked.
- Install / Cancel. Cancel discards the staged file.

## Open discussion points

- **Always show it, or only when something is risky?** The consent
  todo proposed prompting only for process permissions. Once every
  install passes a confirm step anyway, showing the full review every
  time costs nothing extra and is easier to explain.
- **Should the review vary by origin?** A URL-scheme install arrives
  with less user intent than a drop, so its dialog may need a
  stronger warning or an extra "I trust this source" step.
- **Download provenance on macOS.** Files downloaded by a browser
  carry `com.apple.quarantine` and usually `kMDItemWhereFroms` (the
  download URL). Showing "downloaded from github.com" for a local file
  would help the user. Unverified whether this can be read cheaply
  from Rust; `xattr` / `mdls` show it on the command line.
- **Where the permission-to-text mapping lives.** In Rust, next to the
  manifest types (one source of truth, testable), or in the frontend.
  Rust returning structured data plus severity, with the frontend
  wording it, is one option.
- **Remembering decisions.** Out of scope for now. Every install asks.

## Done when

- Every request in the install queue shows this dialog before
  installing.
- The same permission component is used on installed gadget cards in
  Settings → Gadgets.
- The two folded-in todos are removed or reduced to what is not
  covered here.
