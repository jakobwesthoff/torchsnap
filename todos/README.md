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

A review-derived todo keeps its original kind, severity and area in
its frontmatter (see below). `rg '^severity: high' todos/` (or
`critical` / `medium` / `low`) finds findings by severity. Findings
that were resolved or judged obsolete during a 2026-09-23 triage pass
were deleted outright rather than moved.

## Frontmatter

Every todo starts with YAML frontmatter, then a blank line, then the
H1 title. Fields appear in this order, lists are always flow style
(`[a, b]`) so each field stays on one grep-able line, and a field that
does not apply is left out rather than written empty.

```yaml
---
kind: bug
severity: high
status: open
area: [src-tauri/src/caps/command.rs, src-tauri/src/wasm/argv_matcher.rs]
tags: [security, unconfirmed]
plan: todos/plans/<ulid>-<name>.md
depends-on: [todos/<scope>/<ulid>-<name>.md]
---
```

- `kind` (required): `bug`, `feature` (new capability), `improvement`
  (makes existing behavior better), `refactor`, `chore`,
  `investigation`, `question`, `decision`, `docs`, or `plan` (only for
  files in `plans/`).
- `severity` (optional): `critical`, `high`, `medium`, `low`. Only for
  findings that state one.
- `status` (required): `open`, `needs-discussion`, `blocked` (waiting
  on a named todo or external event), `deferred` (parked on purpose,
  no named trigger), `in-progress`. Any nuance goes into the body as
  the first paragraph after the title.
- `area` (optional): repo-relative paths the todo is about.
- `tags` (optional): from the list below only. Do not tag what the
  directory already says (no `macos` under `platform/macos/`, no
  `memory` under `gadget-host/memory/`).
- `plan` (optional): the coordinating file in `plans/`.
- `depends-on` (optional): todos that must land first. Relations that
  are not hard dependencies stay as prose in the body.

Tags:

- Platform: `macos`, `linux`, `windows`.
- Security and trust: `security` (vulnerability, hardening, trust
  boundary), `privacy` (data exposure or disclosure), `unconfirmed`
  (a suspected bug not yet reproduced).
- Correctness and quality: `concurrency`, `error-handling`,
  `performance`, `memory`, `testing`.
- Product: `ux`, `accessibility`, `docs`.
- Infrastructure: `dependencies`, `ci`, `tooling`, `migration` (large
  structural moves such as a framework major version), `api-design`,
  `logging`, `config`.
- Subsystems, when the directory does not already say it: `wasm`,
  `sdk`.

Add a tag to this list before using it anywhere.

## Adding a todo

`<ulid>-<short-description>.md` in the most-specific matching
directory. Use `mkulid -l` to generate the ULID. Start with the
frontmatter above, then the title, then concise content that captures
the topic, any prior discussion, and decisions made. The file is what
carries forward when the conversation that produced it is gone.
