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

- `gadgets/` — work scoped to a single gadget.
- `gadget-host/` — cross-cutting gadget-runtime work (host APIs,
  the WASM runtime, the SDKs, audits).
- `frontend/` — UI / React-side work.
- `backend/` — Rust / Tauri host-side work that isn't
  gadget-runtime.
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

## Adding a todo

`<ulid>-<short-description>.md` in the most-specific matching
directory. Use `mkulid -l` to generate the ULID. Concise content
that captures the topic, any prior discussion, and decisions
made — the file is what carries forward when the conversation
that produced it is gone.
