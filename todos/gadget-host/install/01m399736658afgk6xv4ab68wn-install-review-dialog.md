---
kind: feature
status: open
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
depends-on: [todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md]
---

# Install review dialog: show what a gadget asks for before installing it

Part of the first delivery (decided 2026-09-24). Implementation steps
7, 8, 12 and 13 of the plan.

## Why

Gadgets can spawn processes (`command` rules), make HTTP requests,
read files and open URLs and paths. A file opened by double-click from
Downloads is exactly where the user needs to see that before saying
yes. The dialog replaces a plain confirm step entirely.

## Decisions

- **Always shown**, for every install request and every origin.
- **Content:** name, version, id, description; the source path;
  download provenance on macOS when available ("Downloaded from
  github.com"); a replace notice with installed and incoming version;
  every declared permission, grouped, in plain language, with broad
  grants highlighted; Install / Cancel. A rejected request (builtin,
  system or dev id) shows the reason and only a dismiss action.
- **Severity rules** (Rust, `review.rs`): any `command` rule, HTTP
  origin `"*"` and `opener.open_path` are warnings; other HTTP
  origins, opener schemes, `reveal_path`, filesystem read patterns,
  clipboard and website metadata are notices; settings, frecency, SQL
  storage, icon cache and path resolver are info. The table in the
  plan is authoritative.
- **Command rules show binary and argv only.** Per-rule limits are not
  enforced yet
  (`todos/gadget-host/caps/01kwg1ajrvfsmxyjmm7bvcs04b-command-per-rule-limits-unenforced.md`),
  so the review does not show them.
- **Split of work:** Rust returns structured `PermissionItem`s with a
  severity. The frontend turns them into sentences, so wording can
  change (or be translated later) without touching the contract.
- **One shared component** (`PermissionSummary`) renders permissions
  in the dialog and on installed gadget cards in Settings → Gadgets.
- **Provenance** comes from the `com.apple.metadata:kMDItemWhereFroms`
  xattr (binary plist array of strings) and `com.apple.quarantine`
  (`flags;timestamp;agent;uuid`), read with `libc::getxattr`. Missing
  attributes are skipped silently. Other platforms show no provenance.
- **Signing** stays deferred (ADR 0036). The earlier consent todo
  asked for a `requires_consent(manifest, source, signature_status)`
  shape so trusted signers could skip the prompt later. With the
  review always shown, signing would instead add a line to the review
  ("Signed by …"); the review model gets that field when signing
  exists.

## Folded-in todos

- `todos/gadget-host/wasm/01kq7x2ge7d3ykf7vxkz4fvr6a-install-time-permission-consent.md`
  (deleted, content above).
- Section 2 of
  `todos/product/features/01krp751n5tddffjtb8fr7nnpr-permission-ui-transparency.md`;
  its section 1 and
  `todos/gadget-host/wasm/01kq7x2ge7d3ykf7vxkz4fvr69-show-gadget-permissions-in-settings.md`
  are covered by the shared component (plan step 12).

## Out of scope

- Remembering decisions per gadget or publisher.
- Revoking permissions of installed gadgets.
