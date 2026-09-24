---
kind: feature
status: open
---

# Install-time permission consent prompt for User gadgets

## Context

ADR 0036 deferred a runtime/install-time consent UX, leaving manifest
review at parse time as the sole gate. That was acceptable while gadget
capabilities were limited to URL opening, scoped HTTP fetches, and
read-only data interfaces (`assets`, `frecency`, gadget-namespaced SQL
and settings).

Adding `[[permissions.process]]` raises the trust bar materially.
Spawning system commands — even with argv constraints — is a
categorically larger ask than opening an `https` URL, and an unsigned
`.torchsnap` archive dropped on the install drop-zone should not gain
that capability silently.

## Target

When a gadget from the **User** source (`GadgetSourceKind::User`) is
installed *and* its manifest declares any `[[permissions.process]]`
rule, present a one-shot consent dialog before the file is moved into
`<app_data_dir>/gadgets/`. The dialog must show:

- Gadget display name + version + source path.
- All `[[permissions.process]]` rules in human-readable form (binary +
  argv shape per rule).
- Any HTTP origins requesting `*` and any opener `path-roots`
  requesting `*` (these are similarly broad asks worth surfacing in the
  same prompt for consistency).
- Confirm / cancel buttons. Cancel aborts the install.

System and Dev sources skip the prompt entirely:
- `System` gadgets are bundled with Torchsnap and trusted by definition.
- `Dev` gadgets exist only in debug builds for the developer's own
  workflow.

## Future composition with signing

ADR 0036 also defers extension signing. When that lands, signed gadgets
from a publisher the user has already trusted should be eligible to
skip the prompt (matching the pattern macOS uses for notarized vs.
unsigned apps). The install-time check should therefore be structured
as `requires_consent(manifest, source, signature_status)` rather than a
hardcoded `source == User` branch, so signing can plug in cleanly later.

## Non-goals

- Per-call runtime prompts (e.g., a dialog every time a gadget spawns
  `mdfind`). The trust decision is install-time; runtime is enforced by
  the manifest-derived allowlists.
- Revoking a granted capability after install. See sibling todo
  `*-show-gadget-permissions-in-settings.md` for the read-only display;
  revocation is a separate, larger design question.

## Blocked-by / enables

- Depends on: `[[permissions.process]]` manifest + WIT landing first so
  there is a real surface to consent to.
- Pairs with the settings-panel display todo (sibling file in
  `todos/gadget-host/wasm/`) — same data, different surface.
