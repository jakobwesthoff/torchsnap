# Todos

Each file is a single piece of deferred work — a feature, a refactor,
a bug, or an investigation note. Files are named
`<ulid>-<short-description>.md`; the ULID prefix gives a stable,
chronologically-sortable identifier and **must be preserved** on any
move.

## Structure

Todos are grouped by **scope** — "what subsystem do I open the
editor in when I work on this?" — rather than by kind (feature /
refactor / bug) or lifecycle stage (open / blocked / deferred).
Scope-based grouping makes it easy to pick up adjacent work when
one is already in the relevant area of the codebase; kind and stage
are recorded inside each todo file where they don't constrain the
directory tree.

Top-level groups:

- `gadgets/` — work scoped to a single gadget, WASM or built-in
  (`gadgets/clipboard/`, `gadgets/zerotier/`, ...).
- `gadget-host/` — cross-cutting gadget-runtime work: host APIs
  (`api/`), the WASM runtime (`wasm/`), gadget capability
  implementations (`caps/`), install/uninstall lifecycle
  (`install/`), the SDKs (`sdk/`), memory/footprint tuning
  (`memory/`), architectural decisions about the gadget-host
  design (`architecture/`), and audits.
- `frontend/` — UI / React-side work.
- `backend/` — Rust / Tauri host-side work that isn't
  gadget-runtime (settings, storage, search/app-discovery,
  control socket, metadata/favicon caching, ...).
- `platform/` — OS-specific bugs and integrations.
- `product/` — user-facing features and visual identity that
  cross frontend and backend.
- `build/` — build pipeline, CI, and developer-experience tooling.
- `chores/` — small, opportunistic cleanups with no clear home.
- `plans/` — multi-step coordination documents that span several
  todos.

Subdirectories under each group narrow scope further (e.g.
`gadgets/clipboard/`, `backend/search/`). Add a new subdirectory
when three or more related todos accumulate; before that, keep
them at the parent level.

`memory/` and `architecture/` used to be their own top-level
groups; both were folded into `gadget-host/` (as `gadget-host/memory/`
and `gadget-host/architecture/`) since almost everything in them is
gadget-host-scoped.

## Where the 2026-07-02 review findings live

A full-codebase review on 2026-07-02 filed its findings under a
separate `todos/2026-07-02-review/` batch, grouped by review
operation rather than by scope. That batch has been dissolved: every
finding is now an ordinary todo in its scope folder above, alongside
non-review todos on the same topic. There is no separate "review"
category to browse.

A review-derived todo still carries its original **Kind** /
**Severity** / **Area** header (e.g. `**Severity:** high`); grep for
`Severity:.*high` (or `medium` / `low`) under `todos/` to find
findings by severity. Findings that were resolved or judged obsolete
during a 2026-09-23 triage pass were deleted outright rather than
moved; a handful of remaining todos note this next to the dangling
reference they used to point at (e.g. "done, removed 2026-09-23").

## Adding a todo

`<ulid>-<short-description>.md` in the most-specific matching
directory. Use `mkulid -l` to generate the ULID. Concise content
that captures the topic, any prior discussion, and decisions
made — the file is what carries forward when the conversation
that produced it is gone.
